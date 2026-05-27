//! Evaluation job lifecycle orchestration.
//!
//! SQL state transitions still live in persistence, and container execution
//! still lives in the runner crate. This module owns the application workflow
//! around one worker polling cycle.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;
use tokio::time::interval;
use tracing::{error, info, warn};

use agentics_config::Config;
use agentics_domain::models::evaluation::EvaluationStatus;
use agentics_domain::models::ids::{EvaluationId, EvaluationJobId, SolutionSubmissionId};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    EvaluationJobRecord, HeartbeatPayload, MarkEvaluationStartedInput, PersistedEvaluationResult,
    QueueEvaluationJobInput, Repositories,
};
use agentics_runner::{
    DockerRunner, EvaluationJobExecution, ExecutionResult, RunnerContainerCleanupSummary,
    RunnerContainerIdentity, RunnerContainerRuntimeState, RunnerContainerScope,
    evaluation_runner_log_key,
};
use agentics_storage::Storage;

/// Request for queueing a new evaluation job for an existing solution submission.
#[derive(Debug, Clone)]
pub struct QueueEvaluationJobRequest {
    pub solution_submission_id: SolutionSubmissionId,
    pub eval_type: agentics_domain::models::evaluation::ScoringMode,
    pub max_active_official_jobs: Option<i64>,
}

/// Outcome of one worker polling cycle after lifecycle orchestration completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationWorkerCycleOutcome {
    /// No job was available after maintenance and the worker is idle.
    Idle,
    /// Maintenance ran and removed or reaped stale runtime state, but no job was claimed.
    MaintenanceOnly {
        reaped_requeued: u64,
        reaped_failed: u64,
        removed_containers: u64,
    },
    /// A job completed and persisted its evaluator result.
    Completed {
        job_id: EvaluationJobId,
        solution_submission_id: SolutionSubmissionId,
    },
    /// A job was requeued because runner capacity was temporarily unavailable.
    RequeuedForCapacity { job_id: EvaluationJobId },
    /// A job finished as failed and persisted the failure.
    Failed {
        job_id: EvaluationJobId,
        solution_submission_id: SolutionSubmissionId,
    },
}

/// Service object for running worker evaluation lifecycle transitions.
pub struct EvaluationWorkerService<'a> {
    db: &'a sqlx::PgPool,
    runner: &'a DockerRunner<'a>,
    config: &'a Config,
    storage: &'a dyn Storage,
}

impl std::fmt::Debug for EvaluationWorkerService<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvaluationWorkerService")
            .field("db", &"<PgPool>")
            .field("runner", &"<DockerRunner>")
            .field("config", self.config)
            .field("storage", &"<Storage>")
            .finish()
    }
}

impl<'a> EvaluationWorkerService<'a> {
    /// Build a service over worker runtime handles.
    pub const fn new(
        db: &'a sqlx::PgPool,
        runner: &'a DockerRunner<'a>,
        config: &'a Config,
        storage: &'a dyn Storage,
    ) -> Self {
        Self {
            db,
            runner,
            config,
            storage,
        }
    }

    /// Run one worker polling cycle.
    pub async fn run_one_cycle(&self, worker_id: &str) -> Result<EvaluationWorkerCycleOutcome> {
        let maintenance = self.reap_and_reconcile().await?;
        let repos = Repositories::new(self.db);
        let job = repos
            .evaluation_jobs()
            .claim_next(worker_id, self.config.worker.accelerators)
            .await?;

        let Some(job) = job else {
            self.write_idle_heartbeat(worker_id, None, None).await?;
            if maintenance.has_work() {
                return Ok(EvaluationWorkerCycleOutcome::MaintenanceOnly {
                    reaped_requeued: maintenance.reaped_requeued,
                    reaped_failed: maintenance.reaped_failed,
                    removed_containers: maintenance.removed_containers,
                });
            }
            return Ok(EvaluationWorkerCycleOutcome::Idle);
        };

        repos
            .maintenance()
            .upsert_service_heartbeat(
                worker_id,
                &HeartbeatPayload {
                    status: "running".to_string(),
                    accelerators: self.config.worker.accelerators.heartbeat_values(),
                    job_id: Some(job.id.clone()),
                    solution_submission_id: Some(job.solution_submission_id.clone()),
                    last_completed_job_id: None,
                    last_failed_job_id: None,
                },
            )
            .await?;

        let evaluation_inserted = repos
            .evaluation_jobs()
            .mark_started(&MarkEvaluationStartedInput {
                evaluation_id: EvaluationId::generate(),
                solution_submission_id: job.solution_submission_id.clone(),
                job_id: job.id.clone(),
                worker_id: worker_id.to_string(),
                claim_attempt_count: job.attempt_count,
                target: job.target.clone(),
                eval_type: job.eval_type,
            })
            .await?;
        if !evaluation_inserted {
            warn!(
                job_id = %job.id,
                worker_id,
                attempt_count = job.attempt_count,
                "evaluation row already exists for job; preserving original start record"
            );
        }

        let (lease_stop_tx, lease_stop_rx) = watch::channel(false);
        let lease_task = tokio::spawn(refresh_claim_until_stopped(
            self.db.clone(),
            job.id.clone(),
            worker_id.to_string(),
            job.attempt_count,
            lease_refresh_interval(self.config),
            lease_stop_rx,
        ));

        let exec_result = self
            .runner
            .execute_evaluation_job(EvaluationJobExecution {
                config: self.config,
                job_id: job.id.as_str(),
                worker_id,
                attempt_count: job.attempt_count,
                container_scope: RunnerContainerScope::HostedWorker,
                eval_type: job.eval_type,
                payload: &job.payload,
                storage: self.storage,
            })
            .await;
        let _ = lease_stop_tx.send(true);
        if let Err(join_err) = lease_task.await {
            error!(error = %join_err, "job lease refresh task failed");
        }

        match exec_result {
            Ok(result) => self.finish_completed_job(worker_id, &job, result).await,
            Err(ServiceError::RunnerCapacity(error_msg)) => {
                self.requeue_capacity_limited_job(worker_id, &job, &error_msg)
                    .await
            }
            Err(error) => {
                self.finish_failed_job(worker_id, &job, &error.to_string())
                    .await
            }
        }
    }

