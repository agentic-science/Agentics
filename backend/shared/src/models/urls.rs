//! Validated URL and remote-locator types used by Agentics public contracts.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// User-facing validation message for GitHub repository remotes.
pub const GITHUB_REPO_REMOTE_ERROR_MESSAGE: &str = "repo_url must be a GitHub HTTPS repository URL or git@github.com:{owner}/{repo}.git SSH remote";

/// User-facing validation message for GitHub pull request URLs.
pub const GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE: &str = "pr_url must be a GitHub HTTPS pull request URL like https://github.com/{owner}/{repo}/pull/{number}";

/// User-facing validation message for external data URLs.
pub const EXTERNAL_DATA_URL_ERROR_MESSAGE: &str = "external data url must be an HTTPS URL";

/// Validation message for GitHub OAuth redirect URLs.
pub const GITHUB_OAUTH_REDIRECT_URL_ERROR_MESSAGE: &str =
    "github OAuth redirect URL must be an absolute HTTP(S) URL without query or fragment";

/// Validation message for GitHub OAuth authorization endpoint URLs.
pub const GITHUB_OAUTH_AUTHORIZE_URL_ERROR_MESSAGE: &str =
    "github OAuth authorize URL must be https://github.com/login/oauth/authorize";
pub const GITHUB_OAUTH_AUTHORIZATION_URL_ERROR_MESSAGE: &str =
    "github OAuth authorization URL must be an HTTPS GitHub authorize URL without fragment";

/// Validation message for GitHub OAuth token endpoint URLs.
pub const GITHUB_OAUTH_TOKEN_URL_ERROR_MESSAGE: &str =
    "github OAuth token URL must be https://github.com/login/oauth/access_token";

/// Validation message for the GitHub user API URL.
pub const GITHUB_API_USER_URL_ERROR_MESSAGE: &str =
    "github API user URL must be https://api.github.com/user";

/// Validation failure for URL-like Agentics contract fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UrlFieldError {
    message: &'static str,
}

impl UrlFieldError {
    const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for UrlFieldError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for UrlFieldError {}

macro_rules! impl_string_url_serde {
    ($type_name:ident) => {
        impl Serialize for $type_name {
            /// Handles serialize for this module.
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type_name {
            /// Handles deserialize for this module.
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::from_str(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

macro_rules! impl_string_url_schema {
    ($type_name:ident, $schema_name:literal, $pattern:literal) => {
        impl JsonSchema for $type_name {
            /// Handles inline schema for this module.
            fn inline_schema() -> bool {
                true
            }

            /// Handles schema name for this module.
            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            /// Handles json schema for this module.
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({
                    "type": "string",
                    "format": "uri",
                    "pattern": $pattern
                })
            }
        }
    };
}

/// GitHub repository remote syntax accepted for challenge draft provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GithubRepoRemote {
    /// HTTPS repository URL such as `https://github.com/agentics-reifying/agentics-challenges`.
    Https {
        /// Original validated HTTPS URL.
        url: Url,
        /// Canonical owner/repository key for uniqueness and authorization.
        key: GithubRepoKey,
    },
    /// SSH repository remote such as `git@github.com:agentics-reifying/agentics-challenges.git`.
    Ssh(GithubSshRepoRemote),
}

impl GithubRepoRemote {
    /// Parse and validate a GitHub repository remote.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, UrlFieldError> {
        value.as_ref().parse()
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Https { url, .. } => url.as_str(),
            Self::Ssh(remote) => remote.as_str(),
        }
    }

    /// Return the canonical GitHub owner/repository key.
    ///
    /// This key is not a URL and does not preserve the submitted remote syntax.
    /// It collapses accepted GitHub HTTPS and SSH remotes for the same
    /// repository into one `owner/repo` identity for duplicate detection and
    /// authorization checks.
    pub fn repository_key(&self) -> &GithubRepoKey {
        match self {
            Self::Https { key, .. } => key,
            Self::Ssh(remote) => remote.repository_key(),
        }
    }
}

impl fmt::Display for GithubRepoRemote {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GithubRepoRemote {
    type Err = UrlFieldError;

    /// Handles from str for this module.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.starts_with("git@github.com:") {
            return Ok(Self::Ssh(GithubSshRepoRemote::try_new(value)?));
        }

        let url = parse_url(value, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        let key = github_https_repo_key(&url)?;
        Ok(Self::Https { url, key })
    }
}

impl Serialize for GithubRepoRemote {
    /// Handles serialize for this module.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GithubRepoRemote {
    /// Handles deserialize for this module.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GithubRepoRemote {
    /// Handles inline schema for this module.
    fn inline_schema() -> bool {
        true
    }

    /// Handles schema name for this module.
    fn schema_name() -> Cow<'static, str> {
        "GithubRepoRemote".into()
    }

    /// Handles json schema for this module.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^(https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:\.git)?|git@github\.com:[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\.git)$"
        })
    }
}

