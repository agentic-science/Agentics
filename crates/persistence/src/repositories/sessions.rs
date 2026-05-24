use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    AuthenticatedAdminSession, AuthenticatedCreatorSession, ConsumedGithubOauthState,
    CreateAdminSessionInput, CreateCreatorSessionInput, CreateGithubOauthStateInput,
};
use agentics_domain::error::Result;
use agentics_domain::models::ids::AgentId;

#[derive(Debug, Clone, Copy)]
pub struct SessionsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl SessionsRepository<'_> {
    pub async fn upsert_github_creator_agent(
        &self,
        agent_id: &AgentId,
        github_user_id: i64,
        github_login: &str,
        max_active_agents: i64,
    ) -> Result<AgentId> {
        db::sessions::upsert_github_creator_agent(
            self.pool,
            agent_id,
            github_user_id,
            github_login,
            max_active_agents,
        )
        .await
    }

    pub async fn upsert_github_creator_agent_with_pioneer_code(
        &self,
        fallback_agent_id: &AgentId,
        github_user_id: i64,
        github_login: &str,
        pioneer_code_hash: Option<&str>,
        require_pioneer_code: bool,
        max_active_agents: i64,
    ) -> Result<AgentId> {
        db::sessions::upsert_github_creator_agent_with_pioneer_code(
            self.pool,
            fallback_agent_id,
            github_user_id,
            github_login,
            pioneer_code_hash,
            require_pioneer_code,
            max_active_agents,
        )
        .await
    }

    pub async fn create_github_oauth_state(
        &self,
        input: &CreateGithubOauthStateInput,
    ) -> Result<()> {
        db::sessions::create_github_oauth_state(self.pool, input).await
    }

    pub async fn consume_github_oauth_state(
        &self,
        state_hash: &str,
        browser_nonce_hash: &str,
    ) -> Result<Option<ConsumedGithubOauthState>> {
        db::sessions::consume_github_oauth_state(self.pool, state_hash, browser_nonce_hash).await
    }

    pub async fn create_creator_session(&self, input: &CreateCreatorSessionInput) -> Result<()> {
        db::sessions::create_creator_session(self.pool, input).await
    }

    pub async fn create_admin_session(&self, input: &CreateAdminSessionInput) -> Result<()> {
        db::sessions::create_admin_session(self.pool, input).await
    }

    pub async fn authenticate_creator(
        &self,
        session_token: &str,
    ) -> Result<Option<AuthenticatedCreatorSession>> {
        db::sessions::authenticate_creator_session(self.pool, session_token).await
    }

    pub async fn authenticate_admin(
        &self,
        session_token: &str,
    ) -> Result<Option<AuthenticatedAdminSession>> {
        db::sessions::authenticate_admin_session(self.pool, session_token).await
    }

    pub async fn delete_web_session_by_token(&self, session_token: &str) -> Result<()> {
        db::sessions::delete_web_session_by_token(self.pool, session_token).await
    }

    pub async fn delete_expired_web_auth_rows(&self) -> Result<()> {
        db::sessions::delete_expired_web_auth_rows(self.pool).await
    }
}