    async fn reap_and_reconcile(&self) -> Result<EvaluationWorkerMaintenanceSummary> {
        let repos = Repositories::new(self.db);
        let reaped = repos
            .maintenance()
            .reap_stuck_jobs(self.config.worker.stale_job_minutes.max(1))
            .await?;
        if reaped.requeued > 0 || reaped.failed > 0 {
            info!(
                requeued = reaped.requeued,
                failed = reaped.failed,
                "reaped stale jobs"
            );
        }
        let cleanup = reconcile_worker_containers(
            self.runner,
            self.db,
            self.config.worker.stale_job_minutes.max(1),
            self.config,
        )
        .await?;
        if cleanup.total_removed() > 0 {
            info!(
                removed_stopped = cleanup.removed_stopped,
                removed_running = cleanup.removed_running,
                "reconciled runner containers"
            );
        }
        Ok(EvaluationWorkerMaintenanceSummary {
            reaped_requeued: reaped.requeued,
            reaped_failed: reaped.failed,
            removed_containers: cleanup.total_removed(),
        })
    }

    async fn finish_completed_job(
        &self,
        worker_id: &str,
        job: &EvaluationJobRecord,
        result: ExecutionResult,
    ) -> Result<EvaluationWorkerCycleOutcome> {
        let job_id = job.id.clone();
        let solution_submission_id = job.solution_submission_id.clone();
        let rank_score = result.result.rank_score;

        let persisted = Repositories::new(self.db)
            .evaluation_jobs()
            .mark_finished(&PersistedEvaluationResult {
                solution_submission_id: solution_submission_id.clone(),
                job_id: job_id.clone(),
                worker_id: worker_id.to_string(),
                claim_attempt_count: job.attempt_count,
                target: job.target.clone(),
                eval_type: job.eval_type,
                status: EvaluationStatus::Completed,
                rank_score,
                aggregate_metrics: result.result.aggregate_metrics,
                run_metrics: result.result.run_metrics,
                public_results: result.result.public_results,
                validation_summary: result.result.validation_summary,
                official_summary: result.result.official_summary,
                log_key: Some(result.log_key),
                last_error: None,
            })
            .await?;
        if !persisted {
            warn!(
                job_id = %job_id,
                worker_id,
                attempt_count = job.attempt_count,
                "ignored evaluation completion from stale worker claim"
            );
            self.write_idle_heartbeat(worker_id, None, None).await?;
            return Ok(EvaluationWorkerCycleOutcome::Idle);
        }

        self.write_idle_heartbeat(worker_id, Some(job_id.clone()), None)
            .await?;

        info!(
            job_id = %job_id,
            solution_submission_id = %solution_submission_id,
            rank_score = ?rank_score,
            "evaluation completed"
        );

        Ok(EvaluationWorkerCycleOutcome::Completed {
            job_id,
            solution_submission_id,
        })
    }