/// Validated GitHub SSH repository remote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GithubSshRepoRemote {
    value: String,
    key: GithubRepoKey,
}

impl GithubSshRepoRemote {
    /// Handles try new for this module.
    fn try_new(value: &str) -> Result<Self, UrlFieldError> {
        reject_whitespace_or_control(value, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        let Some(rest) = value.strip_prefix("git@github.com:") else {
            return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
        };
        let Some((owner, repo_with_suffix)) = rest.split_once('/') else {
            return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
        };
        if repo_with_suffix.contains('/') {
            return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
        }
        let Some(repo) = repo_with_suffix.strip_suffix(".git") else {
            return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
        };
        let key = GithubRepoKey::try_new(owner, repo)?;
        Ok(Self {
            value: format!("git@github.com:{owner}/{repo}.git"),
            key,
        })
    }

    /// Returns str in the representation required by callers.
    fn as_str(&self) -> &str {
        &self.value
    }

    /// Handles repository key for this module.
    fn repository_key(&self) -> &GithubRepoKey {
        &self.key
    }
}

/// Canonical GitHub repository identity used for duplicate detection.
///
/// The string is always lowercase `owner/repo`. Keep the original
/// `GithubRepoRemote` when provenance or display should preserve whether the
/// contributor submitted an HTTPS URL or SSH remote.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GithubRepoKey(String);

impl GithubRepoKey {
    /// Handles try new for this module.
    fn try_new(owner: &str, repo: &str) -> Result<Self, UrlFieldError> {
        validate_github_path_segment(owner, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        validate_github_path_segment(repo, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        Ok(Self(format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        )))
    }

    /// Borrow the canonical `owner/repo` key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GithubRepoKey {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// GitHub HTTPS pull request URL.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GithubPullRequestUrl(Url);

impl GithubPullRequestUrl {
    /// Parse and validate a GitHub pull request URL.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, UrlFieldError> {
        value.as_ref().parse()
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for GithubPullRequestUrl {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GithubPullRequestUrl {
    type Err = UrlFieldError;

    /// Handles from str for this module.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = parse_url(value.trim(), GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE)?;
        validate_github_https_pull_request_url(&url)?;
        Ok(Self(url))
    }
}

impl_string_url_serde!(GithubPullRequestUrl);
impl_string_url_schema!(
    GithubPullRequestUrl,
    "GithubPullRequestUrl",
    r"^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/pull/[0-9]+$"
);

/// External HTTPS URL referenced by challenge-owned prepare metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalDataUrl(Url);

impl ExternalDataUrl {
    /// Parse and validate an external data URL.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, UrlFieldError> {
        value.as_ref().parse()
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ExternalDataUrl {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExternalDataUrl {
    type Err = UrlFieldError;

    /// Handles from str for this module.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = parse_url(value.trim(), EXTERNAL_DATA_URL_ERROR_MESSAGE)?;
        validate_https_url(&url, EXTERNAL_DATA_URL_ERROR_MESSAGE)?;
        Ok(Self(url))
    }
}

impl_string_url_serde!(ExternalDataUrl);
impl_string_url_schema!(ExternalDataUrl, "ExternalDataUrl", r"^https://[^?#]+$");

macro_rules! define_url_wrapper {
    ($type_name:ident, $schema_name:literal, $validator:ident, $message:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $type_name(Url);

        impl $type_name {
            /// Parse and validate this configured URL.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, UrlFieldError> {
                value.as_ref().parse()
            }

            /// Borrow the canonical string representation.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Clone the underlying URL for query construction.
            pub fn to_url(&self) -> Url {
                self.0.clone()
            }
        }

        impl fmt::Display for $type_name {
            /// Handles fmt for this module.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $type_name {
            type Err = UrlFieldError;

            /// Handles from str for this module.
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let url = parse_url(value.trim(), $message)?;
                $validator(&url)?;
                Ok(Self(url))
            }
        }

        impl_string_url_serde!($type_name);
        impl_string_url_schema!($type_name, $schema_name, r"^https?://[^?#]+$");
    };
}

define_url_wrapper!(
    GithubOauthRedirectUrl,
    "GithubOauthRedirectUrl",
    validate_oauth_redirect_url,
    GITHUB_OAUTH_REDIRECT_URL_ERROR_MESSAGE
);
define_url_wrapper!(
    GithubOauthAuthorizeUrl,
    "GithubOauthAuthorizeUrl",
    validate_github_oauth_authorize_url,
    GITHUB_OAUTH_AUTHORIZE_URL_ERROR_MESSAGE
);
define_url_wrapper!(
    GithubOauthTokenUrl,
    "GithubOauthTokenUrl",
    validate_github_oauth_token_url,
    GITHUB_OAUTH_TOKEN_URL_ERROR_MESSAGE
);
define_url_wrapper!(
    GithubApiUserUrl,
    "GithubApiUserUrl",
    validate_github_api_user_url,
    GITHUB_API_USER_URL_ERROR_MESSAGE
);

/// GitHub OAuth authorization URL with request query parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GithubOauthAuthorizationUrl(Url);

impl GithubOauthAuthorizationUrl {
    /// Validate a generated authorization URL.
    pub fn try_from_url(url: Url) -> Result<Self, UrlFieldError> {
        validate_github_oauth_authorization_url(&url)?;
        Ok(Self(url))
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for GithubOauthAuthorizationUrl {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GithubOauthAuthorizationUrl {
    type Err = UrlFieldError;

    /// Handles from str for this module.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = parse_url(value.trim(), GITHUB_OAUTH_AUTHORIZATION_URL_ERROR_MESSAGE)?;
        Self::try_from_url(url)
    }
}

impl_string_url_serde!(GithubOauthAuthorizationUrl);
impl_string_url_schema!(
    GithubOauthAuthorizationUrl,
    "GithubOauthAuthorizationUrl",
    r"^https://github\.com/login/oauth/authorize\?[^#]+$"
);

/// Parses url from an external boundary string.
fn parse_url(value: &str, message: &'static str) -> Result<Url, UrlFieldError> {
    reject_whitespace_or_control(value, message)?;
    Url::parse(value).map_err(|_| UrlFieldError::new(message))
}

/// Handles github https repo key for this module.
fn github_https_repo_key(url: &Url) -> Result<GithubRepoKey, UrlFieldError> {
    validate_github_https_base(url, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
    let segments = github_path_segments(url, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
    let [owner, repo_with_suffix] = segments.as_slice() else {
        return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
    };
    let repo = repo_with_suffix
        .strip_suffix(".git")
        .unwrap_or(repo_with_suffix.as_str());
    GithubRepoKey::try_new(owner, repo)
}

/// Validates github https pull request url invariants for this contract.
fn validate_github_https_pull_request_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_github_https_base(url, GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE)?;
    let segments = github_path_segments(url, GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE)?;
    let [owner, repo, pull, number] = segments.as_slice() else {
        return Err(UrlFieldError::new(GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE));
    };
    if pull != "pull" {
        return Err(UrlFieldError::new(GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE));
    }
    validate_github_path_segment(owner, GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE)?;
    validate_github_path_segment(repo, GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE)?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UrlFieldError::new(GITHUB_PULL_REQUEST_URL_ERROR_MESSAGE));
    }
    Ok(())
}

/// Validates oauth redirect url invariants for this contract.
fn validate_oauth_redirect_url(url: &Url) -> Result<(), UrlFieldError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.cannot_be_a_base()
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(UrlFieldError::new(GITHUB_OAUTH_REDIRECT_URL_ERROR_MESSAGE));
    }
    Ok(())
}

/// Validates github oauth authorize url invariants for this contract.
fn validate_github_oauth_authorize_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_exact_https_url(
        url,
        "github.com",
        "/login/oauth/authorize",
        GITHUB_OAUTH_AUTHORIZE_URL_ERROR_MESSAGE,
    )
}

/// Validates github oauth authorization url invariants for this contract.
fn validate_github_oauth_authorization_url(url: &Url) -> Result<(), UrlFieldError> {
    if url.scheme() != "https"
        || url.cannot_be_a_base()
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || url.path() != "/login/oauth/authorize"
        || url.query().is_none()
        || url.fragment().is_some()
    {
        return Err(UrlFieldError::new(
            GITHUB_OAUTH_AUTHORIZATION_URL_ERROR_MESSAGE,
        ));
    }
    Ok(())
}

/// Validates github oauth token url invariants for this contract.
fn validate_github_oauth_token_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_exact_https_url(
        url,
        "github.com",
        "/login/oauth/access_token",
        GITHUB_OAUTH_TOKEN_URL_ERROR_MESSAGE,
    )
}

