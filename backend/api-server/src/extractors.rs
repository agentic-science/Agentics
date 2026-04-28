use axum::{
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::Response,
};

use shared::auth;
use shared::db::queries::authenticate_agent_token;

use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AgentAuth {
    pub agent_id: String,
    pub token_id: String,
    pub name: String,
}

impl FromRequestParts<AppState> for AgentAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
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
            token_id: agent.token_id,
            name: agent.name,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AdminAuth;

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = (StatusCode, axum::Json<shared::models::ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let parsed = auth::parse_basic_auth(auth_header)
            .ok_or_else(|| unauthorized("需要有效的 admin basic auth"))?;

        if parsed.username != state.config.admin_username || parsed.password != state.config.admin_password {
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

// Middleware to inject AppState into request extensions for extractors
pub async fn inject_state(
    state: axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    request.extensions_mut().insert(state.0.clone());
    next.run(request).await
}
