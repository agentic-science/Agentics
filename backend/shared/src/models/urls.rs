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

/// User-facing validation message for Moltbook Submolt URLs.
pub const MOLTBOOK_SUBMOLT_URL_ERROR_MESSAGE: &str = "moltbook_submolt_url must be an HTTPS Moltbook Submolt URL under https://www.moltbook.com/submolts/";

/// User-facing validation message for external data URLs.
pub const EXTERNAL_DATA_URL_ERROR_MESSAGE: &str = "external data url must be an HTTPS URL";

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for UrlFieldError {}

macro_rules! impl_string_url_serde {
    ($type_name:ident) => {
        impl Serialize for $type_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type_name {
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
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

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
    Https(Url),
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
            Self::Https(url) => url.as_str(),
            Self::Ssh(remote) => remote.as_str(),
        }
    }
}

impl fmt::Display for GithubRepoRemote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GithubRepoRemote {
    type Err = UrlFieldError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.starts_with("git@github.com:") {
            return Ok(Self::Ssh(GithubSshRepoRemote::try_new(value)?));
        }

        let url = parse_url(value, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        validate_github_https_repo_url(&url)?;
        Ok(Self::Https(url))
    }
}

impl Serialize for GithubRepoRemote {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GithubRepoRemote {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GithubRepoRemote {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "GithubRepoRemote".into()
    }

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
}

impl GithubSshRepoRemote {
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
        validate_github_path_segment(owner, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        validate_github_path_segment(repo, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
        Ok(Self {
            value: format!("git@github.com:{owner}/{repo}.git"),
        })
    }

    fn as_str(&self) -> &str {
        &self.value
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GithubPullRequestUrl {
    type Err = UrlFieldError;

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

/// Moltbook Submolt URL used to connect challenge discussion spaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoltbookSubmoltUrl(Url);

impl MoltbookSubmoltUrl {
    /// Parse and validate a Moltbook Submolt URL.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, UrlFieldError> {
        value.as_ref().parse()
    }

    /// Borrow the canonical string representation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for MoltbookSubmoltUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MoltbookSubmoltUrl {
    type Err = UrlFieldError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = parse_url(value.trim(), MOLTBOOK_SUBMOLT_URL_ERROR_MESSAGE)?;
        validate_moltbook_submolt_url(&url)?;
        Ok(Self(url))
    }
}

impl_string_url_serde!(MoltbookSubmoltUrl);
impl_string_url_schema!(
    MoltbookSubmoltUrl,
    "MoltbookSubmoltUrl",
    r"^https://www\.moltbook\.com/submolts/.+"
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExternalDataUrl {
    type Err = UrlFieldError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = parse_url(value.trim(), EXTERNAL_DATA_URL_ERROR_MESSAGE)?;
        validate_https_url(&url, EXTERNAL_DATA_URL_ERROR_MESSAGE)?;
        Ok(Self(url))
    }
}

impl_string_url_serde!(ExternalDataUrl);
impl_string_url_schema!(ExternalDataUrl, "ExternalDataUrl", r"^https://.+");

fn parse_url(value: &str, message: &'static str) -> Result<Url, UrlFieldError> {
    reject_whitespace_or_control(value, message)?;
    Url::parse(value).map_err(|_| UrlFieldError::new(message))
}

fn validate_github_https_repo_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_github_https_base(url, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
    let segments = github_path_segments(url, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
    let [owner, repo_with_suffix] = segments.as_slice() else {
        return Err(UrlFieldError::new(GITHUB_REPO_REMOTE_ERROR_MESSAGE));
    };
    validate_github_path_segment(owner, GITHUB_REPO_REMOTE_ERROR_MESSAGE)?;
    let repo = repo_with_suffix
        .strip_suffix(".git")
        .unwrap_or(repo_with_suffix.as_str());
    validate_github_path_segment(repo, GITHUB_REPO_REMOTE_ERROR_MESSAGE)
}

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

fn validate_moltbook_submolt_url(url: &Url) -> Result<(), UrlFieldError> {
    validate_https_url(url, MOLTBOOK_SUBMOLT_URL_ERROR_MESSAGE)?;
    if url.host_str() != Some("www.moltbook.com")
        || url.port().is_some()
        || !url.path().starts_with("/submolts/")
        || url.path() == "/submolts/"
    {
        return Err(UrlFieldError::new(MOLTBOOK_SUBMOLT_URL_ERROR_MESSAGE));
    }
    Ok(())
}

fn validate_https_url(url: &Url, message: &'static str) -> Result<(), UrlFieldError> {
    if url.scheme() != "https" || url.cannot_be_a_base() || url.host_str().is_none() {
        return Err(UrlFieldError::new(message));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(UrlFieldError::new(message));
    }
    Ok(())
}

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

fn reject_whitespace_or_control(value: &str, message: &'static str) -> Result<(), UrlFieldError> {
    if value.is_empty() || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Err(UrlFieldError::new(message))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExternalDataUrl, GithubPullRequestUrl, GithubRepoRemote, MoltbookSubmoltUrl};

    #[test]
    fn parses_github_repo_remotes() {
        assert!(
            GithubRepoRemote::try_new("https://github.com/agentics-reifying/agentics-challenges")
                .is_ok()
        );
        assert!(
            GithubRepoRemote::try_new("git@github.com:agentics-reifying/agentics-challenges.git")
                .is_ok()
        );
        assert!(GithubRepoRemote::try_new("http://github.com/owner/repo").is_err());
        assert!(GithubRepoRemote::try_new("https://example.com/owner/repo").is_err());
        assert!(GithubRepoRemote::try_new("git@github.com:owner/repo").is_err());
    }

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

    #[test]
    fn parses_challenge_external_urls() {
        assert!(ExternalDataUrl::try_new("https://example.com/data.bin").is_ok());
        assert!(ExternalDataUrl::try_new("http://example.com/data.bin").is_err());
        assert!(ExternalDataUrl::try_new("https://example.com/data.bin#section").is_err());
    }

    #[test]
    fn parses_moltbook_submolt_urls() {
        assert!(
            MoltbookSubmoltUrl::try_new("https://www.moltbook.com/submolts/sample-sum").is_ok()
        );
        assert!(MoltbookSubmoltUrl::try_new("https://www.moltbook.com/about").is_err());
        assert!(MoltbookSubmoltUrl::try_new("https://example.com/submolts/sample-sum").is_err());
    }
}