/// Validates github api user url invariants for this contract.
fn validate_github_api_user_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_exact_https_url(
        url,
        "api.github.com",
        "/user",
        GITHUB_API_USER_URL_ERROR_MESSAGE,
    )
}

/// Validates exact https url invariants for this contract.
fn validate_exact_https_url(
    url: &Url,
    host: &str,
    path: &str,
    message: &'static str,
) -> Result<(), UrlFieldError> {
    validate_https_url(url, message)?;
    if url.host_str() != Some(host) || url.port().is_some() || url.path() != path {
        return Err(UrlFieldError::new(message));
    }
    Ok(())
}

/// Validates github https base invariants for this contract.
fn validate_github_https_base(url: &Url, message: &'static str) -> Result<(), UrlFieldError> {
    validate_https_url(url, message)?;
    if url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(UrlFieldError::new(message));
    }
    Ok(())
}

/// Validates https url invariants for this contract.
fn validate_https_url(url: &Url, message: &'static str) -> Result<(), UrlFieldError> {
    if url.scheme() != "https" || url.cannot_be_a_base() || url.host_str().is_none() {
        return Err(UrlFieldError::new(message));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(UrlFieldError::new(message));
    }
    Ok(())
}

/// Handles github path segments for this module.
fn github_path_segments(url: &Url, message: &'static str) -> Result<Vec<String>, UrlFieldError> {
    let Some(segments) = url.path_segments() else {
        return Err(UrlFieldError::new(message));
    };
    let segments: Vec<String> = segments.map(ToString::to_string).collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(UrlFieldError::new(message));
    }
    Ok(segments)
}

