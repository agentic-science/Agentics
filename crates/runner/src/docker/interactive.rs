use std::pin::Pin;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{AttachContainerOptionsBuilder, StartContainerOptions};
use futures::{Stream, StreamExt};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::{Instant as TokioInstant, sleep_until, timeout};

use super::{
    ContainerOutcome, InteractiveSessionOutcome, PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
    duration_millis, kill_container_if_running, wait_container_exit,
};
use crate::docker::options::append_bounded_log_bytes;
use agentics_error::{Result, ServiceError};

/// Run two already-created containers with attached and crossed stdio streams.
pub(super) async fn run_attached_interactive_pair(
    docker: &Docker,
    participant_id: &str,
    interactive_evaluator_id: &str,
    timeout_sec: u64,
    max_interaction_bytes_per_direction: u64,
    shutdown_grace_secs: u64,
) -> Result<InteractiveSessionOutcome> {
    let participant_attach = attach_container_stdio(docker, participant_id).await?;
    let interactive_evaluator_attach =
        attach_container_stdio(docker, interactive_evaluator_id).await?;

    docker
        .start_container(participant_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| ServiceError::Docker(format!("start participant container failed: {e}")))?;
    docker
        .start_container(interactive_evaluator_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| {
            ServiceError::Docker(format!("start interactive-evaluator container failed: {e}"))
        })?;

    let started = Instant::now();
    let participant_output = participant_attach.output;
    let participant_input = participant_attach.input;
    let interactive_evaluator_output = interactive_evaluator_attach.output;
    let interactive_evaluator_input = interactive_evaluator_attach.input;
    let kill_switch = InteractiveKillSwitch {
        docker: docker.clone(),
        participant_id: participant_id.to_string(),
        interactive_evaluator_id: interactive_evaluator_id.to_string(),
    };

    let participant_pump = pump_attached_output(
        "participant",
        participant_output,
        interactive_evaluator_input,
        max_interaction_bytes_per_direction,
        PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
        kill_switch.clone(),
    );
    let interactive_evaluator_pump = pump_attached_output(
        "interactive-evaluator",
        interactive_evaluator_output,
        participant_input,
        max_interaction_bytes_per_direction,
        PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
        kill_switch,
    );
    let participant_wait = wait_container_exit(docker, participant_id);
    let interactive_evaluator_wait = wait_container_exit(docker, interactive_evaluator_id);
    let session_timeout = tokio::time::sleep(Duration::from_secs(timeout_sec));
    tokio::pin!(participant_pump);
    tokio::pin!(interactive_evaluator_pump);
    tokio::pin!(participant_wait);
    tokio::pin!(interactive_evaluator_wait);
    tokio::pin!(session_timeout);

    let mut participant_pump_state = AttachedPumpState::Pending;
    let mut interactive_evaluator_pump_state = AttachedPumpState::Pending;
    let mut participant_exit = None;
    let mut interactive_evaluator_exit = None;
    let mut participant_grace_deadline = None;
    let mut terminal = None;

    loop {
        if terminal.is_some()
            || (participant_exit.is_some() && interactive_evaluator_exit.is_some())
        {
            break;
        }

        tokio::select! {
            result = &mut participant_pump, if participant_pump_state.is_pending() => {
                match result {
                    Ok(outcome) => participant_pump_state = AttachedPumpState::Completed(outcome),
                    Err(error) => {
                        participant_pump_state = AttachedPumpState::Failed;
                        terminal = Some(InteractiveTerminal::Error(error));
                    }
                }
            }
            result = &mut interactive_evaluator_pump, if interactive_evaluator_pump_state.is_pending() => {
                match result {
                    Ok(outcome) => interactive_evaluator_pump_state = AttachedPumpState::Completed(outcome),
                    Err(error) => {
                        interactive_evaluator_pump_state = AttachedPumpState::Failed;
                        terminal = Some(InteractiveTerminal::Error(error));
                    }
                }
            }
            result = &mut participant_wait, if participant_exit.is_none() => {
                match result {
                    Ok(exit_code) => participant_exit = Some(exit_code),
                    Err(error) => terminal = Some(InteractiveTerminal::Error(error)),
                }
            }
            result = &mut interactive_evaluator_wait, if interactive_evaluator_exit.is_none() => {
                match result {
                    Ok(exit_code) => {
                        interactive_evaluator_exit = Some(exit_code);
                        if exit_code == 0 && participant_exit.is_none() {
                            participant_grace_deadline =
                                match TokioInstant::now()
                                    .checked_add(Duration::from_secs(shutdown_grace_secs))
                                {
                                    Some(deadline) => Some(deadline),
                                    None => {
                                        terminal =
                                            Some(InteractiveTerminal::Error(ServiceError::Docker(
                                                "interactive participant shutdown grace duration overflowed"
                                                    .to_string(),
                                            )));
                                        None
                                    }
                                };
                        }
                    }
                    Err(error) => terminal = Some(InteractiveTerminal::Error(error)),
                }
            }
            () = async {
                match participant_grace_deadline {
                    Some(deadline) => sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            }, if participant_grace_deadline.is_some() && participant_exit.is_none() => {
                kill_container_if_running(docker, participant_id).await?;
                participant_exit = Some(0);
            }
            () = &mut session_timeout => {
                terminal = Some(InteractiveTerminal::Timeout);
            }
        }
    }

    let pump_timeout = Duration::from_secs(shutdown_grace_secs);
    match terminal {
        Some(InteractiveTerminal::Timeout) => {
            kill_interactive_pair(docker, participant_id, interactive_evaluator_id).await?;
            let (participant_pump, interactive_evaluator_pump) = finish_attached_pump_pair(
                participant_pump_state,
                participant_pump.as_mut(),
                interactive_evaluator_pump_state,
                interactive_evaluator_pump.as_mut(),
                pump_timeout,
            )
            .await?;

            let wall_time_ms = duration_millis(started.elapsed());
            return Ok(InteractiveSessionOutcome {
                participant: ContainerOutcome {
                    exit_code: 124,
                    logs: participant_pump.logs,
                    timed_out: true,
                    wall_time_ms,
                },
                interactive_evaluator: ContainerOutcome {
                    exit_code: 124,
                    logs: interactive_evaluator_pump.logs,
                    timed_out: true,
                    wall_time_ms,
                },
            });
        }
        Some(InteractiveTerminal::Error(error)) => {
            let original_message = error.to_string();
            let kill_result =
                kill_interactive_pair(docker, participant_id, interactive_evaluator_id).await;
            let drain_result = drain_attached_pumps_for_cleanup(
                participant_pump_state,
                participant_pump.as_mut(),
                interactive_evaluator_pump_state,
                interactive_evaluator_pump.as_mut(),
                pump_timeout,
            )
            .await;
            if let Err(cleanup_error) = kill_result {
                return Err(ServiceError::Docker(format!(
                    "{original_message}; additionally failed to stop interactive containers: {cleanup_error}"
                )));
            }
            if let Err(cleanup_error) = drain_result {
                return Err(ServiceError::Docker(format!(
                    "{original_message}; additionally failed to finish interactive stdio pumps: {cleanup_error}"
                )));
            }
            return Err(error);
        }
        None => {}
    }

    let interactive_evaluator_exit = interactive_evaluator_exit.ok_or_else(|| {
        ServiceError::Docker("interactive-evaluator container exited without status".to_string())
    })?;
    let participant_exit = participant_exit.unwrap_or(0);
    let effective_participant_exit = if interactive_evaluator_exit == 0 {
        0
    } else {
        participant_exit
    };
    let (participant_pump, interactive_evaluator_pump) = finish_attached_pump_pair(
        participant_pump_state,
        participant_pump.as_mut(),
        interactive_evaluator_pump_state,
        interactive_evaluator_pump.as_mut(),
        pump_timeout,
    )
    .await?;

    let wall_time_ms = duration_millis(started.elapsed());
    Ok(InteractiveSessionOutcome {
        participant: ContainerOutcome {
            exit_code: effective_participant_exit,
            logs: participant_pump.logs,
            timed_out: false,
            wall_time_ms,
        },
        interactive_evaluator: ContainerOutcome {
            exit_code: interactive_evaluator_exit,
            logs: interactive_evaluator_pump.logs,
            timed_out: false,
            wall_time_ms,
        },
    })
}

enum AttachedPumpState {
    Pending,
    Completed(AttachedPumpOutcome),
    Failed,
}

impl AttachedPumpState {
    fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

enum InteractiveTerminal {
    Timeout,
    Error(ServiceError),
}

/// Kill both containers in an interactive pair, preserving both errors if both fail.
async fn kill_interactive_pair(
    docker: &Docker,
    participant_id: &str,
    interactive_evaluator_id: &str,
) -> Result<()> {
    let (participant, interactive_evaluator) = tokio::join!(
        kill_container_if_running(docker, participant_id),
        kill_container_if_running(docker, interactive_evaluator_id)
    );
    match (participant, interactive_evaluator) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(participant_error), Err(interactive_evaluator_error)) => {
            Err(ServiceError::Docker(format!(
                "{participant_error}; additionally failed to stop interactive-evaluator container: {interactive_evaluator_error}"
            )))
        }
    }
}

