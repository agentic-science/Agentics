//! Web authentication API models.

use serde::{Deserialize, Serialize};

/// Browser-submitted admin login credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

/// Admin session material returned after a successful login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSessionResponse {
    pub username: String,
    pub csrf_token: String,
    pub expires_at: String,
}

/// URL returned to a browser or CLI so it can start GitHub OAuth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubOauthLoginResponse {
    pub authorization_url: String,
    pub state: String,
}

/// Query parameters GitHub sends back to the OAuth callback.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubOauthCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Creator identity returned after a successful GitHub OAuth callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatorSessionResponse {
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
    pub csrf_token: String,
    pub expires_at: String,
}

/// Current creator session identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatorMeResponse {
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
}
