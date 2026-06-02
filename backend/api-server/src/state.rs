//! Shared Axum application state.

use std::sync::Arc;

use sqlx::PgPool;

use agentics_config::Config;
use agentics_storage::Storage;

/// Cloneable state passed to every API handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
}