/// Finish one attached stdio pump and convert timeout failures into Docker errors.
async fn finish_attached_pump_future<F>(
    label: &'static str,
    state: AttachedPumpState,
    pump: Pin<&mut F>,
    pump_timeout: Duration,
) -> Result<AttachedPumpOutcome>
where
    F: Future<Output = Result<AttachedPumpOutcome>>,
{
    match state {
        AttachedPumpState::Pending => timeout(pump_timeout, pump)
            .await
            .map_err(|_| ServiceError::Docker(format!("{label} stdio pump did not stop")))?,
        AttachedPumpState::Completed(outcome) => Ok(outcome),
        AttachedPumpState::Failed => Err(ServiceError::Docker(format!(
            "{label} stdio pump failed before shutdown"
        ))),
    }
}

/// Finish both attached pumps concurrently and return their collected logs.
async fn finish_attached_pump_pair<F, G>(
    participant_state: AttachedPumpState,
    participant_pump: Pin<&mut F>,
    interactive_evaluator_state: AttachedPumpState,
    interactive_evaluator_pump: Pin<&mut G>,
    pump_timeout: Duration,
) -> Result<(AttachedPumpOutcome, AttachedPumpOutcome)>
where
    F: Future<Output = Result<AttachedPumpOutcome>>,
    G: Future<Output = Result<AttachedPumpOutcome>>,
{
    let participant = finish_attached_pump_future(
        "participant",
        participant_state,
        participant_pump,
        pump_timeout,
    );
    let interactive_evaluator = finish_attached_pump_future(
        "interactive-evaluator",
        interactive_evaluator_state,
        interactive_evaluator_pump,
        pump_timeout,
    );
    let (participant, interactive_evaluator) = tokio::join!(participant, interactive_evaluator);
    Ok((participant?, interactive_evaluator?))
}

