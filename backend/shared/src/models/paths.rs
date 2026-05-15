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

impl AsRef<str> for RepoRelativePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<Path> for RepoRelativePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
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

macro_rules! define_relative_path_type {
    ($type_name:ident, $schema_name:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type_name(String);

        impl $type_name {
            /// Parse and validate a safe relative path.
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

        impl fmt::Display for $type_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $type_name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl AsRef<Path> for $type_name {
            fn as_ref(&self) -> &Path {
                self.as_path()
            }
        }

        impl FromStr for $type_name {
            type Err = AppError;

            fn from_str(value: &str) -> Result<Self> {
                Self::try_new(value)
            }
        }

        impl Serialize for $type_name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type_name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_new(&value).map_err(serde::de::Error::custom)
            }
        }

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
                    "pattern": r"^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$"
                })
            }
        }
    };
}

define_relative_path_type!(BundleRelativePath, "BundleRelativePath");
define_relative_path_type!(RunInputPath, "RunInputPath");
define_relative_path_type!(RunOutputPath, "RunOutputPath");
define_relative_path_type!(ProjectRelativePath, "ProjectRelativePath");
define_relative_path_type!(ScriptPath, "ScriptPath");
define_relative_path_type!(LogRelativePath, "LogRelativePath");

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

macro_rules! define_managed_path_type {
    ($type_name:ident, $schema_name:literal, $constructor:ident, $validator:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $type_name(PathBuf);

        impl $type_name {
            /// Canonicalize and verify a managed platform filesystem path.
            pub fn $constructor(path: impl AsRef<Path>) -> Result<Self> {
                $validator(path.as_ref(), $field).map(Self)
            }

            /// Borrow the canonical filesystem path.
            pub fn as_path(&self) -> &Path {
                &self.0
            }

            /// Borrow the canonical filesystem path as UTF-8 for storage.
            pub fn as_str(&self) -> Result<&str> {
                self.0.to_str().ok_or_else(|| {
                    AppError::Internal(format!("{} is not valid UTF-8", $field))
                })
            }
        }

        impl fmt::Display for $type_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.display())
            }
        }

        impl Serialize for $type_name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let value = self
                    .0
                    .to_str()
                    .ok_or_else(|| serde::ser::Error::custom(format!("{} is not valid UTF-8", $field)))?;
                serializer.serialize_str(value)
            }
        }

        impl<'de> Deserialize<'de> for $type_name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::$constructor(Path::new(&value)).map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $type_name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string" })
            }
        }
    };
}

define_managed_path_type!(
    ManagedBundlePath,
    "ManagedBundlePath",
    from_existing_dir,
    canonical_existing_dir_path,
    "managed bundle path"
);
define_managed_path_type!(
    ManagedStatementPath,
    "ManagedStatementPath",
    from_existing_file,
    canonical_existing_file_path,
    "managed statement path"
);

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

fn canonical_existing_file_path(path: &Path, field: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        AppError::BadRequest(format!("{field} does not exist or cannot be resolved: {e}"))
    })?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| AppError::BadRequest(format!("{field} cannot be inspected: {e}")))?;
    if !metadata.is_file() {
        return Err(AppError::BadRequest(format!("{field} must be a file")));
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
    use super::{
        BundleRelativePath, LogRelativePath, ProjectRelativePath, RepoRelativePath, RunInputPath,
        RunOutputPath, ScriptPath,
    };

    #[test]
    fn validates_repo_relative_paths() {
        for value in ["README.md", "v1", "challenges/sample-sum"] {
            assert!(RepoRelativePath::try_new(value).is_ok());
        }
        for value in ["", "/abs", "../escape", "a/../b", "a//b", "a b", "a\\b"] {
            assert!(RepoRelativePath::try_new(value).is_err());
        }
    }

    #[test]
    fn validates_manifest_and_runner_relative_paths() {
        for value in [
            "agentics.solution.json",
            "public/runs.json",
            "logs/build.txt",
        ] {
            assert!(BundleRelativePath::try_new(value).is_ok());
            assert!(RunInputPath::try_new(value).is_ok());
            assert!(RunOutputPath::try_new(value).is_ok());
            assert!(ProjectRelativePath::try_new(value).is_ok());
            assert!(ScriptPath::try_new(value).is_ok());
            assert!(LogRelativePath::try_new(value).is_ok());
        }
        for value in [
            "",
            "/abs",
            "../escape",
            "a/../b",
            "a//b",
            "a b",
            "a\\b",
            "a/\nb",
        ] {
            assert!(BundleRelativePath::try_new(value).is_err());
            assert!(RunInputPath::try_new(value).is_err());
            assert!(RunOutputPath::try_new(value).is_err());
            assert!(ProjectRelativePath::try_new(value).is_err());
            assert!(ScriptPath::try_new(value).is_err());
            assert!(LogRelativePath::try_new(value).is_err());
        }
    }
}
