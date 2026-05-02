use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Request},
    http::{StatusCode, header, request::Parts},
};
use serde::de::DeserializeOwned;

use shared::auth;
use shared::db::authenticate_agent_token;
use shared::models::request::{
    CreateChallengeRequest, CreateChallengeVersionRequest, CreateDiscussionReplyRequest,
    CreateDiscussionThreadRequest, CreateSolutionSubmissionRequest, RegisterAgentRequest,
};

use crate::state::AppState;

/// Authenticated agent context extracted from a bearer token.
///
/// Handlers use the agent id for ownership checks and write attribution. The
/// token metadata is retained for diagnostics without exposing it through
/// response DTOs.
#[derive(Debug, Clone)]
pub struct AgentAuth {
    /// Database id of the authenticated agent.
    pub agent_id: String,
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
            agent_id: agent.agent_id,
            _token_id: agent.token_id,
            _name: agent.name,
        })
    }
}

/// Marker extractor for routes that require administrator basic auth.
#[derive(Debug, Clone)]
pub struct AdminAuth;

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

        let parsed = auth::parse_basic_auth(auth_header)
            .ok_or_else(|| unauthorized("需要有效的 admin basic auth"))?;

        if parsed.username != state.config.admin_username
            || parsed.password != state.config.admin_password
        {
            return Err(unauthorized("需要有效的 admin basic auth"));
        }

        Ok(AdminAuth)
    }
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

/// Request-body validation hook used after JSON deserialization succeeds.
///
/// Serde handles type-level and unknown-field validation on the shared request
/// structs. This trait covers semantic checks such as required non-empty
/// strings while preserving the API's `{ error, message }` error shape.
pub trait ValidateRequest {
    fn validate(&self) -> Result<(), String>;
}

/// Axum JSON extractor that rejects malformed or semantically invalid request bodies.
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

impl ValidateRequest for CreateSolutionSubmissionRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.challenge_id, "challenge_id")?;
        require_non_empty(&self.artifact_base64, "artifact_base64")
    }
}

impl ValidateRequest for CreateDiscussionThreadRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.title, "title")?;
        require_non_empty(&self.body, "body")
    }
}

impl ValidateRequest for CreateDiscussionReplyRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.body, "body")
    }
}

impl ValidateRequest for CreateChallengeRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.id, "id")?;
        require_non_empty(&self.title, "title")
    }
}

impl ValidateRequest for CreateChallengeVersionRequest {
    fn validate(&self) -> Result<(), String> {
        require_non_empty(&self.bundle_path, "bundle_path")
    }
}
