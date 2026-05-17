//! Worker loop and single-cycle execution for queued evaluation jobs.
//!
//! The binary uses `Worker::run` for continuous polling, while integration
//! tests call `run_worker_cycle` directly to exercise the same production path
//! without starting a long-lived process.

use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use tokio::sync::watch;
use tokio::time::interval;
use tracing::{error, info, warn};
use uuid::Uuid;

use shared::config::Config;
use shared::db::pool::create_pool;
use shared::db::{
    HeartbeatPayload, PersistedEvaluationResult, claim_next_evaluation_job,
    mark_evaluation_finished, mark_evaluation_started, reap_stuck_jobs,
    refresh_evaluation_job_claim, upsert_service_heartbeat,
};
use shared::models::evaluation::EvaluationStatus;
use shared::models::ids::{EvaluationId, EvaluationJobId};
use shared::runner::{
    EvaluationJobExecution, connect_docker, evaluation_runner_log_key, execute_evaluation_job,
    remove_stopped_runner_containers,
};
use shared::storage::LocalStorage;

/// Long-lived evaluation worker with shared database, Docker, and storage handles.
#[derive(Debug)]
pub struct Worker {
    config: Arc<Config>,
    db: sqlx::PgPool,
    docker: Docker,
    storage: Arc<dyn shared::storage::Storage>,
    worker_id: String,
}

impl Worker {
    /// Build a worker from runtime configuration.
    pub async fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        config.validate_runner_storage()?;
        let db = create_pool(&config, 2).await?;
        let docker = connect_docker(&config)?;
        let removed_containers = remove_stopped_runner_containers(&docker).await?;
        if removed_containers > 0 {
            info!(
                removed_containers,
                "removed stopped runner containers from previous attempts"
            );
        }
        let storage: Arc<dyn shared::storage::Storage> =
            Arc::new(LocalStorage::new(&config.storage_root));
        let worker_id = worker_instance_id();

        Ok(Self {
            config,
            db,
            docker,
            storage,
            worker_id,
        })
    }

    /// Poll for queued jobs until the shutdown watch channel is set to `true`.
    pub async fn run(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut ticker = interval(Duration::from_millis(self.config.worker_poll_interval_ms));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.run_cycle().await {
                        error!("worker cycle error: {e}");
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("worker shutting down gracefully");
                        break;
                    }
                }
            }
        }
    }

    /// Handles run cycle for this module.
    async fn run_cycle(&self) -> anyhow::Result<()> {
        run_worker_cycle(
            &self.db,
            &self.docker,
            self.config.as_ref(),
            self.storage.as_ref(),
            &self.worker_id,
        )
        .await
    }
}

/// Handles worker instance id for this module.
fn worker_instance_id() -> String {
    match host_label() {
        Some(host) => format!("agentics-worker-{host}-{}", Uuid::new_v4()),
        None => format!("agentics-worker-{}", Uuid::new_v4()),
    }
}

/// Handles host label for this module.
fn host_label() -> Option<String> {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|value| sanitize_worker_label(&value))
        .filter(|value| !value.is_empty())
}

