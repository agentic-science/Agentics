//! Validated path-like types used at API and repository boundaries.

use std::borrow::Cow;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{AppError, Result};

/// User-facing validation message for repository-relative paths.
pub const REPO_RELATIVE_PATH_ERROR_MESSAGE: &str =
    "repo-relative paths must be non-empty safe relative paths with ASCII components";

/// Path relative to a challenge repository checkout.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoRelativePath(String);

impl RepoRelativePath {
    /// Parse and validate a repository-relative path.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self> {
        validate_relative_path(value.as_ref()).map(Self)
    }

    /// Borrow the path as a string using `/` separators.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrow the path for filesystem joins.
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl fmt::Display for RepoRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RepoRelativePath {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self> {
        Self::try_new(value)
    }
}

impl Serialize for RepoRelativePath {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RepoRelativePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for RepoRelativePath {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "RepoRelativePath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$"
        })
    }
}

/// Canonical server-local repository checkout path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCheckoutPath(PathBuf);

impl RepositoryCheckoutPath {
    /// Canonicalize and verify an existing repository checkout directory.
    pub fn from_existing_dir(path: impl AsRef<str>) -> Result<Self> {
        canonical_existing_dir(path.as_ref(), "repository_path").map(Self)
    }

    /// Borrow the canonical filesystem path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for RepositoryCheckoutPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Canonical server-local admin bundle path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminBundlePath(PathBuf);

impl AdminBundlePath {
    /// Canonicalize and verify an existing admin bundle directory.
    pub fn from_existing_dir(path: impl AsRef<Path>) -> Result<Self> {
        canonical_existing_dir_path(path.as_ref(), "bundle_path").map(Self)
    }

    /// Borrow the canonical filesystem path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for AdminBundlePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

fn canonical_existing_dir(value: &str, field: &str) -> Result<PathBuf> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest(format!(
            "{field} must be a valid directory path"
        )));
    }
    canonical_existing_dir_path(Path::new(value), field)
}

fn canonical_existing_dir_path(path: &Path, field: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        AppError::BadRequest(format!("{field} does not exist or cannot be resolved: {e}"))
    })?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| AppError::BadRequest(format!("{field} cannot be inspected: {e}")))?;
    if !metadata.is_dir() {
        return Err(AppError::BadRequest(format!("{field} must be a directory")));
    }
    Ok(canonical)
}

fn validate_relative_path(value: &str) -> Result<String> {
    if value.is_empty()
        || value.trim() != value
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(AppError::BadRequest(
            REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(AppError::BadRequest(
            REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
        ));
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(part) = part.to_str() else {
                    return Err(AppError::BadRequest(
                        REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
                    ));
                };
                if part.is_empty()
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    })
                {
                    return Err(AppError::BadRequest(
                        REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
                    ));
                }
                parts.push(part);
            }
            _ => {
                return Err(AppError::BadRequest(
                    REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
                ));
            }
        }
    }
    if parts.is_empty() || parts.join("/") != value {
        return Err(AppError::BadRequest(
            REPO_RELATIVE_PATH_ERROR_MESSAGE.to_string(),
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::RepoRelativePath;

    #[test]
    fn validates_repo_relative_paths() {
        for value in ["README.md", "v1", "challenges/sample-sum"] {
            assert!(RepoRelativePath::try_new(value).is_ok());
        }
        for value in ["", "/abs", "../escape", "a/../b", "a//b", "a b", "a\\b"] {
            assert!(RepoRelativePath::try_new(value).is_err());
        }
    }
}
