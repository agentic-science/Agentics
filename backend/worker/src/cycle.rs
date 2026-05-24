//! Worker loop and single-cycle execution for queued evaluation jobs.
//!
//! The binary uses `Worker::run` for continuous polling, while integration
//! tests call `run_worker_cycle` directly to exercise the same production path
//! without starting a long-lived process.

use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use tokio::time::interval;
use tracing::{error, info};
use uuid::Uuid;

use agentics_config::Config;
use agentics_persistence::pool::create_pool;
use agentics_runner::connect_docker;
use agentics_services::evaluation_lifecycle::{
    EvaluationWorkerService, reconcile_worker_containers,
};
use agentics_storage::LocalStorage;

use crate::host_probe::{enforce_host_probe, enforce_worker_gpu_probe};

/// Long-lived evaluation worker with shared database, Docker, and storage handles.
#[derive(Debug)]
pub struct Worker {
    config: Arc<Config>,
    db: sqlx::PgPool,
    docker: Docker,
    storage: Arc<dyn agentics_storage::Storage>,
    worker_id: String,
}

impl Worker {
    /// Build a worker from runtime configuration.
    pub async fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        config.validate_runner_storage()?;
        enforce_host_probe(&config).await?;
        let db = create_pool(&config, 2).await?;
        let docker = connect_docker(&config)?;
        enforce_worker_gpu_probe(&config, &docker).await?;
        let cleanup =
            reconcile_worker_containers(&docker, &db, config.worker_stale_job_minutes.max(1))
                .await?;
        if cleanup.total_removed() > 0 {
            info!(
                removed_stopped = cleanup.removed_stopped,
                removed_running = cleanup.removed_running,
                "reconciled runner containers from previous attempts"
            );
        }
        let storage: Arc<dyn agentics_storage::Storage> =
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
/// The worker crate keeps the public test hook and delegates lifecycle decisions
/// to `agentics-services`.
pub async fn run_worker_cycle(
    db: &sqlx::PgPool,
    docker: &Docker,
    config: &Config,
    storage: &dyn agentics_storage::Storage,
    worker_id: &str,
) -> anyhow::Result<()> {
    EvaluationWorkerService::new(db, docker, config, storage)
        .run_one_cycle(worker_id)
        .await?;
    Ok(())
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
