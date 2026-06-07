//! Authentication token creation, hashing, workflow, and header parsing helpers.

use std::time::Duration as StdDuration;

use agentics_config::{AgentRegistrationMode, Config};
use agentics_domain::models::auth::{
    CompleteHumanSetupRequest, CompleteHumanSetupResponse, GithubSignInCallbackRequest,
    GithubSignInCallbackResponse, GithubSignInLoginRequest, GithubSignInLoginResponse,
    GithubUserId, HumanSessionResponse, HumanStatus,
};
use agentics_domain::models::ids::{AgentId, AgentTokenId, HumanId, HumanSessionId};
use agentics_domain::models::pioneer_codes::{INVALID_OR_UNAVAILABLE_PIONEER_CODE, PioneerCode};
use agentics_domain::models::request::{RegisterAgentRequest, RegisterAgentResponse};
use agentics_domain::models::urls::GithubSignInAuthorizationUrl;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    ConsumedGithubSignInState, CreateGithubSignInStateInput, CreateHumanSessionInput, HumanRecord,
    PioneerCodeRegistrationKind, RegisterAgentInput, Repositories, ResolveGithubHumanInput,
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use rand::Rng;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use url::Url;

const GITHUB_SIGN_IN_STATE_TTL_MINUTES: i64 = 10;
const GITHUB_USER_AGENT: &str = "Agentics";
const FAILED_PIONEER_CODE_DELAY: StdDuration = StdDuration::from_millis(500);

/// Issued web session tokens and response payload for a browser session.
#[derive(Debug, Clone)]
pub struct IssuedWebSession<T> {
    pub session_token: String,
    pub csrf_token: String,
    pub ttl_seconds: i64,
    pub response: T,
}

/// GitHub sign-in result plus browser nonce the HTTP layer stores in a cookie.
#[derive(Debug, Clone)]
pub struct GithubSignInLoginIssue {
    pub browser_nonce: String,
    pub response: GithubSignInLoginResponse,
}

/// GitHub identity returned after exchanging an authorization code.
#[derive(Debug, Clone)]
pub struct GithubSignInUser {
    pub id: GithubUserId,
    pub login: String,
}

/// External GitHub sign-in operations used by the creator login workflow.
#[async_trait]
pub trait GithubSignInClient: Send + Sync {
    async fn exchange_code(&self, config: &Config, code: &str) -> Result<SecretString>;
    async fn fetch_user(
        &self,
        config: &Config,
        access_token: &SecretString,
    ) -> Result<GithubSignInUser>;
}

/// Reqwest-backed GitHub sign-in client for production API handlers.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReqwestGithubSignInClient;

/// Minimal JSON shape returned by GitHub's user authorization token endpoint.
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

#[async_trait]
impl GithubSignInClient for ReqwestGithubSignInClient {
    async fn exchange_code(&self, config: &Config, code: &str) -> Result<SecretString> {
        let client_id = required_github_app_config(
            config.github_app.client_id.as_deref(),
            "AGENTICS_GITHUB_APP_CLIENT_ID",
        )?;
        let client_secret = required_github_app_config(
            config
                .github_app
                .client_secret
                .as_ref()
                .map(ExposeSecret::expose_secret),
            "AGENTICS_GITHUB_APP_CLIENT_SECRET",
        )?;
        let redirect_url = required_github_app_config(
            config
                .github_app
                .redirect_url
                .as_ref()
                .map(|url| url.as_str()),
            "AGENTICS_GITHUB_APP_REDIRECT_URL",
        )?;
        let token_body = form_urlencoded(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code.trim()),
            ("redirect_uri", redirect_url),
        ])?;
        let response = reqwest::Client::new()
            .post(config.github_app.token_url.as_str())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, GITHUB_USER_AGENT)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(token_body)
            .send()
            .await
            .map_err(|e| {
                ServiceError::Internal(format!("GitHub sign-in token request failed: {e}"))
            })?;
        if !response.status().is_success() {
            return Err(ServiceError::BadRequest(format!(
                "GitHub sign-in token request failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<GithubAccessTokenResponse>()
            .await
            .map_err(|e| {
                ServiceError::Internal(format!("invalid GitHub sign-in token response: {e}"))
            })?;
        if let Some(error) = body.error {
            return Err(ServiceError::BadRequest(format!(
                "GitHub sign-in token exchange failed: {}",
                body.error_description.unwrap_or(error)
            )));
        }
        Ok(body.access_token.ok_or_else(|| {
            ServiceError::BadRequest(
                "GitHub sign-in token response did not include an access token".to_string(),
            )
        })?)
    }

    async fn fetch_user(
        &self,
        config: &Config,
        access_token: &SecretString,
    ) -> Result<GithubSignInUser> {
        let response = reqwest::Client::new()
            .get(config.github_app.api_user_url.as_str())
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
            )));
        }
        let user = response
            .json::<GithubUserResponse>()
            .await
            .map_err(|e| ServiceError::Internal(format!("invalid GitHub user response: {e}")))?;
        validate_github_user(user)
    }
}