/// Validates github path segment invariants for this contract.
fn validate_github_path_segment(value: &str, message: &'static str) -> Result<(), UrlFieldError> {
    if value.is_empty()
        || value == ".git"
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(UrlFieldError::new(message));
    }
    Ok(())
}

/// Handles reject whitespace or control for this module.
fn reject_whitespace_or_control(value: &str, message: &'static str) -> Result<(), UrlFieldError> {
    if value.is_empty() || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Err(UrlFieldError::new(message))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExternalDataUrl, GithubPullRequestUrl, GithubRepoRemote};

    /// Verifies that parses github repo remotes.
    #[test]
    fn parses_github_repo_remotes() {
        let https =
            GithubRepoRemote::try_new("https://github.com/Agentics-Reifying/Agentics-Challenges")
                .expect("HTTPS remote is valid");
        let https_with_suffix = GithubRepoRemote::try_new(
            "https://github.com/agentics-reifying/agentics-challenges.git",
        )
        .expect("HTTPS .git remote is valid");
        let ssh =
            GithubRepoRemote::try_new("git@github.com:agentics-reifying/agentics-challenges.git")
                .expect("SSH remote is valid");

        assert_eq!(https.repository_key(), https_with_suffix.repository_key());
        assert_eq!(https.repository_key(), ssh.repository_key());
        assert_eq!(
            https.repository_key().as_str(),
            "agentics-reifying/agentics-challenges"
        );
        assert!(GithubRepoRemote::try_new("http://github.com/owner/repo").is_err());
        assert!(GithubRepoRemote::try_new("https://example.com/owner/repo").is_err());
        assert!(GithubRepoRemote::try_new("git@github.com:owner/repo").is_err());
    }

    /// Verifies that parses github pull request urls.
    #[test]
    fn parses_github_pull_request_urls() {
        assert!(
            GithubPullRequestUrl::try_new(
                "https://github.com/agentics-reifying/agentics-challenges/pull/7",
            )
            .is_ok()
        );
        assert!(
            GithubPullRequestUrl::try_new(
                "git@github.com:agentics-reifying/agentics-challenges.git",
            )
            .is_err()
        );
        assert!(GithubPullRequestUrl::try_new("https://github.com/owner/repo/issues/7").is_err());
    }

    /// Verifies that parses challenge external urls.
    #[test]
    fn parses_challenge_external_urls() {
        assert!(ExternalDataUrl::try_new("https://example.com/data.bin").is_ok());
        assert!(ExternalDataUrl::try_new("http://example.com/data.bin").is_err());
        assert!(ExternalDataUrl::try_new("https://example.com/data.bin#section").is_err());
    }
}
