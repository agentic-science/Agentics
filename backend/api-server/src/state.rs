use std::sync::Arc;

use sqlx::PgPool;

use shared::config::Config;
use shared::storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
}