/// Parsed bearer-token authorization header.
#[derive(Debug, Clone)]
pub struct ParsedBearerToken {
    pub token: SecretString,
}

/// Create an opaque bearer token for an agent.
pub fn create_agent_token() -> String {
    format!("agentics_{}", random_url_token(24))
}

/// Create an opaque bearer token for admin automation.
pub fn create_admin_service_token() -> String {
    format!("agentics_admin_{}", random_url_token(32))
}

/// Create an opaque bearer token for creator CLI automation.
pub fn create_creator_api_token() -> String {
    format!("agentics_creator_{}", random_url_token(32))
}

/// Create an opaque browser session token.
pub fn create_web_session_token() -> String {
    format!("agentics_session_{}", random_url_token(32))
}

/// Create an opaque CSRF token bound to a browser session.
pub fn create_csrf_token() -> String {
    format!("agentics_csrf_{}", random_url_token(32))
}

/// Handles random url token for this module.
fn random_url_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::rng().fill_bytes(&mut bytes);
    base64_urlencode(&bytes)
}

/// Create an opaque GitHub sign-in state token.
pub fn create_github_sign_in_state_token() -> String {
    format!("agentics_github_sign_in_{}", random_url_token(32))
}

/// Create an opaque browser nonce that binds a sign-in state to one browser.
pub fn create_github_sign_in_browser_nonce() -> String {
    format!("agentics_github_sign_in_nonce_{}", random_url_token(32))
}

/// Hash an opaque token before storing or comparing it.
pub fn hash_opaque_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Handles base64 urlencode for this module.
fn base64_urlencode(input: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(input)
}

/// Hash an agent token before storing or comparing it.
pub fn hash_agent_token(token: &str) -> String {
    hash_opaque_token(token)
}

/// Register an agent and return its one-time bearer token response.
pub async fn register_agent(
    pool: &PgPool,
    config: &Config,
    body: RegisterAgentRequest,
) -> Result<RegisterAgentResponse> {
    let max_active_agents = i64::from(config.quotas.max_active_agents);
    let token = create_agent_token();
    let input = RegisterAgentInput {
        agent_id: AgentId::generate(),
        token_id: AgentTokenId::generate(),
        token_hash: hash_agent_token(&token),
        display_name: body.display_name.trim().to_string(),
        agent_description: body.agent_description.trim().to_string(),
        model_info: body.model_info,
    };

    let repos = Repositories::new(pool);
    let agent = match config.agent_registration_mode() {
        AgentRegistrationMode::PioneerCode => {
            let Some(code) = body.pioneer_code.as_ref() else {
                return Err(reject_failed_pioneer_code().await);
            };
            let Ok(code) = PioneerCode::try_new(code.expose_secret().to_string()) else {
                return Err(reject_failed_pioneer_code().await);
            };
            let code_hash = hash_opaque_token(code.expose_secret());
            match repos
                .agents()
                .register_agent_with_pioneer_code(
                    &input,
                    &code_hash,
                    max_active_agents,
                    PioneerCodeRegistrationKind::AgentApi,
                )
                .await
            {
                Ok(agent) => agent,
                Err(error) if is_invalid_pioneer_code(&error) => {
                    return Err(reject_failed_pioneer_code().await);
                }
                Err(error) => return Err(error),
            }
        }
        AgentRegistrationMode::Public => {
            repos
                .agents()
                .register_agent(&input, max_active_agents)
                .await?
        }
    };

    Ok(RegisterAgentResponse {
        agent_id: agent.id,
        token,
        display_name: agent.display_name,
        created_at: agent.created_at.to_rfc3339(),
    })
}

