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

use crate::error::ApiResult as Result;
use agentics_config::AgentRegistrationMode;
use agentics_domain::error::ServiceError;
use agentics_domain::models::auth::{
    AdminLoginRequest, AdminSessionResponse, CreatorMeResponse, CreatorSessionResponse,
    GithubOauthCallbackRequest, GithubOauthLoginRequest, GithubOauthLoginResponse,
};
use agentics_domain::models::ids::AgentId;
use agentics_domain::models::pioneer_codes::PioneerCode;
use agentics_domain::models::urls::GithubOauthAuthorizationUrl;
use agentics_persistence as db;
use agentics_services::auth;

use crate::extractors::{AdminAuth, CreatorAuth, ValidatedJson};
use crate::pioneer_code_security::{is_invalid_pioneer_code, reject_failed_pioneer_code};
use crate::state::AppState;

const OAUTH_STATE_TTL_MINUTES: i64 = 10;
const OAUTH_NONCE_COOKIE_NAME: &str = "agentics_oauth_nonce";
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
    ValidatedJson(request): ValidatedJson<AdminLoginRequest>,
) -> Result<(
    StatusCode,
    AppendHeaders<[(HeaderName, String); 2]>,
    Json<AdminSessionResponse>,
)> {
    if request.username.trim().is_empty() || request.password.expose_secret().is_empty() {
        state
            .admin_auth_throttle
            .record_failed_attempt(&request.username, &remote_addr.ip().to_string())?;
        return Err(ServiceError::Unauthorized.into());
    }
    if request.username != state.config.admin_username
        || !state
            .config
            .admin_password_matches(request.password.expose_secret())
    {
        state
            .admin_auth_throttle
            .record_failed_attempt(&request.username, &remote_addr.ip().to_string())?;
        return Err(ServiceError::Unauthorized.into());
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
        .ok_or(ServiceError::Unauthorized)?;
    let csrf_token = cookie_value(cookie_header, &state.config.web_csrf_cookie_name)
        .ok_or(ServiceError::Unauthorized)?;
    let session = db::authenticate_admin_session(&state.db, &session_token)
        .await?
        .ok_or(ServiceError::Unauthorized)?;
    if auth::hash_opaque_token(&csrf_token) != session.csrf_token_hash {
        return Err(ServiceError::Unauthorized.into());
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
) -> Result<(
    StatusCode,
    AppendHeaders<[(HeaderName, String); 1]>,
    Json<GithubOauthLoginResponse>,
)> {
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
    let browser_nonce = auth::create_oauth_browser_nonce();
    let browser_nonce_hash = auth::hash_opaque_token(&browser_nonce);
    let pioneer_code_hash = match state.config.agent_registration_mode() {
        AgentRegistrationMode::PioneerCode => match request.pioneer_code.as_ref() {
            Some(code) => {
                let Ok(code) = PioneerCode::try_new(code.expose_secret().to_string()) else {
                    return Err(reject_failed_pioneer_code().await.into());
                };
                Some(auth::hash_opaque_token(code.expose_secret()))
            }
            None => None,
        },
        AgentRegistrationMode::Public => None,
    };
    let expires_at = Utc::now()
        .checked_add_signed(Duration::minutes(OAUTH_STATE_TTL_MINUTES))
        .ok_or_else(|| ServiceError::Internal("OAuth state TTL overflow".to_string()))?;
    db::delete_expired_web_auth_rows(&state.db).await?;
    db::create_github_oauth_state(
        &state.db,
        &db::CreateGithubOauthStateInput {
            state_hash,
            browser_nonce_hash,
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
        .map_err(|e| ServiceError::Internal(format!("generated invalid GitHub OAuth URL: {e}")))?;

    Ok((
        StatusCode::OK,
        AppendHeaders([oauth_nonce_cookie(&state, &browser_nonce)]),
        Json(GithubOauthLoginResponse { authorization_url }),
    ))
}

/// Complete GitHub OAuth and issue a creator web session.
pub async fn github_oauth_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(request): ValidatedJson<GithubOauthCallbackRequest>,
) -> Result<(
    StatusCode,
    AppendHeaders<[(HeaderName, String); 3]>,
    Json<CreatorSessionResponse>,
)> {
    let cookie_header = headers.get(header::COOKIE).and_then(|h| h.to_str().ok());
    let browser_nonce =
        cookie_value(cookie_header, OAUTH_NONCE_COOKIE_NAME).ok_or(ServiceError::Unauthorized)?;
    let oauth_state = consume_callback_oauth_state(&state, &request.state, &browser_nonce).await?;
    let access_token = exchange_github_code(&state, &request.code).await?;
    let github_user = validate_github_user(fetch_github_user(&state, &access_token).await?)?;
    let agent_id = upsert_callback_creator_agent(&state, &oauth_state, &github_user).await?;
    let issued_session = issue_creator_session(&state, agent_id, &github_user).await?;
    let [session_cookie, csrf_cookie] = issued_session.headers;

    Ok((
        StatusCode::OK,
        AppendHeaders([
            session_cookie,
            csrf_cookie,
            expired_oauth_nonce_cookie(&state),
        ]),
        Json(issued_session.response),
    ))
}

async fn consume_callback_oauth_state(
    state: &AppState,
    state_token: &str,
    browser_nonce: &str,
) -> Result<db::ConsumedGithubOauthState> {
    let state_hash = auth::hash_opaque_token(state_token);
    let browser_nonce_hash = auth::hash_opaque_token(browser_nonce);
    db::consume_github_oauth_state(&state.db, &state_hash, &browser_nonce_hash)
        .await?
        .ok_or_else(|| ServiceError::Unauthorized.into())
}

#[derive(Debug, Clone)]
struct VerifiedGithubUser {
    id: i64,
    login: String,
}

fn validate_github_user(user: GithubUserResponse) -> Result<VerifiedGithubUser> {
    let login = user.login.trim();
    if user.id <= 0 || login.is_empty() {
        return Err(ServiceError::BadRequest(
            "GitHub OAuth returned an invalid creator identity".to_string(),
        )
        .into());
    }
    Ok(VerifiedGithubUser {
        id: user.id,
        login: login.to_string(),
    })
}

async fn upsert_callback_creator_agent(
    state: &AppState,
    oauth_state: &db::ConsumedGithubOauthState,
    github_user: &VerifiedGithubUser,
) -> Result<AgentId> {
    let fallback_agent_id = AgentId::generate();
    let require_pioneer_code =
        state.config.agent_registration_mode() == AgentRegistrationMode::PioneerCode;
    match db::upsert_github_creator_agent_with_pioneer_code(
        &state.db,
        &fallback_agent_id,
        github_user.id,
        &github_user.login,
        oauth_state.pioneer_code_hash.as_deref(),
        require_pioneer_code,
        i64::from(state.config.max_active_agents),
    )
    .await
    {
        Ok(agent_id) => Ok(agent_id),
        Err(error) if is_invalid_pioneer_code(&error) => {
            Err(reject_failed_pioneer_code().await.into())
        }
        Err(error) => Err(error.into()),
    }
}

struct IssuedCreatorSession {
    headers: [(HeaderName, String); 2],
    response: CreatorSessionResponse,
}

async fn issue_creator_session(
    state: &AppState,
    agent_id: AgentId,
    github_user: &VerifiedGithubUser,
) -> Result<IssuedCreatorSession> {
    let session_token = auth::create_web_session_token();
    let csrf_token = auth::create_csrf_token();
    let ttl_seconds = session_ttl_seconds(state)?;
    let expires_at = session_expires_at(ttl_seconds)?;
    db::create_creator_session(
        &state.db,
        &db::CreateCreatorSessionInput {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_token_hash: auth::hash_opaque_token(&session_token),
            csrf_token_hash: auth::hash_opaque_token(&csrf_token),
            agent_id: agent_id.as_str().to_string(),
            github_user_id: github_user.id,
            github_login: github_user.login.clone(),
            expires_at,
        },
    )
    .await?;

    Ok(IssuedCreatorSession {
        headers: session_cookies(state, &session_token, &csrf_token, ttl_seconds),
        response: CreatorSessionResponse {
            agent_id,
            github_user_id: github_user.id,
            github_login: github_user.login.clone(),
            csrf_token,
            expires_at: expires_at.to_rfc3339(),
        },
    })
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
    let csrf_token = creator.csrf_token.ok_or(ServiceError::Unauthorized)?;
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
        .map_err(|e| ServiceError::Internal(format!("GitHub OAuth token request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(ServiceError::BadRequest(format!(
            "GitHub OAuth token request failed with status {}",
            response.status()
        ))
        .into());
    }
    let body = response
        .json::<GithubAccessTokenResponse>()
        .await
        .map_err(|e| ServiceError::Internal(format!("invalid GitHub OAuth token response: {e}")))?;
    if let Some(error) = body.error {
        return Err(ServiceError::BadRequest(format!(
            "GitHub OAuth token exchange failed: {}",
            body.error_description.unwrap_or(error)
        ))
        .into());
    }
    Ok(body.access_token.ok_or_else(|| {
        ServiceError::BadRequest(
            "GitHub OAuth token response did not include an access token".to_string(),
        )
    })?)
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
        .map_err(|e| ServiceError::Internal(format!("GitHub user request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(ServiceError::BadRequest(format!(
            "GitHub user request failed with status {}",
            response.status()
        ))
        .into());
    }
    Ok(response
        .json::<GithubUserResponse>()
        .await
        .map_err(|e| ServiceError::Internal(format!("invalid GitHub user response: {e}")))?)
}

/// Reads one required OAuth configuration value with a user-facing error name.
fn required_oauth_config<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServiceError::BadRequest(format!("{name} is not configured")))?;
    Ok(value)
}

