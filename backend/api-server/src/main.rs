mod extractors;
mod handlers;
mod presenters;
mod router;
mod state;

use std::sync::Arc;

use axum::Router;
use shared::config::Config;
use shared::db::pool::create_pool;
use shared::storage::LocalStorage;
use tokio::net::TcpListener;
use tracing::info;

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    info!("starting api server on {}:{}", config.api_host, config.api_port);

    let db = create_pool(&config, 10).await?;
    let storage = Arc::new(LocalStorage::new(&config.storage_root));

    // Seed problems from problems_root
    if tokio::fs::metadata(&config.problems_root).await.is_ok() {
        shared::db::queries::ensure_problems_seeded_from_root(&db, &config.problems_root).await?;
    }

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config.clone()),
        storage,
    };

    let app = router::router().with_state(state);

    let listener = TcpListener::bind(format!("{}:{}", config.api_host, config.api_port)).await?;
    info!("api server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