/// End a browser session by opaque session token.
pub async fn delete_web_session_by_token(pool: &PgPool, session_token: &str) -> Result<()> {
    Repositories::new(pool)
        .sessions()
        .delete_human_session_by_token(session_token)
        .await
}

/// Soft-delete the current human account and revoke active credentials it owns.
pub async fn delete_human_account(pool: &PgPool, human_id: &HumanId) -> Result<()> {
    Repositories::new(pool)
        .sessions()
        .delete_human_account(human_id)
        .await?;
    Ok(())
}

/// Return the current human session when browser cookies are still valid.
pub async fn authenticate_human_session(
    pool: &PgPool,
    session_token: &str,
    csrf_token: &str,
) -> Result<HumanSessionResponse> {
    let session = Repositories::new(pool)
        .sessions()
        .authenticate_human(session_token)
        .await?
        .ok_or(ServiceError::Unauthorized)?;
    if hash_opaque_token(csrf_token) != session.csrf_token_hash {
        return Err(ServiceError::Unauthorized);
    }

    Ok(HumanSessionResponse {
        human_id: session.human_id,
        status: session.status,
        github_user_id: session.github_user_id,
        github_login: session.github_login,
        roles: session.roles,
        csrf_token: csrf_token.to_string(),
        expires_at: session.expires_at.to_rfc3339(),
    })
}

/// Start a GitHub sign-in login for a browser human account.
pub async fn start_github_sign_in_login(
    pool: &PgPool,
    config: &Config,
    request: GithubSignInLoginRequest,
) -> Result<GithubSignInLoginIssue> {
    let client_id = required_github_app_config(
        config.github_app.client_id.as_deref(),
        "AGENTICS_GITHUB_APP_CLIENT_ID",
    )?;
    let redirect_url = required_github_app_config(
        config
            .github_app
            .redirect_url
            .as_ref()
            .map(|url| url.as_str()),
        "AGENTICS_GITHUB_APP_REDIRECT_URL",
    )?;
    let state_token = create_github_sign_in_state_token();
    let state_hash = hash_opaque_token(&state_token);
    let browser_nonce = create_github_sign_in_browser_nonce();
    let browser_nonce_hash = hash_opaque_token(&browser_nonce);
    let return_to = normalize_return_to(request.return_to)?;
    let expires_at = Utc::now()
        .checked_add_signed(Duration::minutes(GITHUB_SIGN_IN_STATE_TTL_MINUTES))
        .ok_or_else(|| ServiceError::Internal("GitHub sign-in state TTL overflow".to_string()))?;
    let repos = Repositories::new(pool);
    repos.sessions().delete_expired_web_auth_rows().await?;
    repos
        .sessions()
        .create_github_sign_in_state(&CreateGithubSignInStateInput {
            state_hash,
            browser_nonce_hash,
            return_to,
            expires_at,
        })
        .await?;

    let mut authorization_url = config.github_app.authorize_url.to_url();
    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_url)
        .append_pair("state", &state_token);

    let authorization_url =
        GithubSignInAuthorizationUrl::try_from_url(authorization_url).map_err(|e| {
            ServiceError::Internal(format!("generated invalid GitHub sign-in URL: {e}"))
        })?;

    Ok(GithubSignInLoginIssue {
        browser_nonce,
        response: GithubSignInLoginResponse { authorization_url },
    })
}

/// Complete setup for a signed-in human using a valid human pioneer code.
pub async fn complete_human_setup(
    pool: &PgPool,
    human_id: &HumanId,
    csrf_token: &str,
    expires_at: chrono::DateTime<Utc>,
    request: CompleteHumanSetupRequest,
) -> Result<CompleteHumanSetupResponse> {
    let Ok(code) = PioneerCode::try_new(request.pioneer_code.expose_secret().to_string()) else {
        return Err(reject_failed_pioneer_code().await);
    };
    let human = match Repositories::new(pool)
        .sessions()
        .complete_human_setup(human_id, &hash_opaque_token(code.expose_secret()))
        .await
    {
        Ok(human) => human,
        Err(error) if is_invalid_pioneer_code(&error) => {
            return Err(reject_failed_pioneer_code().await);
        }
        Err(error) => return Err(error),
    };

    Ok(CompleteHumanSetupResponse {
        session: HumanSessionResponse {
            human_id: human.human_id,
            status: parse_human_status(&human.status)?,
            github_user_id: human.github_user_id,
            github_login: human.github_login,
            roles: human.roles,
            csrf_token: csrf_token.to_string(),
            expires_at: expires_at.to_rfc3339(),
        },
    })
}

