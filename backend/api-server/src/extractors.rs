use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Request},
    http::{Method, header, request::Parts},
};
use garde::Validate;
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;

use agentics_domain::models::ErrorDetail;
use agentics_domain::models::auth::{GithubUserId, HumanRole};
use agentics_domain::models::ids::{
    AdminServiceTokenId, AgentId, AgentTokenId, ChallengeReviewRecordId, HumanId, HumanSessionId,
    SolutionSubmissionId,
};
use agentics_error::ServiceError;
use agentics_persistence::{AuthenticatedHumanSession, Repositories};
use agentics_services::auth;

use crate::error::ApiError;
use crate::state::AppState;

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

/// Validated challenge-review-record id extracted from a route path parameter.
///
/// This is HTTP framework glue, not a filesystem or storage path. Its only
/// responsibility is to parse `challenge_review_record_id` before handlers perform
/// authorization or database lookup, so malformed UUIDs fail as `400 bad_request`
/// instead of surfacing later as SQL cast errors.
#[derive(Debug, Clone)]
pub struct ChallengeReviewRecordIdPath(pub ChallengeReviewRecordId);

impl FromRequestParts<AppState> for ChallengeReviewRecordIdPath {
    type Rejection = ApiError;

    /// Parses the path segment as a canonical challenge-review-record id.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| bad_request("challenge_review_record_id path parameter is required"))?;
        let id = ChallengeReviewRecordId::try_new(raw).map_err(|e| bad_request(&e.to_string()))?;
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

/// Actor that authenticated one admin request.
#[derive(Debug, Clone)]
pub enum AdminActor {
    Human {
        human_id: HumanId,
        github_user_id: GithubUserId,
        github_login: String,
    },
    ServiceToken {
        token_id: AdminServiceTokenId,
        label: String,
    },
}

impl AdminActor {
    /// Stable display string for audit events.
    pub fn display(&self) -> String {
        match self {
            Self::Human { github_login, .. } => format!("@{github_login}"),
            Self::ServiceToken { label, .. } => format!("service-token:{label}"),
        }
    }

    pub fn human_id(&self) -> Option<&HumanId> {
        match self {
            Self::Human { human_id, .. } => Some(human_id),
            Self::ServiceToken { .. } => None,
        }
    }

    pub fn service_token_id(&self) -> Option<&AdminServiceTokenId> {
        match self {
            Self::Human { .. } => None,
            Self::ServiceToken { token_id, .. } => Some(token_id),
        }
    }
}

/// Marker extractor for routes that require administrator authentication.
#[derive(Debug, Clone)]
pub struct AdminAuth {
    pub actor: AdminActor,
}

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = ApiError;

    /// Authenticates admin requests through human sessions or service-token bearer auth.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        if let Some(parsed) = auth::parse_bearer_token(auth_header) {
            let token_hash = auth::hash_opaque_token(parsed.token.expose_secret());
            let token = Repositories::new(&state.db)
                .sessions()
                .authenticate_admin_service_token(&token_hash)
                .await
                .map_err(|_| unauthorized("admin service token 无效或已被撤销"))?
                .ok_or_else(|| unauthorized("admin service token 无效或已被撤销"))?;
            return Ok(AdminAuth {
                actor: AdminActor::ServiceToken {
                    token_id: token.token_id,
                    label: token.label,
                },
            });
        }

        let session =
            authenticate_human_session_parts(parts, state, "admin session 无效或已过期").await?;
        require_session_csrf(parts, &session)?;
        if !session.roles.contains(&HumanRole::Admin) {
            return Err(forbidden("需要 admin 权限"));
        }

        Ok(AdminAuth {
            actor: AdminActor::Human {
                human_id: session.human_id,
                github_user_id: session.github_user_id,
                github_login: session.github_login,
            },
        })
    }
}

/// GitHub sign-in-authenticated human context from a web session.
#[derive(Debug, Clone)]
pub struct HumanAuth {
    pub session_id: HumanSessionId,
    pub human_id: HumanId,
    pub github_user_id: GithubUserId,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub csrf_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl FromRequestParts<AppState> for HumanAuth {
    type Rejection = ApiError;

    /// Authenticates human web requests through the GitHub-linked session cookie.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session =
            authenticate_human_session_parts(parts, state, "human session 无效或已过期").await?;
        require_session_csrf(parts, &session)?;
        let csrf_token = cookie_value(
            parts
                .headers
                .get(header::COOKIE)
                .and_then(|h| h.to_str().ok()),
            &state.config.api_web.web_csrf_cookie_name,
        );

        Ok(HumanAuth {
            session_id: session.session_id,
            human_id: session.human_id,
            github_user_id: session.github_user_id,
            github_login: session.github_login,
            roles: session.roles,
            csrf_token,
            expires_at: session.expires_at,
        })
    }
}

/// Human-authenticated challenge creator context from a web session.
#[derive(Debug, Clone)]
pub struct CreatorAuth {
    pub session_id: HumanSessionId,
    pub human_id: HumanId,
    pub github_user_id: GithubUserId,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub csrf_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl FromRequestParts<AppState> for CreatorAuth {
    type Rejection = ApiError;

    /// Authenticates creator web requests through a human session with creator role.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let human = HumanAuth::from_request_parts(parts, state).await?;
        if !human.roles.contains(&HumanRole::Creator) {
            return Err(forbidden("需要 creator 权限"));
        }
        Ok(Self {
            session_id: human.session_id,
            human_id: human.human_id,
            github_user_id: human.github_user_id,
            github_login: human.github_login,
            roles: human.roles,
            csrf_token: human.csrf_token,
            expires_at: human.expires_at,
        })
    }
}

/// Shared interface for database session records that carry CSRF token hashes.
trait WebSessionCsrf {
    /// Returns the hashed CSRF token stored with the web session.
    fn csrf_token_hash(&self) -> &str;
}

impl WebSessionCsrf for AuthenticatedHumanSession {
    /// Returns the hashed CSRF token for a human web session.
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

async fn authenticate_human_session_parts(
    parts: &Parts,
    state: &AppState,
    unauthorized_message: &str,
) -> Result<AuthenticatedHumanSession, ApiError> {
    let session_token = cookie_value(
        parts
            .headers
            .get(header::COOKIE)
            .and_then(|h| h.to_str().ok()),
        &state.config.api_web.web_session_cookie_name,
    )
    .ok_or_else(|| unauthorized(unauthorized_message))?;

    Repositories::new(&state.db)
        .sessions()
        .authenticate_human(&session_token)
        .await
        .map_err(|_| unauthorized(unauthorized_message))?
        .ok_or_else(|| unauthorized(unauthorized_message))
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
