#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::enum_glob_use,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::wildcard_imports,
        reason = "unit tests use direct assertions and fixture indexing for concise failure diagnostics"
    )
)]

use std::sync::Arc;

use tracing::info;

use agentics_config::{Config, EnvPolicyReport, EnvServiceRole};
use worker::cycle::Worker;

#[tokio::main]
/// Handles main for this module.
async fn main() -> anyhow::Result<()> {
    let env_report = agentics_config::validate_current_env_policy(EnvServiceRole::Worker)?;
    print_env_policy_warnings(&env_report);

    let config = Arc::new(Config::from_env()?);
    init_logging(&config.logging.log_level)?;
    info!("starting worker");

    let worker = Worker::new(Arc::clone(&config)).await?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }

        info!("received shutdown signal");
        let _ = shutdown_tx.send(true);
        Ok::<(), anyhow::Error>(())
    });

    worker.run(shutdown_rx).await;

    info!("worker exited");
    Ok(())
}

fn init_logging(log_level: &str) -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_new(log_level)
        .map_err(|error| anyhow::anyhow!("invalid AGENTICS_LOG_LEVEL: {error}"))?;
    tracing_subscriber::fmt().with_env_filter(filter).init();
    Ok(())
}

fn print_env_policy_warnings(report: &EnvPolicyReport) {
    for warning in &report.warnings {
        eprintln!("[worker] WARN env {}: {}", warning.name, warning.message);
    }
}