/// Complete GitHub sign-in and issue a human web session.
pub async fn complete_github_sign_in_callback(
    pool: &PgPool,
    config: &Config,
    client: &dyn GithubSignInClient,
    request: GithubSignInCallbackRequest,
    browser_nonce: &str,
) -> Result<IssuedWebSession<GithubSignInCallbackResponse>> {
    let sign_in_state =
        consume_callback_github_sign_in_state(pool, &request.state, browser_nonce).await?;
    let access_token = client.exchange_code(config, &request.code).await?;
    let github_user = client.fetch_user(config, &access_token).await?;
    let human = resolve_callback_human(pool, config, &sign_in_state, &github_user).await?;
    let issued = issue_human_session(pool, config, human).await?;
    Ok(IssuedWebSession {
        session_token: issued.session_token,
        csrf_token: issued.csrf_token,
        ttl_seconds: issued.ttl_seconds,
        response: GithubSignInCallbackResponse {
            session: issued.response,
            return_to: sign_in_state.return_to,
        },
    })
}

/// Parse an `Authorization: Bearer ...` header.
pub fn parse_bearer_token(value: Option<&str>) -> Option<ParsedBearerToken> {
    let value = value?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;

    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    if token.is_empty() {
        return None;
    }

    Some(ParsedBearerToken {
        token: SecretString::from(token),
    })
}

/// Sleep before returning a generic failed pioneer-code response.
pub async fn reject_failed_pioneer_code() -> ServiceError {
    tokio::time::sleep(FAILED_PIONEER_CODE_DELAY).await;
    invalid_pioneer_code()
}

/// Return whether an application error is the generic pioneer-code rejection.
pub fn is_invalid_pioneer_code(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Forbidden(message) if message == INVALID_OR_UNAVAILABLE_PIONEER_CODE)
}

/// Return the generic pioneer-code rejection without timing mitigation.
fn invalid_pioneer_code() -> ServiceError {
    ServiceError::Forbidden(INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string())
}

/// Reads one required GitHub App configuration value with a user-facing error name.
fn required_github_app_config<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServiceError::BadRequest(format!("{name} is not configured")))?;
    Ok(value)
}

