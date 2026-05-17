//! Web authentication handlers for browser-based roles.

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::AppendHeaders,
};
use chrono::{Duration, Utc};
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use shared::auth;
use shared::config::AgentRegistrationMode;
use shared::db;
use shared::error::{AppError, Result};
use shared::models::auth::{
    AdminLoginRequest, AdminSessionResponse, CreatorMeResponse, CreatorSessionResponse,
    GithubOauthCallbackQuery, GithubOauthLoginRequest, GithubOauthLoginResponse,
};
use shared::models::ids::AgentId;
use shared::models::pioneer_codes::PioneerCode;
use shared::models::urls::GithubOauthAuthorizationUrl;

use crate::extractors::{AdminAuth, CreatorAuth, ValidatedJson};
use crate::pioneer_code_security::{is_invalid_pioneer_code, reject_failed_pioneer_code};
use crate::state::AppState;

const OAUTH_STATE_TTL_MINUTES: i64 = 10;
const GITHUB_USER_AGENT: &str = "Agentics";

/// Minimal JSON shape returned by GitHub's OAuth token exchange endpoint.
#[derive(Debug, Deserialize)]
struct GithubAccessTokenResponse {
    access_token: Option<SecretString>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Minimal GitHub user profile needed to bind a creator session.
#[derive(Debug, Deserialize)]
struct GithubUserResponse {
    id: i64,
    login: String,
}

/// Authenticate an administrator and issue a browser session.
pub async fn admin_login(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<std::net::SocketAddr>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<(
    StatusCode,
    AppendHeaders<[(HeaderName, String); 2]>,
    Json<AdminSessionResponse>,
)> {
    if request.username.trim().is_empty() || request.password.expose_secret().is_empty() {
        state
            .admin_auth_throttle
            .record_failed_attempt(&request.username, &remote_addr.ip().to_string())?;
        return Err(AppError::Unauthorized);
    }
    if request.username != state.config.admin_username
        || !state
            .config
            .admin_password_matches(request.password.expose_secret())
    {
        state
            .admin_auth_throttle
            .record_failed_attempt(&request.username, &remote_addr.ip().to_string())?;
        return Err(AppError::Unauthorized);
    }
    let username = request.username.trim().to_string();

    let session_token = auth::create_web_session_token();
    let csrf_token = auth::create_csrf_token();
    let ttl_seconds = session_ttl_seconds(&state)?;
    let expires_at = session_expires_at(ttl_seconds)?;
    db::delete_expired_web_auth_rows(&state.db).await?;
    db::create_admin_session(
        &state.db,
        &db::CreateAdminSessionInput {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_token_hash: auth::hash_opaque_token(&session_token),
            csrf_token_hash: auth::hash_opaque_token(&csrf_token),
            admin_username: username.clone(),
            expires_at,
        },
    )
    .await?;

    let headers = AppendHeaders(session_cookies(
        &state,
        &session_token,
        &csrf_token,
        ttl_seconds,
    ));

    Ok((
        StatusCode::OK,
        headers,
        Json(AdminSessionResponse {
            username,
            csrf_token,
            expires_at: expires_at.to_rfc3339(),
        }),
    ))
}

/// End an administrator browser session and clear auth cookies.
pub async fn admin_logout(
    _admin: AdminAuth,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, AppendHeaders<[(HeaderName, String); 2]>)> {
    if let Some(session_token) = cookie_value(
        headers.get(header::COOKIE).and_then(|h| h.to_str().ok()),
        &state.config.web_session_cookie_name,
    ) {
        db::delete_web_session_by_token(&state.db, &session_token).await?;
    }

    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders(expired_session_cookies(&state)),
    ))
}

