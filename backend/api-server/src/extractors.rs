use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Request},
    http::{Method, header, request::Parts},
};
use garde::Validate;
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;

use agentics_domain::error::ServiceError;
use agentics_domain::models::ErrorDetail;
use agentics_domain::models::ids::{AgentId, AgentTokenId, ChallengeDraftId, SolutionSubmissionId};
use agentics_persistence::{AuthenticatedAdminSession, AuthenticatedCreatorSession, Repositories};
use agentics_services::auth;

use crate::admin_auth_throttle::remote_addr_from_parts;
use crate::error::ApiError;
use crate::state::AppState;

const X_AGENTICS_ADMIN_AUTOMATION: &str = "x-agentics-admin-automation";

/// Validated solution-submission id extracted from a route path.
///
/// Put this extractor before authentication extractors in handler signatures when
/// malformed ids should fail before auth and database lookup.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionPath(pub SolutionSubmissionId);

impl FromRequestParts<AppState> for SolutionSubmissionPath {
    type Rejection = ApiError;

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
    type Rejection = ApiError;

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
    type Rejection = ApiError;

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

        let agent = Repositories::new(&state.db)
            .agents()
            .authenticate_token(&parsed.token)
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
    type Rejection = ApiError;

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
            if parsed.username == state.config.auth.admin_username
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
            if let Err(ServiceError::TooManyRequests(message)) = state
                .admin_auth_throttle
                .record_failed_attempt(&parsed.username, &remote_addr)
            {
                return Err(ServiceError::too_many_requests(message).into());
            }
            return Err(unauthorized("需要有效的 admin basic auth"));
        }

        let session_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.api_web.web_session_cookie_name,
        )
        .ok_or_else(|| unauthorized("需要有效的 admin session 或 basic auth"))?;

        let session = Repositories::new(&state.db)
            .sessions()
            .authenticate_admin(&session_token)
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
    type Rejection = ApiError;

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
            &state.config.api_web.web_session_cookie_name,
        )
        .ok_or_else(|| unauthorized("需要有效的 creator session"))?;

        let session = Repositories::new(&state.db)
            .sessions()
            .authenticate_creator(&session_token)
            .await
            .map_err(|_| unauthorized("creator session 无效或已过期"))?
            .ok_or_else(|| unauthorized("creator session 无效或已过期"))?;

        require_session_csrf(parts, &session)?;
        let csrf_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.api_web.web_csrf_cookie_name,
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

impl WebSessionCsrf for AuthenticatedCreatorSession {
    /// Returns the hashed CSRF token for a creator web session.
    fn csrf_token_hash(&self) -> &str {
        &self.csrf_token_hash
    }
}

/// Validates CSRF headers for state-changing web-session requests.
fn require_session_csrf<S: WebSessionCsrf>(parts: &Parts, session: &S) -> Result<(), ApiError> {
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
fn require_basic_admin_automation_header(parts: &Parts) -> Result<(), ApiError> {
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
fn unauthorized(message: &str) -> ApiError {
    ServiceError::unauthorized(message).into()
}

/// Builds a localized forbidden API rejection.
fn forbidden(message: &str) -> ApiError {
    ServiceError::Forbidden(message.to_string()).into()
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
/// strings while preserving the API error envelope.
pub trait ValidateRequest {
    /// Performs semantic validation after serde has accepted the request shape.
    fn validate(&self) -> std::result::Result<(), ServiceError>;
}

/// Axum JSON extractor that rejects malformed or semantically invalid request bodies.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + ValidateRequest,
{
    type Rejection = ApiError;

    /// Deserializes a JSON body and runs the request's semantic validator.
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| bad_request(&rejection.body_text()))?;

        value.validate()?;

        Ok(Self(value))
    }
}

/// Builds a structured bad-request rejection for JSON and path validation failures.
fn bad_request(message: &str) -> ApiError {
    ServiceError::bad_request(message).into()
}

impl<T> ValidateRequest for T
where
    T: Validate<Context = ()>,
{
    /// Runs derived `garde` request validators and maps them into the API error envelope.
    fn validate(&self) -> std::result::Result<(), ServiceError> {
        Validate::validate(self).map_err(garde_report_to_service_error)
    }
}

fn garde_report_to_service_error(report: garde::Report) -> ServiceError {
    let details = report
        .into_inner()
        .into_iter()
        .map(|(path, error)| ErrorDetail {
            field: (!path.is_empty()).then(|| path.to_string()),
            message: error.to_string(),
        })
        .collect::<Vec<_>>();
    let message = details
        .first()
        .map(|detail| match &detail.field {
            Some(field) => format!("{field}: {}", detail.message),
            None => detail.message.clone(),
        })
        .unwrap_or_else(|| "request validation failed".to_string());
    ServiceError::validation_failed(message, details)
}