/// Encodes OAuth form fields using the URL crate instead of hand-built escaping.
fn form_urlencoded(values: &[(&str, &str)]) -> Result<String> {
    let mut url = Url::parse("https://agentics.local/")
        .map_err(|e| ServiceError::Internal(format!("invalid form helper URL: {e}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in values {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.query().map(ToOwned::to_owned).ok_or_else(|| {
        ServiceError::Internal("failed to encode OAuth token request".to_string())
    })?)
}

/// Converts configured session lifetime hours into seconds with overflow checking.
fn session_ttl_seconds(state: &AppState) -> Result<i64> {
    Ok(state
        .config
        .web_session_ttl_hours
        .checked_mul(60 * 60)
        .ok_or_else(|| ServiceError::Internal("web session TTL overflow".to_string()))?)
}

/// Computes the absolute expiration time for a newly issued web session.
fn session_expires_at(ttl_seconds: i64) -> Result<chrono::DateTime<Utc>> {
    Ok(Utc::now()
        .checked_add_signed(Duration::seconds(ttl_seconds))
        .ok_or_else(|| ServiceError::Internal("web session TTL overflow".to_string()))?)
}

/// Builds a browser-binding OAuth nonce cookie.
fn oauth_nonce_cookie(state: &AppState, browser_nonce: &str) -> (HeaderName, String) {
    (
        header::SET_COOKIE,
        build_cookie(
            OAUTH_NONCE_COOKIE_NAME,
            browser_nonce,
            OAUTH_STATE_TTL_MINUTES * 60,
            true,
            state.config.web_session_cookie_secure,
        ),
    )
}

/// Builds an expired OAuth nonce cookie after a successful callback.
fn expired_oauth_nonce_cookie(state: &AppState) -> (HeaderName, String) {
    (
        header::SET_COOKIE,
        build_cookie(
            OAUTH_NONCE_COOKIE_NAME,
            "",
            0,
            true,
            state.config.web_session_cookie_secure,
        ),
    )
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