/// Return the current admin session when browser cookies are still valid.
pub async fn admin_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminSessionResponse>> {
    let cookie_header = headers.get(header::COOKIE).and_then(|h| h.to_str().ok());
    let session_token = cookie_value(cookie_header, &state.config.web_session_cookie_name)
        .ok_or(AppError::Unauthorized)?;
    let csrf_token = cookie_value(cookie_header, &state.config.web_csrf_cookie_name)
        .ok_or(AppError::Unauthorized)?;
    let session = db::authenticate_admin_session(&state.db, &session_token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if auth::hash_opaque_token(&csrf_token) != session.csrf_token_hash {
        return Err(AppError::Unauthorized);
    }

    Ok(Json(AdminSessionResponse {
        username: session.admin_username,
        csrf_token,
        expires_at: session.expires_at.to_rfc3339(),
    }))
}

/// Start a GitHub OAuth login for challenge creators.
pub async fn github_oauth_login(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<GithubOauthLoginRequest>,
) -> Result<Json<GithubOauthLoginResponse>> {
    let client_id = required_oauth_config(
        state.config.github_oauth_client_id.as_deref(),
        "AGENTICS_GITHUB_OAUTH_CLIENT_ID",
    )?;
    let redirect_url = required_oauth_config(
        state
            .config
            .github_oauth_redirect_url
            .as_ref()
            .map(|url| url.as_str()),
        "AGENTICS_GITHUB_OAUTH_REDIRECT_URL",
    )?;
    let state_token = auth::create_oauth_state();
    let state_hash = auth::hash_opaque_token(&state_token);
    let pioneer_code_hash = match state
        .config
        .agent_registration_mode()
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        AgentRegistrationMode::PioneerCode => {
            let Some(code) = request.pioneer_code.as_ref() else {
                return Err(reject_failed_pioneer_code().await);
            };
            let Ok(code) = PioneerCode::try_new(code.expose_secret().to_string()) else {
                return Err(reject_failed_pioneer_code().await);
            };
            let code_hash = auth::hash_opaque_token(code.expose_secret());
            if let Err(error) = db::ensure_pioneer_code_available(&state.db, &code_hash).await {
                if is_invalid_pioneer_code(&error) {
                    return Err(reject_failed_pioneer_code().await);
                }
                return Err(error);
            }
            Some(code_hash)
        }
        AgentRegistrationMode::Public => None,
    };
    let expires_at = Utc::now()
        .checked_add_signed(Duration::minutes(OAUTH_STATE_TTL_MINUTES))
        .ok_or_else(|| AppError::Internal("OAuth state TTL overflow".to_string()))?;
    db::delete_expired_web_auth_rows(&state.db).await?;
    db::create_github_oauth_state(
        &state.db,
        &db::CreateGithubOauthStateInput {
            state_hash,
            pioneer_code_hash,
            expires_at,
        },
    )
    .await?;

    let mut authorization_url = state.config.github_oauth_authorize_url.to_url();
    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_url)
        .append_pair("state", &state_token);

    let authorization_url = GithubOauthAuthorizationUrl::try_from_url(authorization_url)
        .map_err(|e| AppError::Internal(format!("generated invalid GitHub OAuth URL: {e}")))?;

    Ok(Json(GithubOauthLoginResponse {
        authorization_url,
        state: state_token,
    }))
}

/// Complete GitHub OAuth and issue a creator web session.
pub async fn github_oauth_callback(
    State(state): State<AppState>,
    ValidatedJson(query): ValidatedJson<GithubOauthCallbackQuery>,
) -> Result<(
    StatusCode,
    AppendHeaders<[(HeaderName, String); 2]>,
    Json<CreatorSessionResponse>,
)> {
    let state_hash = auth::hash_opaque_token(&query.state);
    let oauth_state = db::consume_github_oauth_state(&state.db, &state_hash)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let access_token = exchange_github_code(&state, &query.code).await?;
    let github_user = fetch_github_user(&state, &access_token).await?;
    if github_user.id <= 0 || github_user.login.trim().is_empty() {
        return Err(AppError::BadRequest(
            "GitHub OAuth returned an invalid creator identity".to_string(),
        ));
    }

    let fallback_agent_id = AgentId::generate();
    let agent_id = match db::upsert_github_creator_agent_with_pioneer_code(
        &state.db,
        &fallback_agent_id,
        github_user.id,
        github_user.login.trim(),
        oauth_state.pioneer_code_hash.as_deref(),
        i64::from(state.config.max_active_agents),
    )
    .await
    {
        Ok(agent_id) => agent_id,
        Err(error) if is_invalid_pioneer_code(&error) => {
            return Err(reject_failed_pioneer_code().await);
        }
        Err(error) => return Err(error),
    };

    let session_token = auth::create_web_session_token();
    let csrf_token = auth::create_csrf_token();
    let ttl_seconds = session_ttl_seconds(&state)?;
    let expires_at = session_expires_at(ttl_seconds)?;
    db::create_creator_session(
        &state.db,
        &db::CreateCreatorSessionInput {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_token_hash: auth::hash_opaque_token(&session_token),
            csrf_token_hash: auth::hash_opaque_token(&csrf_token),
            agent_id: agent_id.as_str().to_string(),
            github_user_id: github_user.id,
            github_login: github_user.login.trim().to_string(),
            expires_at,
        },
    )
    .await?;

    let headers = AppendHeaders(session_cookies(
        &state,
        &session_token,
        &csrf_token,
        ttl_seconds,
    ));

    Ok((
        StatusCode::OK,
        headers,
        Json(CreatorSessionResponse {
            agent_id,
            github_user_id: github_user.id,
            github_login: github_user.login,
            csrf_token,
            expires_at: expires_at.to_rfc3339(),
        }),
    ))
}

/// Return the current creator identity for a session cookie.
pub async fn creator_me(creator: CreatorAuth) -> Result<Json<CreatorMeResponse>> {
    Ok(Json(CreatorMeResponse {
        agent_id: creator.agent_id,
        github_user_id: creator.github_user_id,
        github_login: creator.github_login,
    }))
}

/// Return the current creator identity and CSRF token for browser session bootstrap.
pub async fn creator_session(creator: CreatorAuth) -> Result<Json<CreatorSessionResponse>> {
    let csrf_token = creator.csrf_token.ok_or(AppError::Unauthorized)?;
    Ok(Json(CreatorSessionResponse {
        agent_id: creator.agent_id,
        github_user_id: creator.github_user_id,
        github_login: creator.github_login,
        csrf_token,
        expires_at: creator.expires_at.to_rfc3339(),
    }))
}

