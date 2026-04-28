//! Shared Axum application state.

use std::sync::Arc;

use sqlx::PgPool;

use shared::config::Config;
use shared::storage::Storage;

/// Cloneable state passed to every API handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
}