/// Finish all still-pending pumps after a session-level error.
async fn drain_attached_pumps_for_cleanup<F, G>(
    participant_state: AttachedPumpState,
    participant_pump: Pin<&mut F>,
    interactive_evaluator_state: AttachedPumpState,
    interactive_evaluator_pump: Pin<&mut G>,
    pump_timeout: Duration,
) -> Result<()>
where
    F: Future<Output = Result<AttachedPumpOutcome>>,
    G: Future<Output = Result<AttachedPumpOutcome>>,
{
    let participant = drain_attached_pump_for_cleanup(
        "participant",
        participant_state,
        participant_pump,
        pump_timeout,
    );
    let interactive_evaluator = drain_attached_pump_for_cleanup(
        "interactive-evaluator",
        interactive_evaluator_state,
        interactive_evaluator_pump,
        pump_timeout,
    );
    let (participant, interactive_evaluator) = tokio::join!(participant, interactive_evaluator);
    participant?;
    interactive_evaluator?;
    Ok(())
}

/// Drain one pending pump for cleanup; completed or failed pumps need no more work.
async fn drain_attached_pump_for_cleanup<F>(
    label: &'static str,
    state: AttachedPumpState,
    pump: Pin<&mut F>,
    pump_timeout: Duration,
) -> Result<()>
where
    F: Future<Output = Result<AttachedPumpOutcome>>,
{
    match state {
        AttachedPumpState::Pending => {
            timeout(pump_timeout, pump)
                .await
                .map_err(|_| ServiceError::Docker(format!("{label} stdio pump did not stop")))??;
            Ok(())
        }
        AttachedPumpState::Completed(_) | AttachedPumpState::Failed => Ok(()),
    }
}

