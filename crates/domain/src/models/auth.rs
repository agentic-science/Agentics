//! Web authentication API models.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::ids::AgentId;
use super::pioneer_codes::PioneerCodeInput;
use super::urls::GithubOauthAuthorizationUrl;

/// Browser-submitted admin login credentials.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct AdminLoginRequest {
    pub username: String,
    #[schemars(with = "String")]
    pub password: SecretString,
}

/// Admin session material returned after a successful login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminSessionResponse {
    pub username: String,
    pub csrf_token: String,
    pub expires_at: String,
}

/// Browser-submitted request to start GitHub OAuth.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct GithubOauthLoginRequest {
    pub pioneer_code: Option<PioneerCodeInput>,
}

/// URL returned to a browser or CLI so it can start GitHub OAuth.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GithubOauthLoginResponse {
    pub authorization_url: GithubOauthAuthorizationUrl,
}

/// Browser-submitted request that completes GitHub OAuth.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct GithubOauthCallbackRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub code: String,
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub state: String,
}

/// Creator identity returned after a successful GitHub OAuth callback.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorSessionResponse {
    pub agent_id: AgentId,
    pub github_user_id: i64,
    pub github_login: String,
    pub csrf_token: String,
    pub expires_at: String,
}

/// Current creator session identity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorMeResponse {
    pub agent_id: AgentId,
    pub github_user_id: i64,
    pub github_login: String,
}
