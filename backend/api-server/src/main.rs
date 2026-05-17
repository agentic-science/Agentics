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

use api_server::admin_auth_throttle::AdminAuthThrottle;
use api_server::router;
use api_server::state::AppState;
use shared::config::Config;
use shared::db::pool::create_pool;
use shared::storage::LocalStorage;
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
        config.api_host, config.api_port
    );

    let db = create_pool(&config, 10).await?;
    let storage = Arc::new(LocalStorage::new(&config.storage_root));

    // Seed challenges from challenges_root
    if tokio::fs::metadata(&config.challenges_root).await.is_ok() {
        shared::db::ensure_challenges_seeded_from_root(&db, &config.challenges_root).await?;
    }

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config.clone()),
        storage,
        admin_auth_throttle: Arc::new(AdminAuthThrottle::new()?),
    };

    let app = router::router(&config).with_state(state);

    let listener = TcpListener::bind(format!("{}:{}", config.api_host, config.api_port)).await?;
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
