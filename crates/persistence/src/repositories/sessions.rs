use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    AdminServiceTokenRecord, AuthenticatedAdminServiceToken, AuthenticatedCreatorApiToken,
    AuthenticatedHumanSession, ConsumedGithubSignInState, CreateAdminServiceTokenInput,
    CreateCreatorApiTokenInput, CreateGithubSignInStateInput, CreateHumanSessionInput,
    CreatorApiTokenRecord, HumanRecord, ResolveGithubHumanInput,
};
use agentics_domain::models::ids::{AdminServiceTokenId, CreatorApiTokenId, HumanId};
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct SessionsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl SessionsRepository<'_> {
    pub async fn resolve_github_human(
        &self,
        input: &ResolveGithubHumanInput,
    ) -> Result<HumanRecord> {
        db::sessions::resolve_github_human(self.pool, input).await
    }

    pub async fn create_github_sign_in_state(
        &self,
        input: &CreateGithubSignInStateInput,
    ) -> Result<()> {
        db::sessions::create_github_sign_in_state(self.pool, input).await
    }

    pub async fn consume_github_sign_in_state(
        &self,
        state_hash: &str,
        browser_nonce_hash: &str,
    ) -> Result<Option<ConsumedGithubSignInState>> {
        db::sessions::consume_github_sign_in_state(self.pool, state_hash, browser_nonce_hash).await
    }

    pub async fn create_human_session(&self, input: &CreateHumanSessionInput) -> Result<()> {
        db::sessions::create_human_session(self.pool, input).await
    }

    pub async fn authenticate_human(
        &self,
        session_token: &str,
    ) -> Result<Option<AuthenticatedHumanSession>> {
        db::sessions::authenticate_human_session(self.pool, session_token).await
    }

    pub async fn complete_human_setup(
        &self,
        human_id: &HumanId,
        code_hash: &str,
    ) -> Result<HumanRecord> {
        db::sessions::complete_human_setup(self.pool, human_id, code_hash).await
    }

    pub async fn delete_human_session_by_token(&self, session_token: &str) -> Result<()> {
        db::sessions::delete_human_session_by_token(self.pool, session_token).await
    }

    pub async fn list_humans(&self) -> Result<Vec<HumanRecord>> {
        db::sessions::list_humans(self.pool).await
    }

    pub async fn get_human_by_id(&self, human_id: &HumanId) -> Result<HumanRecord> {
        db::sessions::get_human_by_id(self.pool, human_id).await
    }

    pub async fn grant_admin_role(
        &self,
        human_id: &HumanId,
        granted_by_human_id: &HumanId,
    ) -> Result<HumanRecord> {
        db::sessions::grant_admin_role(self.pool, human_id, granted_by_human_id).await
    }

    pub async fn revoke_admin_role(
        &self,
        human_id: &HumanId,
        revoked_by_human_id: &HumanId,
    ) -> Result<HumanRecord> {
        db::sessions::revoke_admin_role(self.pool, human_id, revoked_by_human_id).await
    }

    pub async fn create_admin_service_token(
        &self,
        input: &CreateAdminServiceTokenInput,
    ) -> Result<AdminServiceTokenRecord> {
        db::sessions::create_admin_service_token(self.pool, input).await
    }

    pub async fn list_admin_service_tokens(&self) -> Result<Vec<AdminServiceTokenRecord>> {
        db::sessions::list_admin_service_tokens(self.pool).await
    }

    pub async fn revoke_admin_service_token(
        &self,
        id: &AdminServiceTokenId,
        revoked_by_human_id: &HumanId,
    ) -> Result<AdminServiceTokenRecord> {
        db::sessions::revoke_admin_service_token(self.pool, id, revoked_by_human_id).await
    }

    pub async fn authenticate_admin_service_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedAdminServiceToken>> {
        db::sessions::authenticate_admin_service_token(self.pool, token_hash).await
    }

    pub async fn create_creator_api_token(
        &self,
        input: &CreateCreatorApiTokenInput,
    ) -> Result<CreatorApiTokenRecord> {
        db::sessions::create_creator_api_token(self.pool, input).await
    }

    pub async fn list_creator_api_tokens(
        &self,
        human_id: &HumanId,
    ) -> Result<Vec<CreatorApiTokenRecord>> {
        db::sessions::list_creator_api_tokens(self.pool, human_id).await
    }

    pub async fn revoke_creator_api_token(
        &self,
        human_id: &HumanId,
        id: &CreatorApiTokenId,
    ) -> Result<CreatorApiTokenRecord> {
        db::sessions::revoke_creator_api_token(self.pool, human_id, id).await
    }

    pub async fn authenticate_creator_api_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedCreatorApiToken>> {
        db::sessions::authenticate_creator_api_token(self.pool, token_hash).await
    }

    pub async fn delete_expired_web_auth_rows(&self) -> Result<()> {
        db::sessions::delete_expired_web_auth_rows(self.pool).await
    }
}