    async fn requeue_capacity_limited_job(
        &self,
        worker_id: &str,
        job: &EvaluationJobRecord,
        error_msg: &str,
    ) -> Result<EvaluationWorkerCycleOutcome> {
        let requeued = Repositories::new(self.db)
            .evaluation_jobs()
            .requeue_for_capacity(&job.id, worker_id, job.attempt_count, error_msg)
            .await?;
        if !requeued {
            warn!(
                job_id = %job.id,
                worker_id,
                attempt_count = job.attempt_count,
                error = %error_msg,
                "ignored capacity requeue from stale worker claim"
            );
        } else {
            warn!(
                job_id = %job.id,
                solution_submission_id = %job.solution_submission_id,
                error = %error_msg,
                "evaluation requeued because runner capacity is temporarily unavailable"
            );
        }
        self.write_idle_heartbeat(worker_id, None, None).await?;
        if !requeued {
            return Ok(EvaluationWorkerCycleOutcome::Idle);
        }
        Ok(EvaluationWorkerCycleOutcome::RequeuedForCapacity {
            job_id: job.id.clone(),
        })
    }

    async fn finish_failed_job(
        &self,
        worker_id: &str,
        job: &EvaluationJobRecord,
        error_msg: &str,
    ) -> Result<EvaluationWorkerCycleOutcome> {
        let log_key = evaluation_runner_log_key(job.id.as_str(), job.attempt_count).ok();
        let persisted = Repositories::new(self.db)
            .evaluation_jobs()
            .mark_finished(&PersistedEvaluationResult {
                solution_submission_id: job.solution_submission_id.clone(),
                job_id: job.id.clone(),
                worker_id: worker_id.to_string(),
                claim_attempt_count: job.attempt_count,
                target: job.target.clone(),
                eval_type: job.eval_type,
                status: EvaluationStatus::Failed,
                rank_score: None,
                aggregate_metrics: vec![],
                run_metrics: vec![],
                public_results: vec![],
                validation_summary: None,
                official_summary: None,
                log_key,
                last_error: Some(error_msg.to_string()),
            })
            .await?;
        if !persisted {
            warn!(
                job_id = %job.id,
                worker_id,
                attempt_count = job.attempt_count,
                error = %error_msg,
                "ignored evaluation failure from stale worker claim"
            );
            self.write_idle_heartbeat(worker_id, None, None).await?;
            return Ok(EvaluationWorkerCycleOutcome::Idle);
        }

        self.write_idle_heartbeat(worker_id, None, Some(job.id.clone()))
            .await?;

        error!(
            job_id = %job.id,
            solution_submission_id = %job.solution_submission_id,
            error = %error_msg,
            "evaluation failed"
        );

        Ok(EvaluationWorkerCycleOutcome::Failed {
            job_id: job.id.clone(),
            solution_submission_id: job.solution_submission_id.clone(),
        })
    }

    async fn write_idle_heartbeat(
        &self,
        worker_id: &str,
        last_completed_job_id: Option<EvaluationJobId>,
        last_failed_job_id: Option<EvaluationJobId>,
    ) -> Result<()> {
        Repositories::new(self.db)
            .maintenance()
            .upsert_service_heartbeat(
                worker_id,
                &HeartbeatPayload {
                    status: "idle".to_string(),
                    accelerators: self.config.worker.accelerators.heartbeat_values(),
                    job_id: None,
                    solution_submission_id: None,
                    last_completed_job_id,
                    last_failed_job_id,
                },
            )
            .await
    }
}

/// Reconcile runner containers for one worker runtime.
pub async fn reconcile_worker_containers(
    runner: &DockerRunner<'_>,
    db: &sqlx::PgPool,
    stale_minutes: i32,
    config: &Config,
) -> Result<RunnerContainerCleanupSummary> {
    let repos = Repositories::new(db);
    let containers = runner.list_hosted_worker_containers(config).await?;
    let now_secs = current_unix_timestamp_secs();
    let mut summary = RunnerContainerCleanupSummary::default();

    for container in containers {
        match container.state {
            RunnerContainerRuntimeState::Stopped => {
                if container.is_stale(now_secs) {
                    runner
                        .remove_runner_container(config, &container.container_id)
                        .await?;
                    summary.removed_stopped =
                        summary.removed_stopped.checked_add(1).ok_or_else(|| {
                            ServiceError::Internal(
                                "removed stopped container count overflow".to_string(),
                            )
                        })?;
                }
            }
            RunnerContainerRuntimeState::Running => {
                let should_remove = match &container.identity {
                    Some(identity) => {
                        let claim = repos
                            .evaluation_jobs()
                            .runner_claim(&identity.job_id, stale_minutes)
                            .await?;
                        runner_container_claim_is_stale(identity, claim.as_ref())
                    }
                    None => true,
                };
                if should_remove {
                    runner
                        .kill_runner_container(config, &container.container_id)
                        .await?;
                    summary.removed_running =
                        summary.removed_running.checked_add(1).ok_or_else(|| {
                            ServiceError::Internal(
                                "removed running container count overflow".to_string(),
                            )
                        })?;
                }
            }
        }
    }

    Ok(summary)
}

