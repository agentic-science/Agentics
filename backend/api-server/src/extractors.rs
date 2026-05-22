use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Request},
    http::{Method, StatusCode, header, request::Parts},
};
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;

use shared::auth;
use shared::db::{
    AuthenticatedAdminSession, authenticate_admin_session, authenticate_agent_token,
    authenticate_creator_session,
};
use shared::error::AppError;
use shared::models::auth::GithubOauthCallbackRequest;
use shared::models::auth::GithubOauthLoginRequest;
use shared::models::challenge_creation::{
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use shared::models::ids::{AgentId, AgentTokenId, ChallengeDraftId, SolutionSubmissionId};
use shared::models::request::{
    CreateChallengeRequest, CreateChallengeShortlistRevisionRequest, CreatePioneerCodeRequest,
    CreateSolutionSubmissionRequest, PublishChallengeRequest, RegisterAgentRequest,
    SetChallengeMoltbookDiscussionRequest,
};
use shared::validation::text;

use crate::admin_auth_throttle::remote_addr_from_parts;
use crate::state::AppState;

const X_AGENTICS_ADMIN_AUTOMATION: &str = "x-agentics-admin-automation";

/// Validated solution-submission id extracted from a route path.
///
/// Put this extractor before authentication extractors in handler signatures when
/// malformed ids should fail before auth and database lookup.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionPath(pub SolutionSubmissionId);

impl FromRequestParts<AppState> for SolutionSubmissionPath {
    type Rejection = (StatusCode, Json<shared::models::ErrorResponse>);

    /// Parses the path segment as a canonical solution-submission id.
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

/// Validated challenge-draft id extracted from a route path parameter.
///
/// This is HTTP framework glue, not a filesystem or storage path. Its only
/// responsibility is to parse `challenge_draft_id` before handlers perform
/// authorization or database lookup, so malformed UUIDs fail as `400 bad_request`
/// instead of surfacing later as SQL cast errors.
#[derive(Debug, Clone)]
pub struct ChallengeDraftIdPath(pub ChallengeDraftId);

impl FromRequestParts<AppState> for ChallengeDraftIdPath {
    type Rejection = (StatusCode, Json<shared::models::ErrorResponse>);

    /// Parses the path segment as a canonical challenge-draft id.
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
    pub _token_id: AgentTokenId,
    pub _display_name: String,
}

impl FromRequestParts<AppState> for AgentAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    /// Authenticates the bearer token and returns the active agent context.
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
            agent_id: agent.agent_id,
            _token_id: agent.token_id,
            _display_name: agent.display_name,
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

    /// Authenticates admin requests through Basic auth or session cookies plus CSRF.
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
                && state
                    .config
                    .admin_password_matches(parsed.password.expose_secret())
            {
                require_basic_admin_automation_header(parts)?;
                return Ok(AdminAuth {
                    username: parsed.username,
                });
            }
            let remote_addr = remote_addr_from_parts(parts);
            if let Err(AppError::TooManyRequests(message)) = state
                .admin_auth_throttle
                .record_failed_attempt(&parsed.username, &remote_addr)
            {
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    axum::Json(shared::models::ErrorResponse {
                        error: "too_many_requests".to_string(),
                        message,
                    }),
                ));
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
    pub csrf_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl FromRequestParts<AppState> for CreatorAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    /// Authenticates creator web requests through the GitHub-linked session cookie.
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
        let csrf_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.web_csrf_cookie_name,
        );

        Ok(CreatorAuth {
            session_id: session.session_id,
            agent_id: AgentId::try_new(session.agent_id)
                .map_err(|_| unauthorized("creator session 无效或已过期"))?,
            github_user_id: session.github_user_id,
            github_login: session.github_login,
            csrf_token,
            expires_at: session.expires_at,
        })
    }
}

/// Shared interface for database session records that carry CSRF token hashes.
trait WebSessionCsrf {
    /// Returns the hashed CSRF token stored with the web session.
    fn csrf_token_hash(&self) -> &str;
}

impl WebSessionCsrf for AuthenticatedAdminSession {
    /// Returns the hashed CSRF token for an admin web session.
    fn csrf_token_hash(&self) -> &str {
        &self.csrf_token_hash
    }
}

impl WebSessionCsrf for shared::db::AuthenticatedCreatorSession {
    /// Returns the hashed CSRF token for a creator web session.
    fn csrf_token_hash(&self) -> &str {
        &self.csrf_token_hash
    }
}

/// Validates CSRF headers for state-changing web-session requests.
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