/// Attach to stdin/stdout/stderr for one interactive container.
async fn attach_container_stdio(
    docker: &Docker,
    container_id: &str,
) -> Result<bollard::container::AttachContainerResults> {
    let options = AttachContainerOptionsBuilder::default()
        .stream(true)
        .stdin(true)
        .stdout(true)
        .stderr(true)
        .build();
    docker
        .attach_container(container_id, Some(options))
        .await
        .map_err(|e| ServiceError::Docker(format!("attach container failed: {e}")))
}

/// Outcome from pumping one attached output stream into the opposite stdin.
struct AttachedPumpOutcome {
    logs: String,
}

/// Allows a stdio pump to stop both containers immediately on protocol-limit failure.
#[derive(Clone)]
struct InteractiveKillSwitch {
    docker: Docker,
    participant_id: String,
    interactive_evaluator_id: String,
}

impl InteractiveKillSwitch {
    /// Best-effort kill both sides of an interactive session.
    async fn kill_both(&self) {
        drop(kill_container_if_running(&self.docker, &self.participant_id).await);
        drop(kill_container_if_running(&self.docker, &self.interactive_evaluator_id).await);
    }
}

/// Pump stdout to the peer stdin while capturing only stderr into bounded logs.
async fn pump_attached_output(
    label: &'static str,
    mut output: Pin<
        Box<dyn Stream<Item = std::result::Result<LogOutput, bollard::errors::Error>> + Send>,
    >,
    mut peer_input: Pin<Box<dyn AsyncWrite + Send>>,
    max_interaction_bytes: u64,
    log_cap_bytes: u64,
    kill_switch: InteractiveKillSwitch,
) -> Result<AttachedPumpOutcome> {
    let mut relayed = 0u64;
    let mut logs = Vec::new();
    let mut logs_truncated = false;
    let log_limit = usize::try_from(log_cap_bytes).unwrap_or(usize::MAX);

    while let Some(chunk) = output.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message }) | Ok(LogOutput::Console { message }) => {
                let chunk_len = u64::try_from(message.len()).unwrap_or(u64::MAX);
                relayed = relayed.checked_add(chunk_len).ok_or_else(|| {
                    ServiceError::Docker(format!("{label} interaction byte count overflowed"))
                })?;
                if relayed > max_interaction_bytes {
                    drop(peer_input.shutdown().await);
                    kill_switch.kill_both().await;
                    return Err(ServiceError::Docker(format!(
                        "{label} interaction output exceeded {max_interaction_bytes} bytes"
                    )));
                }
                peer_input.write_all(&message).await.map_err(|e| {
                    ServiceError::Docker(format!("write {label} interaction bytes failed: {e}"))
                })?;
            }
            Ok(LogOutput::StdErr { message }) => {
                append_bounded_log_bytes(&mut logs, &message, log_limit, &mut logs_truncated);
            }
            Ok(LogOutput::StdIn { .. }) => {}
            Err(error) => {
                drop(peer_input.shutdown().await);
                return Err(ServiceError::Docker(format!(
                    "read {label} attached output failed: {error}"
                )));
            }
        }
    }

    peer_input
        .shutdown()
        .await
        .map_err(|e| ServiceError::Docker(format!("shutdown {label} peer stdin failed: {e}")))?;
    let mut logs = String::from_utf8_lossy(&logs).into_owned();
    if logs_truncated {
        logs.push_str(&format!(
            "\n[agentics] {label} stderr truncated at {log_cap_bytes} bytes\n"
        ));
    }
    Ok(AttachedPumpOutcome { logs })
}