/// Exchanges a one-time GitHub OAuth code for an access token.
async fn exchange_github_code(state: &AppState, code: &str) -> Result<SecretString> {
    let client_id = required_oauth_config(
        state.config.github_oauth_client_id.as_deref(),
        "AGENTICS_GITHUB_OAUTH_CLIENT_ID",
    )?;
    let client_secret = required_oauth_config(
        state
            .config
            .github_oauth_client_secret
            .as_ref()
            .map(ExposeSecret::expose_secret),
        "AGENTICS_GITHUB_OAUTH_CLIENT_SECRET",
    )?;
    let redirect_url = required_oauth_config(
        state
            .config
            .github_oauth_redirect_url
            .as_ref()
            .map(|url| url.as_str()),
        "AGENTICS_GITHUB_OAUTH_REDIRECT_URL",
    )?;
    let token_body = form_urlencoded(&[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code.trim()),
        ("redirect_uri", redirect_url),
    ])?;
    let response = reqwest::Client::new()
        .post(state.config.github_oauth_token_url.as_str())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, GITHUB_USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(token_body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GitHub OAuth token request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "GitHub OAuth token request failed with status {}",
            response.status()
        )));
    }
    let body = response
        .json::<GithubAccessTokenResponse>()
        .await
        .map_err(|e| AppError::Internal(format!("invalid GitHub OAuth token response: {e}")))?;
    if let Some(error) = body.error {
        return Err(AppError::BadRequest(format!(
            "GitHub OAuth token exchange failed: {}",
            body.error_description.unwrap_or(error)
        )));
    }
    body.access_token.ok_or_else(|| {
        AppError::BadRequest(
            "GitHub OAuth token response did not include an access token".to_string(),
        )
    })
}

/// Fetches the GitHub account identity associated with an OAuth access token.
async fn fetch_github_user(
    state: &AppState,
    access_token: &SecretString,
) -> Result<GithubUserResponse> {
    let response = reqwest::Client::new()
        .get(state.config.github_api_user_url.as_str())
        .bearer_auth(access_token.expose_secret())
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, GITHUB_USER_AGENT)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GitHub user request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "GitHub user request failed with status {}",
            response.status()
        )));
    }
    response
        .json::<GithubUserResponse>()
        .await
        .map_err(|e| AppError::Internal(format!("invalid GitHub user response: {e}")))
}

/// Reads one required OAuth configuration value with a user-facing error name.
fn required_oauth_config<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("{name} is not configured")))?;
    Ok(value)
}

/// Encodes OAuth form fields using the URL crate instead of hand-built escaping.
fn form_urlencoded(values: &[(&str, &str)]) -> Result<String> {
    let mut url = Url::parse("https://agentics.local/")
        .map_err(|e| AppError::Internal(format!("invalid form helper URL: {e}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in values {
            pairs.append_pair(key, value);
        }
    }
    url.query()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Internal("failed to encode OAuth token request".to_string()))
}

/// Converts configured session lifetime hours into seconds with overflow checking.
fn session_ttl_seconds(state: &AppState) -> Result<i64> {
    state
        .config
        .web_session_ttl_hours
        .checked_mul(60 * 60)
        .ok_or_else(|| AppError::Internal("web session TTL overflow".to_string()))
}

/// Computes the absolute expiration time for a newly issued web session.
fn session_expires_at(ttl_seconds: i64) -> Result<chrono::DateTime<Utc>> {
    Utc::now()
        .checked_add_signed(Duration::seconds(ttl_seconds))
        .ok_or_else(|| AppError::Internal("web session TTL overflow".to_string()))
}

/// Builds the session and CSRF cookies for a successful browser login.
fn session_cookies(
    state: &AppState,
    session_token: &str,
    csrf_token: &str,
    ttl_seconds: i64,
) -> [(HeaderName, String); 2] {
    [
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.web_session_cookie_name,
                session_token,
                ttl_seconds,
                true,
                state.config.web_session_cookie_secure,
            ),
        ),
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.web_csrf_cookie_name,
                csrf_token,
                ttl_seconds,
                false,
                state.config.web_session_cookie_secure,
            ),
        ),
    ]
}

/// Builds expired cookies that clear browser session state during logout.
fn expired_session_cookies(state: &AppState) -> [(HeaderName, String); 2] {
    [
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.web_session_cookie_name,
                "",
                0,
                true,
                state.config.web_session_cookie_secure,
            ),
        ),
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.web_csrf_cookie_name,
                "",
                0,
                false,
                state.config.web_session_cookie_secure,
            ),
        ),
    ]
}

/// Formats one session cookie with the security attributes configured for the deployment.
fn build_cookie(
    name: &str,
    value: &str,
    max_age_seconds: i64,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!("{name}={value}; Path=/; Max-Age={max_age_seconds}; SameSite=Lax");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
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
