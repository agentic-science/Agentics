//! Worker loop and single-cycle execution for queued evaluation jobs.
//!
//! The binary uses `Worker::run` for continuous polling, while integration
//! tests call `run_worker_cycle` directly to exercise the same production path
//! without starting a long-lived process.

use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use tokio::time::interval;
use tracing::{error, info, warn};

use shared::config::Config;
use shared::db::pool::create_pool;
use shared::db::queries::{
    HeartbeatPayload, PersistedEvaluationResult, claim_next_evaluation_job,
    mark_evaluation_finished, mark_evaluation_started, reap_stuck_jobs, upsert_service_heartbeat,
};
use shared::models::evaluation::EvaluationStatus;
use shared::runner::{connect_docker, execute_evaluation_job, pre_pull_image};
use shared::storage::LocalStorage;

/// Long-lived evaluation worker with shared database, Docker, and storage handles.
pub struct Worker {
    config: Arc<Config>,
    db: sqlx::PgPool,
    docker: Docker,
    storage: Arc<dyn shared::storage::Storage>,
    worker_id: String,
}

impl Worker {
    /// Build a worker from runtime configuration and pre-pull the runner image.
    pub async fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        let db = create_pool(&config, 2).await?;
        let docker = connect_docker(&config)?;
        let storage: Arc<dyn shared::storage::Storage> =
            Arc::new(LocalStorage::new(&config.storage_root));
        let worker_id = format!("agentics-worker-{}", std::process::id());

        info!("pre-pulling runner image: {}", config.runner_python_image);
        if let Err(e) = pre_pull_image(&docker, &config.runner_python_image).await {
            warn!("failed to pre-pull image: {e}")
        }

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
    let reaped = reap_stuck_jobs(db, (config.runner_timeout_sec * 2 / 60).max(1) as i32).await?;
    if reaped > 0 {
        info!("reaped {reaped} stuck jobs");
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
                submission_id: None,
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
            submission_id: Some(job.submission_id.clone()),
            last_completed_job_id: None,
            last_failed_job_id: None,
        },
    )
    .await?;

    let evaluation_id = uuid::Uuid::new_v4().to_string();
    mark_evaluation_started(
        db,
        &shared::db::queries::MarkEvaluationStartedInput {
            evaluation_id: evaluation_id.clone(),
            submission_id: job.submission_id.clone(),
            job_id: job.id.clone(),
            eval_type: job.eval_type,
        },
    )
    .await?;

    let exec_result = execute_evaluation_job(
        docker,
        config,
        &job.id,
        job.eval_type,
        &job.payload,
        storage,
    )
    .await;

    match exec_result {
        Ok(result) => {
            let job_id = job.id.clone();
            let submission_id = job.submission_id.clone();
            let primary_score = result.result.primary_score;

            mark_evaluation_finished(
                db,
                &PersistedEvaluationResult {
                    evaluation_id,
                    submission_id: submission_id.clone(),
                    job_id: job_id.clone(),
                    eval_type: job.eval_type,
                    status: EvaluationStatus::Completed,
                    primary_score: Some(primary_score),
                    shown_results: result.result.shown_results,
                    hidden_summary: result.result.hidden_summary,
                    official_summary: result.result.official_summary,
                    log_path: Some(result.log_path),
                    last_error: None,
                },
            )
            .await?;

            upsert_service_heartbeat(
                db,
                worker_id,
                &HeartbeatPayload {
                    status: "idle".to_string(),
                    job_id: None,
                    submission_id: None,
                    last_completed_job_id: Some(job_id.clone()),
                    last_failed_job_id: None,
                },
            )
            .await?;

            info!(
                job_id = %job_id,
                submission_id = %submission_id,
                primary_score = %primary_score,
                "evaluation completed"
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            mark_evaluation_finished(
                db,
                &PersistedEvaluationResult {
                    evaluation_id,
                    submission_id: job.submission_id.clone(),
                    job_id: job.id.clone(),
                    eval_type: job.eval_type,
                    status: EvaluationStatus::Failed,
                    primary_score: None,
                    shown_results: vec![],
                    hidden_summary: None,
                    official_summary: None,
                    log_path: None,
                    last_error: Some(error_msg.clone()),
                },
            )
            .await?;

            upsert_service_heartbeat(
                db,
                worker_id,
                &HeartbeatPayload {
                    status: "idle".to_string(),
                    job_id: None,
                    submission_id: None,
                    last_completed_job_id: None,
                    last_failed_job_id: Some(job.id.clone()),
                },
            )
            .await?;

            error!(
                job_id = %job.id,
                submission_id = %job.submission_id,
                error = %error_msg,
                "evaluation failed"
            );
        }
    }

    Ok(())
}
