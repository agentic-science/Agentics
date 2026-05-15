use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Request},
    http::{Method, StatusCode, header, request::Parts},
};
use serde::de::DeserializeOwned;

use shared::auth;
use shared::db::{
    AuthenticatedAdminSession, authenticate_admin_session, authenticate_agent_token,
    authenticate_creator_session,
};
use shared::models::challenge_creation::{
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use shared::models::ids::{AgentId, ChallengeDraftId, SolutionSubmissionId};
use shared::models::request::{
    CreateChallengeRequest, CreateChallengeShortlistRevisionRequest,
    CreateSolutionSubmissionRequest, PublishChallengeRequest, RegisterAgentRequest,
};

use crate::state::AppState;

/// Validated solution-submission id extracted from a route path.
///
/// Put this extractor before authentication extractors in handler signatures when
/// malformed ids should fail before auth and database lookup.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionPath(pub SolutionSubmissionId);

impl FromRequestParts<AppState> for SolutionSubmissionPath {
    type Rejection = (StatusCode, Json<shared::models::ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| bad_request("solution_submission_id path parameter is required"))?;
        let id = SolutionSubmissionId::try_new(raw).map_err(|e| bad_request(&e.to_string()))?;
        Ok(Self(id))
    }
}

/// Validated challenge-draft id extracted from a route path.
#[derive(Debug, Clone)]
pub struct ChallengeDraftPath(pub ChallengeDraftId);

impl FromRequestParts<AppState> for ChallengeDraftPath {
    type Rejection = (StatusCode, Json<shared::models::ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| bad_request("challenge_draft_id path parameter is required"))?;
        let id = ChallengeDraftId::try_new(raw).map_err(|e| bad_request(&e.to_string()))?;
        Ok(Self(id))
    }
}

/// Authenticated agent context extracted from a bearer token.
///
/// Handlers use the agent id for ownership checks and write attribution. The
/// token metadata is retained for diagnostics without exposing it through
/// response DTOs.
#[derive(Debug, Clone)]
pub struct AgentAuth {
    /// Database id of the authenticated agent.
    pub agent_id: AgentId,
    pub _token_id: String,
    pub _name: String,
}

impl FromRequestParts<AppState> for AgentAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let parsed = auth::parse_bearer_token(auth_header)
            .ok_or_else(|| unauthorized("缺少有效的 Bearer token"))?;

        let agent = authenticate_agent_token(&state.db, &parsed.token)
            .await
            .map_err(|_| unauthorized("token 无效或已被撤销"))?
            .ok_or_else(|| unauthorized("token 无效或已被撤销"))?;

        Ok(AgentAuth {
            agent_id: AgentId::try_new(agent.agent_id)
                .map_err(|_| unauthorized("token 无效或已被撤销"))?,
            _token_id: agent.token_id,
            _name: agent.name,
        })
    }
}

/// Marker extractor for routes that require administrator authentication.
///
/// Server-side tools can continue to use Basic auth. Browser routes should use
/// the session-cookie path issued by `/api/auth/admin/login`, which keeps the
/// reusable admin password out of browser storage and request logs.
#[derive(Debug, Clone)]
pub struct AdminAuth {
    pub username: String,
}

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        if let Some(parsed) = auth::parse_basic_auth(auth_header) {
            if parsed.username == state.config.admin_username
                && parsed.password == state.config.admin_password
            {
                return Ok(AdminAuth {
                    username: parsed.username,
                });
            }
            return Err(unauthorized("需要有效的 admin basic auth"));
        }

        let session_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.web_session_cookie_name,
        )
        .ok_or_else(|| unauthorized("需要有效的 admin session 或 basic auth"))?;

        let session = authenticate_admin_session(&state.db, &session_token)
            .await
            .map_err(|_| unauthorized("admin session 无效或已过期"))?
            .ok_or_else(|| unauthorized("admin session 无效或已过期"))?;

        require_session_csrf(parts, &session)?;

        Ok(AdminAuth {
            username: session.admin_username,
        })
    }
}

/// GitHub OAuth-authenticated challenge creator context from a web session.
#[derive(Debug, Clone)]
pub struct CreatorAuth {
    pub session_id: String,
    pub agent_id: AgentId,
    pub github_user_id: i64,
    pub github_login: String,
}

impl FromRequestParts<AppState> for CreatorAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.web_session_cookie_name,
        )
        .ok_or_else(|| unauthorized("需要有效的 creator session"))?;

        let session = authenticate_creator_session(&state.db, &session_token)
            .await
            .map_err(|_| unauthorized("creator session 无效或已过期"))?
            .ok_or_else(|| unauthorized("creator session 无效或已过期"))?;

        require_session_csrf(parts, &session)?;

        Ok(CreatorAuth {
            session_id: session.session_id,
            agent_id: AgentId::try_new(session.agent_id)
                .map_err(|_| unauthorized("creator session 无效或已过期"))?,
            github_user_id: session.github_user_id,
            github_login: session.github_login,
        })
    }
}