/// Encodes form fields using the URL crate instead of hand-built escaping.
fn form_urlencoded(values: &[(&str, &str)]) -> Result<String> {
    let mut url = Url::parse("https://agentics.local/")
        .map_err(|e| ServiceError::Internal(format!("invalid form helper URL: {e}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in values {
            pairs.append_pair(key, value);
        }
    }
    url.query().map(ToOwned::to_owned).ok_or_else(|| {
        ServiceError::Internal("failed to encode GitHub sign-in token request".to_string())
    })
}

/// Restrict post-login navigation to same-site paths.
fn normalize_return_to(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\n')
        || value.contains('\r')
        || value.contains("://")
    {
        return Err(ServiceError::BadRequest(
            "return_to must be a same-site path".to_string(),
        ));
    }
    Ok(Some(value.to_string()))
}

/// Converts configured session lifetime hours into seconds with overflow checking.
fn session_ttl_seconds(config: &Config) -> Result<i64> {
    config
        .api_web
        .web_session_ttl_hours
        .checked_mul(60 * 60)
        .ok_or_else(|| ServiceError::Internal("web session TTL overflow".to_string()))
}

/// Computes the absolute expiration time for a newly issued web session.
fn session_expires_at(ttl_seconds: i64) -> Result<chrono::DateTime<Utc>> {
    Utc::now()
        .checked_add_signed(Duration::seconds(ttl_seconds))
        .ok_or_else(|| ServiceError::Internal("web session TTL overflow".to_string()))
}

/// Consume a GitHub sign-in state after the callback nonce has been verified.
async fn consume_callback_github_sign_in_state(
    pool: &PgPool,
    state_token: &str,
    browser_nonce: &str,
) -> Result<ConsumedGithubSignInState> {
    let state_hash = hash_opaque_token(state_token);
    let browser_nonce_hash = hash_opaque_token(browser_nonce);
    Repositories::new(pool)
        .sessions()
        .consume_github_sign_in_state(&state_hash, &browser_nonce_hash)
        .await?
        .ok_or(ServiceError::Unauthorized)
}

/// Validate the GitHub account identity associated with the temporary access token.
fn validate_github_user(user: GithubUserResponse) -> Result<GithubSignInUser> {
    let login = user.login.trim();
    let Ok(id) = GithubUserId::try_new(user.id) else {
        return Err(ServiceError::BadRequest(
            "GitHub sign-in returned an invalid creator identity".to_string(),
        ));
    };
    if login.is_empty() {
        return Err(ServiceError::BadRequest(
            "GitHub sign-in returned an invalid creator identity".to_string(),
        ));
    }
    Ok(GithubSignInUser {
        id,
        login: login.to_string(),
    })
}

/// Resolve or create the human tied to a GitHub account.
async fn resolve_callback_human(
    pool: &PgPool,
    config: &Config,
    _sign_in_state: &ConsumedGithubSignInState,
    github_user: &GithubSignInUser,
) -> Result<HumanRecord> {
    let fallback_human_id = HumanId::generate();
    let bootstrap_admin_candidate = config
        .auth
        .bootstrap_admin_github_user_ids
        .contains(&github_user.id);
    match Repositories::new(pool)
        .sessions()
        .resolve_github_human(&ResolveGithubHumanInput {
            fallback_human_id,
            github_user_id: github_user.id,
            github_login: github_user.login.clone(),
            bootstrap_admin_candidate,
        })
        .await
    {
        Ok(human) => Ok(human),
        Err(error) if is_invalid_pioneer_code(&error) => Err(reject_failed_pioneer_code().await),
        Err(error) => Err(error),
    }
}

/// Persist a human session for the authenticated GitHub user.
async fn issue_human_session(
    pool: &PgPool,
    config: &Config,
    human: HumanRecord,
) -> Result<IssuedWebSession<HumanSessionResponse>> {
    let session_token = create_web_session_token();
    let csrf_token = create_csrf_token();
    let ttl_seconds = session_ttl_seconds(config)?;
    let expires_at = session_expires_at(ttl_seconds)?;
    Repositories::new(pool)
        .sessions()
        .create_human_session(&CreateHumanSessionInput {
            session_id: HumanSessionId::generate(),
            session_token_hash: hash_opaque_token(&session_token),
            csrf_token_hash: hash_opaque_token(&csrf_token),
            human_id: human.human_id.clone(),
            expires_at,
        })
        .await?;

    Ok(IssuedWebSession {
        session_token,
        csrf_token: csrf_token.clone(),
        ttl_seconds,
        response: HumanSessionResponse {
            human_id: human.human_id,
            status: parse_human_status(&human.status)?,
            github_user_id: human.github_user_id,
            github_login: human.github_login,
            roles: human.roles,
            csrf_token,
            expires_at: expires_at.to_rfc3339(),
        },
    })
}

fn parse_human_status(status: &str) -> Result<HumanStatus> {
    HumanStatus::from_storage_value(status)
        .ok_or_else(|| ServiceError::Internal(format!("stored invalid human status `{status}`")))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration as StdDuration, Instant};

    use super::{create_agent_token, hash_agent_token, reject_failed_pioneer_code};

    /// Verifies that creates agentics prefixed tokens.
    #[test]
    fn creates_agentics_prefixed_tokens() {
        let token = create_agent_token();
        assert!(token.starts_with("agentics_"));
        assert_ne!(hash_agent_token(&token), token);
    }

    /// Verifies that failed pioneer-code paths pay the intended minimum delay.
    #[tokio::test]
    async fn failed_pioneer_code_rejection_waits_before_returning() {
        let started = Instant::now();
        let _error = reject_failed_pioneer_code().await;

        assert!(started.elapsed() >= StdDuration::from_millis(450));
    }
}
