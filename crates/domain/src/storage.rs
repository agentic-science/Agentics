use std::borrow::Cow;
use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub type Result<T> = std::result::Result<T, StorageKeyError>;

/// Storage-key parse failures before mapping to a storage backend error.
#[derive(Debug, thiserror::Error)]
pub enum StorageKeyError {
    #[error(
        "storage key must be a non-empty relative path with safe ASCII components and no `.` or `..` components"
    )]
    InvalidKey,
}

/// Opaque object key relative to the configured Agentics storage namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    /// Parse and validate a storage-relative object key.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self> {
        validate_storage_key(value.as_ref()).map(Self)
    }

    /// Borrow the storage key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the safe relative storage key as a path.
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl fmt::Display for StorageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StorageKey {
    type Err = StorageKeyError;

    fn from_str(value: &str) -> Result<Self> {
        Self::try_new(value)
    }
}

impl Serialize for StorageKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StorageKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for StorageKey {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "StorageKey".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$"
        })
    }
}

fn validate_storage_key(value: &str) -> Result<String> {
    if value.is_empty()
        || value.trim() != value
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(StorageKeyError::InvalidKey);
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(StorageKeyError::InvalidKey);
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(part) = part.to_str() else {
                    return Err(StorageKeyError::InvalidKey);
                };
                if part.is_empty()
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    })
                {
                    return Err(StorageKeyError::InvalidKey);
                }
                parts.push(part);
            }
            _ => return Err(StorageKeyError::InvalidKey),
        }
    }
    if parts.is_empty() || parts.join("/") != value {
        return Err(StorageKeyError::InvalidKey);
    }
    Ok(value.to_string())
}
