//! Web authentication and human identity API models.

use serde::{Deserialize, Serialize};

use super::ids::{AdminServiceTokenId, HumanId};
use super::pioneer_codes::PioneerCodeInput;
use super::urls::GithubSignInAuthorizationUrl;

/// Role granted to a human account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HumanRole {
    Creator,
    Admin,
}

impl HumanRole {
    /// Stable database and wire string for a human role.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Creator => "creator",
            Self::Admin => "admin",
        }
    }

    /// Parse a stable database string for a human role.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "creator" => Some(Self::Creator),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// Persistent lifecycle state for a human account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HumanStatus {
    Active,
    Disabled,
}

impl HumanStatus {
    /// Stable database and wire string for a human status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// Parse a stable database string for a human status.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Browser-submitted request to start GitHub sign-in.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct GithubSignInLoginRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pioneer_code: Option<PioneerCodeInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
}

/// URL returned to a browser or CLI so it can start GitHub sign-in.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GithubSignInLoginResponse {
    pub authorization_url: GithubSignInAuthorizationUrl,
}

/// Browser-submitted request that completes GitHub sign-in.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct GithubSignInCallbackRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub code: String,
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub state: String,
}

/// Current human browser session identity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HumanSessionResponse {
    pub human_id: HumanId,
    pub github_user_id: i64,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub csrf_token: String,
    pub expires_at: String,
}

/// Response returned after completing GitHub sign-in.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GithubSignInCallbackResponse {
    pub session: HumanSessionResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
}

/// Admin-visible human identity row.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminHumanDto {
    pub human_id: HumanId,
    pub status: HumanStatus,
    pub github_user_id: i64,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<String>,
}

/// Admin list response for human identities.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminHumanListResponse {
    pub items: Vec<AdminHumanDto>,
}

/// Response returned after granting or revoking a human admin role.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminHumanRoleResponse {
    pub human: AdminHumanDto,
}

/// Browser-submitted request to create an admin service token.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreateAdminServiceTokenRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Admin service-token metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceTokenDto {
    pub id: AdminServiceTokenId,
    pub label: String,
    pub status: String,
    pub created_by_human_id: HumanId,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_by_human_id: Option<HumanId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// Response returned after creating an admin service token.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceTokenCreatedResponse {
    pub token: String,
    pub token_record: AdminServiceTokenDto,
}

/// Admin list response for admin service tokens.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceTokenListResponse {
    pub items: Vec<AdminServiceTokenDto>,
}

/// Response returned after revoking an admin service token.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevokeAdminServiceTokenResponse {
    pub token_record: AdminServiceTokenDto,
}