trait WebSessionCsrf {
    fn csrf_token_hash(&self) -> &str;
}

impl WebSessionCsrf for AuthenticatedAdminSession {
    fn csrf_token_hash(&self) -> &str {
        &self.csrf_token_hash
    }
}

impl WebSessionCsrf for shared::db::AuthenticatedCreatorSession {
    fn csrf_token_hash(&self) -> &str {
        &self.csrf_token_hash
    }
}

fn require_session_csrf<S: WebSessionCsrf>(
    parts: &Parts,
    session: &S,
) -> Result<(), (StatusCode, axum::Json<shared::models::ErrorResponse>)> {
    if !requires_csrf(&parts.method) {
        return Ok(());
    }
    let csrf_token = parts
        .headers
        .get("x-agentics-csrf-token")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| forbidden("缺少有效的 CSRF token"))?;
    let csrf_hash = auth::hash_opaque_token(csrf_token);
    if csrf_hash != session.csrf_token_hash() {
        return Err(forbidden("缺少有效的 CSRF token"));
    }
    Ok(())
}

fn unauthorized(message: &str) -> (StatusCode, axum::Json<shared::models::ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(shared::models::ErrorResponse {
            error: "unauthorized".to_string(),
            message: message.to_string(),
        }),
    )
}

fn forbidden(message: &str) -> (StatusCode, axum::Json<shared::models::ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        axum::Json(shared::models::ErrorResponse {
            error: "forbidden".to_string(),
            message: message.to_string(),
        }),
    )
}

fn requires_csrf(method: &Method) -> bool {
    !(method == Method::GET || method == Method::HEAD || method == Method::OPTIONS)
}

fn cookie_value(cookie_header: Option<&str>, name: &str) -> Option<String> {
    let cookie_header = cookie_header?;
    for pair in cookie_header.split(';') {
        if let Some((candidate_name, value)) = pair.trim().split_once('=')
            && candidate_name == name
        {
            return Some(value.to_string());
        }
    }
    None
}

/// Request-body validation hook used after JSON deserialization succeeds.
///
/// Serde handles type-level and unknown-field validation on the shared request
/// structs. This trait covers semantic checks such as required non-empty
/// strings while preserving the API's `{ error, message }` error shape.
pub trait ValidateRequest {
    fn validate(&self) -> Result<(), String>;
}

/// Axum JSON extractor that rejects malformed or semantically invalid request bodies.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + ValidateRequest,
{
    type Rejection = (StatusCode, Json<shared::models::ErrorResponse>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| bad_request(&rejection.body_text()))?;

        value.validate().map_err(|message| bad_request(&message))?;

        Ok(Self(value))
    }
}

fn bad_request(message: &str) -> (StatusCode, Json<shared::models::ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(shared::models::ErrorResponse {
            error: "bad_request".to_string(),
            message: message.to_string(),
        }),
    )
}

fn require_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} 不能为空"));
    }

    Ok(())
}

impl ValidateRequest for RegisterAgentRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.name, "name")
    }
}

impl ValidateRequest for CreateChallengeDraftRequest {
    fn validate(&self) -> Result<(), String> {
        if self.pr_number <= 0 {
            return Err("pr_number must be greater than zero".to_string());
        }
        if self.pr_author_github_user_id <= 0 {
            return Err("pr_author_github_user_id must be greater than zero".to_string());
        }
        Ok(())
    }
}

impl ValidateRequest for UploadChallengePrivateAssetRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.asset_base64, "asset_base64")
    }
}

impl ValidateRequest for ValidateChallengeDraftRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.repository_path, "repository_path")
    }
}

impl ValidateRequest for ReviewChallengeDraftRequest {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

impl ValidateRequest for CreateSolutionSubmissionRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.artifact_base64, "artifact_base64")
    }
}

impl ValidateRequest for CreateChallengeShortlistRevisionRequest {
    fn validate(&self) -> Result<(), String> {
        if self.agent_ids_to_add.is_empty() {
            return Err("agent_ids_to_add must contain at least one agent id".to_string());
        }
        Ok(())
    }
}

impl ValidateRequest for CreateChallengeRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.title, "title")?;
        require_non_empty(&self.summary, "summary")
    }
}

impl ValidateRequest for PublishChallengeRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.bundle_path, "bundle_path")
    }
}
