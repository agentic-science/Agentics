#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::sync::Arc;

use tracing::info;

use shared::config::Config;
use worker::cycle::Worker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = Arc::new(Config::from_env()?);
    info!("starting worker");

    let worker = Worker::new(config.clone()).await?;

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
