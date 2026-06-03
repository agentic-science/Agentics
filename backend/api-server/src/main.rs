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

use agentics_config::Config;
use agentics_persistence::pool::create_pool;
use agentics_storage::build_storage;
use anyhow::Context;
use api_server::router;
use api_server::state::AppState;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
/// Starts the API server, wires storage/database state, and handles termination signals.
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    config.validate_api_security()?;
    info!(
        "starting api server on {}:{}",
        config.api_web.api_host, config.api_web.api_port
    );

    let db = create_pool(&config, 10)
        .await
        .context("create database pool")?;
    let storage = build_storage(config.storage_factory_options()?)
        .await
        .context("build storage backend")?;

    // Seed challenges from challenges_root
    if tokio::fs::metadata(&config.storage.challenges_root)
        .await
        .is_ok()
    {
        agentics_services::maintenance::ensure_challenges_seeded_from_root(
            &db,
            &config,
            storage.as_ref(),
            &config.storage.challenges_root,
        )
        .await
        .context("seed challenges from configured root")?;
    }

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config.clone()),
        storage,
        github_sign_in_client: Arc::new(agentics_services::auth::ReqwestGithubSignInClient),
    };

    let app = router::router(&config).with_state(state);

    let listener = TcpListener::bind(format!(
        "{}:{}",
        config.api_web.api_host, config.api_web.api_port
    ))
    .await
    .with_context(|| {
        format!(
            "bind API listener on {}:{}",
            config.api_web.api_host, config.api_web.api_port
        )
    })?;
    info!("api server listening on {}", listener.local_addr()?);

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

    let shutdown = async move {
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }

        info!("received shutdown signal");
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await?;

    info!("api server exited");
    Ok(())
}