/// Requires an explicit non-simple header before Basic-auth admin mutations.
fn require_basic_admin_automation_header(
    parts: &Parts,
) -> Result<(), (StatusCode, axum::Json<shared::models::ErrorResponse>)> {
    if !requires_csrf(&parts.method) {
        return Ok(());
    }
    let allowed = parts
        .headers
        .get(X_AGENTICS_ADMIN_AUTOMATION)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|value| value == "true" || value == "1");
    if allowed {
        Ok(())
    } else {
        Err(forbidden(
            "admin Basic-auth mutations require x-agentics-admin-automation: true",
        ))
    }
}

/// Builds a localized unauthorized API rejection.
fn unauthorized(message: &str) -> (StatusCode, axum::Json<shared::models::ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(shared::models::ErrorResponse {
            error: "unauthorized".to_string(),
            message: message.to_string(),
        }),
    )
}

/// Builds a localized forbidden API rejection.
fn forbidden(message: &str) -> (StatusCode, axum::Json<shared::models::ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        axum::Json(shared::models::ErrorResponse {
            error: "forbidden".to_string(),
            message: message.to_string(),
        }),
    )
}

/// Returns whether an HTTP method can mutate server state and therefore needs CSRF.
fn requires_csrf(method: &Method) -> bool {
    !(method == Method::GET || method == Method::HEAD || method == Method::OPTIONS)
}

/// Extracts one cookie value from a raw Cookie header without accepting partial name matches.
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
    /// Performs semantic validation after serde has accepted the request shape.
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

    /// Deserializes a JSON body and runs the request's semantic validator.
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| bad_request(&rejection.body_text()))?;

        value.validate().map_err(|message| bad_request(&message))?;

        Ok(Self(value))
    }
}

/// Builds a structured bad-request rejection for JSON and path validation failures.
fn bad_request(message: &str) -> (StatusCode, Json<shared::models::ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(shared::models::ErrorResponse {
            error: "bad_request".to_string(),
            message: message.to_string(),
        }),
    )
}

/// Validates that a string request field has visible, non-whitespace content.
fn require_non_empty(value: &str, field: &str) -> Result<(), String> {
    text::require_non_empty(value, field).map_err(|error| match error {
        AppError::Validation(message) => message,
        other => other.to_string(),
    })
}

impl ValidateRequest for RegisterAgentRequest {
    /// Ensures agent registration provides a display name.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.display_name, "display_name")
    }
}

impl ValidateRequest for CreatePioneerCodeRequest {
    /// Defers pioneer-code semantics to the handler, which needs runtime config.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

impl ValidateRequest for CreateChallengeDraftRequest {
    /// Ensures GitHub draft metadata has positive numeric identifiers.
    fn validate(&self) -> Result<(), String> {
        if self.pr_author_github_user_id <= 0 {
            return Err("pr_author_github_user_id must be greater than zero".to_string());
        }
        Ok(())
    }
}

impl ValidateRequest for GithubOauthLoginRequest {
    /// Defers pioneer-code semantics to the handler, which needs runtime config.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

impl ValidateRequest for GithubOauthCallbackRequest {
    /// Ensures the browser returned both OAuth values before backend exchange.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.code, "code")?;
        require_non_empty(&self.state, "state")
    }
}

impl ValidateRequest for UploadChallengePrivateAssetRequest {
    /// Ensures private asset uploads contain an encoded ZIP payload.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.asset_base64, "asset_base64")
    }
}

impl ValidateRequest for ValidateChallengeDraftRequest {
    /// Ensures draft validation references a local checkout path.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.repository_path, "repository_path")
    }
}

impl ValidateRequest for ReviewChallengeDraftRequest {
    /// Accepts review decisions because serde has already validated the enum payload.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

impl ValidateRequest for CreateSolutionSubmissionRequest {
    /// Ensures solution submissions contain an encoded artifact payload.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.artifact_base64, "artifact_base64")
    }
}

impl ValidateRequest for CreateChallengeShortlistRevisionRequest {
    /// Ensures shortlist updates add at least one agent id.
    fn validate(&self) -> Result<(), String> {
        if self.agent_ids_to_add.is_empty() {
            return Err("agent_ids_to_add must contain at least one agent id".to_string());
        }
        Ok(())
    }
}

impl ValidateRequest for CreateChallengeRequest {
    /// Ensures direct admin challenge creation includes public display text.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.title, "title")?;
        require_non_empty(&self.summary.en, "summary.en")?;
        require_non_empty(&self.summary.zh, "summary.zh")
    }
}

impl ValidateRequest for PublishChallengeRequest {
    /// Ensures direct admin publishing references a bundle path.
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.bundle_path, "bundle_path")
    }
}

impl ValidateRequest for SetChallengeMoltbookDiscussionRequest {
    /// URL syntax is validated by the typed request field during deserialization.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