/// Handles sanitize worker label for this module.
fn sanitize_worker_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .take(64)
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Run one worker poll iteration.
///
/// A cycle reaps stale claims, claims at most one queued job, records heartbeat
/// state, executes the runner, and persists either a completed evaluation or a
/// failed evaluation with the runner error.
pub async fn run_worker_cycle(
    db: &sqlx::PgPool,
    docker: &Docker,
    config: &Config,
    storage: &dyn shared::storage::Storage,
    worker_id: &str,
) -> anyhow::Result<()> {
    let reaped = reap_stuck_jobs(db, config.worker_stale_job_minutes.max(1)).await?;
    if reaped.requeued > 0 || reaped.failed > 0 {
        info!(
            requeued = reaped.requeued,
            failed = reaped.failed,
            "reaped stale jobs"
        );
    }

    let job = claim_next_evaluation_job(db, worker_id).await?;

    let Some(job) = job else {
        // Heartbeats are the admin-facing signal that an otherwise idle worker
        // is still alive and able to claim future jobs.
        upsert_service_heartbeat(
            db,
            worker_id,
            &HeartbeatPayload {
                status: "idle".to_string(),
                job_id: None,
                solution_submission_id: None,
                last_completed_job_id: None,
                last_failed_job_id: None,
            },
        )
        .await?;
        return Ok(());
    };

    upsert_service_heartbeat(
        db,
        worker_id,
        &HeartbeatPayload {
            status: "running".to_string(),
            job_id: Some(job.id.clone()),
            solution_submission_id: Some(job.solution_submission_id.clone()),
            last_completed_job_id: None,
            last_failed_job_id: None,
        },
    )
    .await?;

    let evaluation_id = EvaluationId::generate();
    let evaluation_inserted = mark_evaluation_started(
        db,
        &shared::db::MarkEvaluationStartedInput {
            evaluation_id,
            solution_submission_id: job.solution_submission_id.clone(),
            job_id: job.id.clone(),
            worker_id: worker_id.to_string(),
            claim_attempt_count: job.attempt_count,
            target: job.target.clone(),
            eval_type: job.eval_type,
        },
    )
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
        db.clone(),
        job.id.clone(),
        worker_id.to_string(),
        lease_refresh_interval(config),
        lease_stop_rx,
    ));

    let exec_result = execute_evaluation_job(EvaluationJobExecution {
        docker,
        config,
        job_id: job.id.as_str(),
        worker_id,
        attempt_count: job.attempt_count,
        eval_type: job.eval_type,
        payload: &job.payload,
        storage,
    })
    .await;
    let _ = lease_stop_tx.send(true);
    if let Err(join_err) = lease_task.await {
        error!(error = %join_err, "job lease refresh task failed");
    }

    match exec_result {
        Ok(result) => {
            let job_id = job.id.clone();
            let solution_submission_id = job.solution_submission_id.clone();
            let primary_score = result.result.primary_score;
            let rank_score = result.result.rank_score;

            let persisted = mark_evaluation_finished(
                db,
                &PersistedEvaluationResult {
                    solution_submission_id: solution_submission_id.clone(),
                    job_id: job_id.clone(),
                    worker_id: worker_id.to_string(),
                    claim_attempt_count: job.attempt_count,
                    target: job.target.clone(),
                    eval_type: job.eval_type,
                    status: EvaluationStatus::Completed,
                    primary_score: Some(primary_score),
                    rank_score,
                    aggregate_metrics: result.result.aggregate_metrics,
                    run_metrics: result.result.run_metrics,
                    public_results: result.result.public_results,
                    validation_summary: result.result.validation_summary,
                    official_summary: result.result.official_summary,
                    log_key: Some(result.log_key),
                    last_error: None,
                },
            )
            .await?;
            if !persisted {
                warn!(
                    job_id = %job_id,
                    worker_id,
                    attempt_count = job.attempt_count,
                    "ignored evaluation completion from stale worker claim"
                );
                upsert_service_heartbeat(
                    db,
                    worker_id,
                    &HeartbeatPayload {
                        status: "idle".to_string(),
                        job_id: None,
                        solution_submission_id: None,
                        last_completed_job_id: None,
                        last_failed_job_id: None,
                    },
                )
                .await?;
                return Ok(());
            }

            upsert_service_heartbeat(
                db,
                worker_id,
                &HeartbeatPayload {
                    status: "idle".to_string(),
                    job_id: None,
                    solution_submission_id: None,
                    last_completed_job_id: Some(job_id.clone()),
                    last_failed_job_id: None,
                },
            )
            .await?;

            info!(
                job_id = %job_id,
                solution_submission_id = %solution_submission_id,
                primary_score = %primary_score,
                "evaluation completed"
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            let log_key = evaluation_runner_log_key(job.id.as_str(), job.attempt_count).ok();
            let persisted = mark_evaluation_finished(
                db,
                &PersistedEvaluationResult {
                    solution_submission_id: job.solution_submission_id.clone(),
                    job_id: job.id.clone(),
                    worker_id: worker_id.to_string(),
                    claim_attempt_count: job.attempt_count,
                    target: job.target.clone(),
                    eval_type: job.eval_type,
                    status: EvaluationStatus::Failed,
                    primary_score: None,
                    rank_score: None,
                    aggregate_metrics: vec![],
                    run_metrics: vec![],
                    public_results: vec![],
                    validation_summary: None,
                    official_summary: None,
                    log_key,
                    last_error: Some(error_msg.clone()),
                },
            )
            .await?;
            if !persisted {
                warn!(
                    job_id = %job.id,
                    worker_id,
                    attempt_count = job.attempt_count,
                    error = %error_msg,
                    "ignored evaluation failure from stale worker claim"
                );
                upsert_service_heartbeat(
                    db,
                    worker_id,
                    &HeartbeatPayload {
                        status: "idle".to_string(),
                        job_id: None,
                        solution_submission_id: None,
                        last_completed_job_id: None,
                        last_failed_job_id: None,
                    },
                )
                .await?;
                return Ok(());
            }

            upsert_service_heartbeat(
                db,
                worker_id,
                &HeartbeatPayload {
                    status: "idle".to_string(),
                    job_id: None,
                    solution_submission_id: None,
                    last_completed_job_id: None,
                    last_failed_job_id: Some(job.id.clone()),
                },
            )
            .await?;

            error!(
                job_id = %job.id,
                solution_submission_id = %job.solution_submission_id,
                error = %error_msg,
                "evaluation failed"
            );
        }
    }

    Ok(())
}

/// Handles lease refresh interval for this module.
fn lease_refresh_interval(config: &Config) -> Duration {
    let stale_minutes = u64::from(config.worker_stale_job_minutes.max(1).unsigned_abs());
    let stale_window = Duration::from_mins(stale_minutes);
    stale_window
        .checked_div(3)
        .unwrap_or(stale_window)
        .clamp(Duration::from_secs(5), Duration::from_mins(1))
}

/// Handles refresh claim until stopped for this module.
async fn refresh_claim_until_stopped(
    db: sqlx::PgPool,
    job_id: EvaluationJobId,
    worker_id: String,
    refresh_every: Duration,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = interval(refresh_every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match refresh_evaluation_job_claim(&db, &job_id, &worker_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        error!(job_id = %job_id, worker_id = %worker_id, "job lease no longer belongs to worker");
                        break;
                    }
                    Err(e) => {
                        error!(job_id = %job_id, worker_id = %worker_id, error = %e, "failed to refresh job lease");
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
    use super::{sanitize_worker_label, worker_instance_id};

    /// Verifies that worker instance ids use log safe uuid suffix.
    #[test]
    fn worker_instance_ids_use_log_safe_uuid_suffix() {
        let instance_id = worker_instance_id();
        let uuid_start = instance_id
            .len()
            .checked_sub(36)
            .expect("worker id should include a UUID suffix");
        let (prefix, uuid_suffix) = instance_id.split_at(uuid_start);

        assert!(prefix.starts_with("agentics-worker-"));
        assert!(uuid::Uuid::parse_str(uuid_suffix).is_ok());
    }

    /// Verifies that worker host label is log safe.
    #[test]
    fn worker_host_label_is_log_safe() {
        assert_eq!(
            sanitize_worker_label("dgx spark/slot 1"),
            "dgx-spark-slot-1"
        );
    }
}