fn runner_container_claim_is_stale(
    identity: &RunnerContainerIdentity,
    claim: Option<&agentics_persistence::RunnerJobClaimRecord>,
) -> bool {
    let Some(claim) = claim else {
        return true;
    };
    !(claim.status == "running"
        && claim.worker_id.as_deref() == Some(identity.worker_id.as_str())
        && claim.attempt_count == identity.attempt_count
        && claim.claim_is_fresh)
}

fn current_unix_timestamp_secs() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

/// Promote a staged evaluation job into the queued worker lifecycle.
pub async fn mark_staged_evaluation_job_ready(
    db: &sqlx::PgPool,
    job_id: &EvaluationJobId,
) -> Result<()> {
    Repositories::new(db)
        .evaluation_jobs()
        .mark_ready(job_id)
        .await
}

/// Queue an evaluation job for an existing solution submission.
pub async fn queue_solution_evaluation_job(
    db: &sqlx::PgPool,
    request: QueueEvaluationJobRequest,
) -> Result<agentics_persistence::EvaluationJobRecord> {
    Repositories::new(db)
        .evaluation_jobs()
        .queue(&QueueEvaluationJobInput {
            job_id: EvaluationJobId::generate(),
            solution_submission_id: request.solution_submission_id,
            eval_type: request.eval_type,
            max_active_official_jobs: request.max_active_official_jobs,
        })
        .await
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EvaluationWorkerMaintenanceSummary {
    reaped_requeued: u64,
    reaped_failed: u64,
    removed_containers: u64,
}

impl EvaluationWorkerMaintenanceSummary {
    const fn has_work(self) -> bool {
        self.reaped_requeued > 0 || self.reaped_failed > 0 || self.removed_containers > 0
    }
}

fn lease_refresh_interval(config: &Config) -> Duration {
    let stale_minutes = u64::from(config.worker.stale_job_minutes.max(1).unsigned_abs());
    let stale_window = Duration::from_secs(stale_minutes.saturating_mul(60));
    stale_window
        .checked_div(3)
        .unwrap_or(stale_window)
        .clamp(Duration::from_secs(5), Duration::from_secs(60))
}

async fn refresh_claim_until_stopped(
    db: sqlx::PgPool,
    job_id: EvaluationJobId,
    worker_id: String,
    attempt_count: i32,
    refresh_every: Duration,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = interval(refresh_every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match Repositories::new(&db)
                    .evaluation_jobs()
                    .refresh_claim(&job_id, &worker_id, attempt_count)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        error!(job_id = %job_id, worker_id = %worker_id, attempt_count, "job lease no longer belongs to worker attempt");
                        break;
                    }
                    Err(e) => {
                        error!(job_id = %job_id, worker_id = %worker_id, attempt_count, error = %e, "failed to refresh job lease");
                    }
                }
            }
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::runner_container_claim_is_stale;
    use agentics_domain::models::ids::EvaluationJobId;
    use agentics_persistence::RunnerJobClaimRecord;
    use agentics_runner::RunnerContainerIdentity;

    fn identity(worker_id: &str, attempt_count: i32) -> RunnerContainerIdentity {
        RunnerContainerIdentity {
            job_id: EvaluationJobId::generate(),
            worker_id: worker_id.to_string(),
            attempt_count,
        }
    }

    /// Verifies fresh matching claims keep their runner containers.
    #[test]
    fn runner_container_claim_keeps_fresh_matching_claim() {
        let identity = identity("worker-a", 2);
        let claim = RunnerJobClaimRecord {
            status: "running".to_string(),
            worker_id: Some("worker-a".to_string()),
            attempt_count: 2,
            claim_is_fresh: true,
        };

        assert!(!runner_container_claim_is_stale(&identity, Some(&claim)));
    }

    /// Verifies stale or superseded claims remove running runner containers.
    #[test]
    fn runner_container_claim_removes_stale_or_superseded_claims() {
        let identity = identity("worker-a", 2);

        for claim in [
            RunnerJobClaimRecord {
                status: "queued".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 2,
                claim_is_fresh: true,
            },
            RunnerJobClaimRecord {
                status: "running".to_string(),
                worker_id: Some("worker-b".to_string()),
                attempt_count: 2,
                claim_is_fresh: true,
            },
            RunnerJobClaimRecord {
                status: "running".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 3,
                claim_is_fresh: true,
            },
            RunnerJobClaimRecord {
                status: "running".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 2,
                claim_is_fresh: false,
            },
        ] {
            assert!(runner_container_claim_is_stale(&identity, Some(&claim)));
        }
        assert!(runner_container_claim_is_stale(&identity, None));
    }
}
