//! Web authentication handlers for browser-based roles.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::AppendHeaders,
};

use crate::error::ApiResult as Result;
use agentics_domain::models::auth::{
    GithubOauthCallbackRequest, GithubOauthCallbackResponse, GithubOauthLoginRequest,
    GithubOauthLoginResponse, HumanSessionResponse,
};
use agentics_error::ServiceError;
use agentics_services::auth;

use crate::extractors::{HumanAuth, ValidatedJson};
use crate::state::AppState;

const OAUTH_STATE_TTL_MINUTES: i64 = 10;
const OAUTH_NONCE_COOKIE_NAME: &str = "agentics_oauth_nonce";

/// End a human browser session and clear auth cookies.
pub async fn human_logout(
    _human: HumanAuth,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, AppendHeaders<[(HeaderName, String); 2]>)> {
    if let Some(session_token) = cookie_value(
        headers.get(header::COOKIE).and_then(|h| h.to_str().ok()),
        &state.config.api_web.web_session_cookie_name,
    ) {
        auth::delete_web_session_by_token(&state.db, &session_token).await?;
    }

    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders(expired_session_cookies(&state)),
    ))
}

/// Return the current human session when browser cookies are still valid.
pub async fn human_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<HumanSessionResponse>> {
    let cookie_header = headers.get(header::COOKIE).and_then(|h| h.to_str().ok());
    let session_token = cookie_value(cookie_header, &state.config.api_web.web_session_cookie_name)
        .ok_or(ServiceError::Unauthorized)?;
    let csrf_token = cookie_value(cookie_header, &state.config.api_web.web_csrf_cookie_name)
        .ok_or(ServiceError::Unauthorized)?;
    Ok(Json(
        auth::authenticate_human_session(&state.db, &session_token, &csrf_token).await?,
    ))
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
    let issue = auth::start_github_oauth_login(&state.db, &state.config, request).await?;

    Ok((
        StatusCode::OK,
        AppendHeaders([oauth_nonce_cookie(&state, &issue.browser_nonce)]),
        Json(issue.response),
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
    Json<GithubOauthCallbackResponse>,
)> {
    let cookie_header = headers.get(header::COOKIE).and_then(|h| h.to_str().ok());
    let browser_nonce =
        cookie_value(cookie_header, OAUTH_NONCE_COOKIE_NAME).ok_or(ServiceError::Unauthorized)?;
    let github_client = auth::ReqwestGithubOauthClient;
    let issued_session = auth::complete_github_oauth_callback(
        &state.db,
        &state.config,
        &github_client,
        request,
        &browser_nonce,
    )
    .await?;
    let [session_cookie, csrf_cookie] = session_cookies(
        &state,
        &issued_session.session_token,
        &issued_session.csrf_token,
        issued_session.ttl_seconds,
    );

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

/// Builds a browser-binding OAuth nonce cookie.
fn oauth_nonce_cookie(state: &AppState, browser_nonce: &str) -> (HeaderName, String) {
    (
        header::SET_COOKIE,
        build_cookie(
            OAUTH_NONCE_COOKIE_NAME,
            browser_nonce,
            OAUTH_STATE_TTL_MINUTES * 60,
            true,
            state.config.api_web.web_session_cookie_secure,
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
            state.config.api_web.web_session_cookie_secure,
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
                &state.config.api_web.web_session_cookie_name,
                session_token,
                ttl_seconds,
                true,
                state.config.api_web.web_session_cookie_secure,
            ),
        ),
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.api_web.web_csrf_cookie_name,
                csrf_token,
                ttl_seconds,
                false,
                state.config.api_web.web_session_cookie_secure,
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
                &state.config.api_web.web_session_cookie_name,
                "",
                0,
                true,
                state.config.api_web.web_session_cookie_secure,
            ),
        ),
        (
            header::SET_COOKIE,
            build_cookie(
                &state.config.api_web.web_csrf_cookie_name,
                "",
                0,
                false,
                state.config.api_web.web_session_cookie_secure,
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
