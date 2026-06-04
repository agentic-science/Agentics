//! Web authentication and human identity API models.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ids::{AdminServiceTokenId, CreatorApiTokenId, HumanId};
use super::pioneer_codes::PioneerCodeInput;
use super::urls::GithubSignInAuthorizationUrl;

/// Validation failure for [`GithubUserId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GithubUserIdError;

impl fmt::Display for GithubUserIdError {
    /// Format the user-facing GitHub user id validation error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("github_user_id must be a positive integer")
    }
}

impl std::error::Error for GithubUserIdError {}

/// Positive numeric GitHub user id returned by GitHub sign-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GithubUserId(i64);

impl GithubUserId {
    /// Parse a positive GitHub user id from an external boundary.
    pub fn try_new(value: i64) -> Result<Self, GithubUserIdError> {
        if value <= 0 {
            return Err(GithubUserIdError);
        }
        Ok(Self(value))
    }

    /// Borrow the numeric value for database and wire boundaries.
    pub fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for GithubUserId {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for GithubUserId {
    /// Serialize as the existing JSON integer contract.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for GithubUserId {
    /// Deserialize from the existing JSON integer contract.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GithubUserId {
    /// Keep the generated schema inline so DTO wire shape stays a number.
    fn inline_schema() -> bool {
        true
    }

    /// Handles schema name for this module.
    fn schema_name() -> Cow<'static, str> {
        "GithubUserId".into()
    }

    /// Handles json schema for this module.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "integer",
            "minimum": 1
        })
    }
}

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
    SetupRequired,
    Disabled,
}

impl HumanStatus {
    /// Stable database and wire string for a human status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::SetupRequired => "setup_required",
            Self::Disabled => "disabled",
        }
    }

    /// Parse a stable database string for a human status.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "setup_required" => Some(Self::SetupRequired),
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
    pub return_to: Option<String>,
}

/// Browser-submitted request to finish setup for a signed-in human.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CompleteHumanSetupRequest {
    pub pioneer_code: PioneerCodeInput,
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
    pub status: HumanStatus,
    pub github_user_id: GithubUserId,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub csrf_token: String,
    pub expires_at: String,
}

/// Response returned after finishing human account setup.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompleteHumanSetupResponse {
    pub session: HumanSessionResponse,
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
    pub github_user_id: GithubUserId,
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
#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceTokenCreatedResponse {
    pub token: String,
    pub token_record: AdminServiceTokenDto,
}

impl fmt::Debug for AdminServiceTokenCreatedResponse {
    /// Redacts the one-time raw admin service token from debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdminServiceTokenCreatedResponse")
            .field("token", &"<redacted>")
            .field("token_record", &self.token_record)
            .finish()
    }
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

/// Browser-submitted request to create a creator API token.
#[derive(Debug, Clone, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreateCreatorApiTokenRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Creator API-token metadata visible to the owning creator.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorApiTokenDto {
    pub id: CreatorApiTokenId,
    pub label: String,
    pub status: String,
    pub created_by_human_id: HumanId,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// Response returned after creating a creator API token.
#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorApiTokenCreatedResponse {
    pub token: String,
    pub token_record: CreatorApiTokenDto,
}

impl fmt::Debug for CreatorApiTokenCreatedResponse {
    /// Redacts the one-time raw creator API token from debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatorApiTokenCreatedResponse")
            .field("token", &"<redacted>")
            .field("token_record", &self.token_record)
            .finish()
    }
}

/// Creator list response for API tokens.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorApiTokenListResponse {
    pub items: Vec<CreatorApiTokenDto>,
}

/// Response returned after revoking a creator API token.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevokeCreatorApiTokenResponse {
    pub token_record: CreatorApiTokenDto,
}

#[cfg(test)]
mod tests {
    use super::{
        AdminServiceTokenCreatedResponse, AdminServiceTokenDto, CreatorApiTokenCreatedResponse,
        CreatorApiTokenDto, GithubUserId,
    };
    use crate::models::ids::{AdminServiceTokenId, CreatorApiTokenId, HumanId};

    /// Verifies GitHub user ids keep the integer wire contract while rejecting invalid ids.
    #[test]
    fn github_user_ids_are_positive_wire_integers() {
        let id = GithubUserId::try_new(42).expect("positive id should parse");
        assert_eq!(id.as_i64(), 42);
        assert_eq!(
            serde_json::to_value(id).expect("id should serialize"),
            serde_json::json!(42)
        );
        assert!(GithubUserId::try_new(0).is_err());
        assert!(serde_json::from_value::<GithubUserId>(serde_json::json!(-1)).is_err());
    }

    /// Verifies one-time bearer tokens cannot leak through accidental debug output.
    #[test]
    fn token_creation_debug_output_redacts_raw_tokens() {
        let human_id = HumanId::generate();
        let admin = AdminServiceTokenCreatedResponse {
            token: "agentics_admin_secret".to_string(),
            token_record: AdminServiceTokenDto {
                id: AdminServiceTokenId::generate(),
                label: "admin".to_string(),
                status: "active".to_string(),
                created_by_human_id: human_id.clone(),
                created_at: "2026-06-01T00:00:00Z".to_string(),
                last_used_at: None,
                expires_at: None,
                revoked_by_human_id: None,
                revoked_at: None,
            },
        };
        let creator = CreatorApiTokenCreatedResponse {
            token: "agentics_creator_secret".to_string(),
            token_record: CreatorApiTokenDto {
                id: CreatorApiTokenId::generate(),
                label: "creator".to_string(),
                status: "active".to_string(),
                created_by_human_id: human_id,
                created_at: "2026-06-01T00:00:00Z".to_string(),
                last_used_at: None,
                expires_at: None,
                revoked_at: None,
            },
        };

        let admin_debug = format!("{admin:?}");
        let creator_debug = format!("{creator:?}");

        assert!(!admin_debug.contains("agentics_admin_secret"));
        assert!(!creator_debug.contains("agentics_creator_secret"));
        assert!(admin_debug.contains("<redacted>"));
        assert!(creator_debug.contains("<redacted>"));
    }
}
