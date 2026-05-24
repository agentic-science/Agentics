use std::pin::Pin;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{AttachContainerOptionsBuilder, StartContainerOptions};
use futures::{Stream, StreamExt};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use super::{
    ContainerOutcome, InteractiveSessionOutcome, PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
    duration_millis, kill_container_if_running, wait_container_exit,
};
use crate::docker::options::append_bounded_log_bytes;
use agentics_domain::error::{Result, ServiceError};

/// Run two already-created containers with attached and crossed stdio streams.
pub(super) async fn run_attached_interactive_pair(
    docker: &Docker,
    participant_id: &str,
    interactor_id: &str,
    timeout_sec: u64,
    max_interaction_bytes_per_direction: u64,
    shutdown_grace_secs: u64,
) -> Result<InteractiveSessionOutcome> {
    let participant_attach = attach_container_stdio(docker, participant_id).await?;
    let interactor_attach = attach_container_stdio(docker, interactor_id).await?;

    docker
        .start_container(participant_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| ServiceError::Docker(format!("start participant container failed: {e}")))?;
    docker
        .start_container(interactor_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| ServiceError::Docker(format!("start interactor container failed: {e}")))?;

    let started = Instant::now();
    let participant_output = participant_attach.output;
    let participant_input = participant_attach.input;
    let interactor_output = interactor_attach.output;
    let interactor_input = interactor_attach.input;
    let kill_switch = InteractiveKillSwitch {
        docker: docker.clone(),
        participant_id: participant_id.to_string(),
        interactor_id: interactor_id.to_string(),
    };

    let participant_pump = pump_attached_output(
        "participant",
        participant_output,
        interactor_input,
        max_interaction_bytes_per_direction,
        PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
        kill_switch.clone(),
    );
    let interactor_pump = pump_attached_output(
        "interactor",
        interactor_output,
        participant_input,
        max_interaction_bytes_per_direction,
        PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
        kill_switch,
    );
    let wait_pair = async {
        let (participant, interactor) = tokio::join!(
            wait_container_exit(docker, participant_id),
            wait_container_exit(docker, interactor_id)
        );
        Ok::<_, ServiceError>((participant?, interactor?))
    };
    let session_timeout = tokio::time::sleep(Duration::from_secs(timeout_sec));
    tokio::pin!(participant_pump);
    tokio::pin!(interactor_pump);
    tokio::pin!(wait_pair);
    tokio::pin!(session_timeout);

    let mut participant_pump_state = AttachedPumpState::Pending;
    let mut interactor_pump_state = AttachedPumpState::Pending;
    let mut exits = None;
    let mut wait_pending = true;
    let mut terminal = None;

    loop {
        if terminal.is_some()
            || exits.is_some()
            || (!wait_pending
                && !participant_pump_state.is_pending()
                && !interactor_pump_state.is_pending())
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
            result = &mut interactor_pump, if interactor_pump_state.is_pending() => {
                match result {
                    Ok(outcome) => interactor_pump_state = AttachedPumpState::Completed(outcome),
                    Err(error) => {
                        interactor_pump_state = AttachedPumpState::Failed;
                        terminal = Some(InteractiveTerminal::Error(error));
                    }
                }
            }
            result = &mut wait_pair, if wait_pending => {
                wait_pending = false;
                match result {
                    Ok(pair) => exits = Some(pair),
                    Err(error) => terminal = Some(InteractiveTerminal::Error(error)),
                }
            }
            () = &mut session_timeout => {
                terminal = Some(InteractiveTerminal::Timeout);
            }
        }
    }

    let pump_timeout = Duration::from_secs(shutdown_grace_secs);
    match terminal {
        Some(InteractiveTerminal::Timeout) => {
            kill_interactive_pair(docker, participant_id, interactor_id).await?;
            let (participant_pump, interactor_pump) = finish_attached_pump_pair(
                participant_pump_state,
                participant_pump.as_mut(),
                interactor_pump_state,
                interactor_pump.as_mut(),
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
                interactor: ContainerOutcome {
                    exit_code: 124,
                    logs: interactor_pump.logs,
                    timed_out: true,
                    wall_time_ms,
                },
            });
        }
        Some(InteractiveTerminal::Error(error)) => {
            let original_message = error.to_string();
            let kill_result = kill_interactive_pair(docker, participant_id, interactor_id).await;
            let drain_result = drain_attached_pumps_for_cleanup(
                participant_pump_state,
                participant_pump.as_mut(),
                interactor_pump_state,
                interactor_pump.as_mut(),
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

    let (participant_exit, interactor_exit) = exits.ok_or_else(|| {
        ServiceError::Docker("interactive containers exited without status".to_string())
    })?;
    let (participant_pump, interactor_pump) = finish_attached_pump_pair(
        participant_pump_state,
        participant_pump.as_mut(),
        interactor_pump_state,
        interactor_pump.as_mut(),
        pump_timeout,
    )
    .await?;

    let wall_time_ms = duration_millis(started.elapsed());
    Ok(InteractiveSessionOutcome {
        participant: ContainerOutcome {
            exit_code: participant_exit,
            logs: participant_pump.logs,
            timed_out: false,
            wall_time_ms,
        },
        interactor: ContainerOutcome {
            exit_code: interactor_exit,
            logs: interactor_pump.logs,
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
    interactor_id: &str,
) -> Result<()> {
    let (participant, interactor) = tokio::join!(
        kill_container_if_running(docker, participant_id),
        kill_container_if_running(docker, interactor_id)
    );
    match (participant, interactor) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(participant_error), Err(interactor_error)) => Err(ServiceError::Docker(format!(
            "{participant_error}; additionally failed to stop interactor container: {interactor_error}"
        ))),
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
    interactor_state: AttachedPumpState,
    interactor_pump: Pin<&mut G>,
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
    let interactor = finish_attached_pump_future(
        "interactor",
        interactor_state,
        interactor_pump,
        pump_timeout,
    );
    let (participant, interactor) = tokio::join!(participant, interactor);
    Ok((participant?, interactor?))
}

/// Finish all still-pending pumps after a session-level error.
async fn drain_attached_pumps_for_cleanup<F, G>(
    participant_state: AttachedPumpState,
    participant_pump: Pin<&mut F>,
    interactor_state: AttachedPumpState,
    interactor_pump: Pin<&mut G>,
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
    let interactor = drain_attached_pump_for_cleanup(
        "interactor",
        interactor_state,
        interactor_pump,
        pump_timeout,
    );
    let (participant, interactor) = tokio::join!(participant, interactor);
    participant?;
    interactor?;
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
    interactor_id: String,
}

impl InteractiveKillSwitch {
    /// Best-effort kill both sides of an interactive session.
    async fn kill_both(&self) {
        drop(kill_container_if_running(&self.docker, &self.participant_id).await);
        drop(kill_container_if_running(&self.docker, &self.interactor_id).await);
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
