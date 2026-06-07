//! Shared Axum application state.

use std::{fmt, sync::Arc};

use sqlx::PgPool;

use agentics_config::{Config, DeploymentStage};
use agentics_services::auth::GithubSignInClient;
use agentics_storage::Storage;

/// Cloneable state passed to every API handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub deployment_stage: DeploymentStage,
    pub storage: Arc<dyn Storage>,
    pub github_sign_in_client: Arc<dyn GithubSignInClient>,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("db", &self.db)
            .field("config", &self.config)
            .field("deployment_stage", &self.deployment_stage)
            .field("storage", &"<dyn Storage>")
            .field("github_sign_in_client", &"<dyn GithubSignInClient>")
            .finish()
    }
}
