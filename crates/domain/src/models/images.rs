//! Typed Docker and OCI image references used by challenge resource profiles.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use oci_spec::distribution::Reference as OciDistributionReference;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::hashes::{OCI_SHA256_DIGEST_ERROR_MESSAGE, OciSha256Digest};

const LOCAL_IMAGE_REFERENCE_ERROR_MESSAGE: &str =
    "local image reference must be a supported Agentics local repository with an explicit tag";
const REGISTRY_IMAGE_REFERENCE_ERROR_MESSAGE: &str =
    "registry image reference must include an explicit registry, repository, and tag";

/// Local Agentics image repositories accepted for development-only challenge specs.
pub const SUPPORTED_LOCAL_AGENTICS_IMAGE_REPOSITORIES: &[&str] =
    &["agentics-linux-arm64-cpu", "agentics-linux-arm64-cuda"];

/// Validation failure for local Agentics image references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgenticsImageReferenceError;

impl fmt::Display for LocalAgenticsImageReferenceError {
    /// Render the stable user-facing validation message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(LOCAL_IMAGE_REFERENCE_ERROR_MESSAGE)
    }
}

impl std::error::Error for LocalAgenticsImageReferenceError {}

/// Development-only Docker image reference for first-party Agentics local images.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalAgenticsImageReference {
    original: String,
    repository: String,
    tag: String,
}

impl LocalAgenticsImageReference {
    /// Parse and validate a supported local Agentics Docker image reference.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, LocalAgenticsImageReferenceError> {
        let value = value.as_ref();
        if value.trim() != value || value.is_empty() || value.contains('/') || value.contains('@') {
            return Err(LocalAgenticsImageReferenceError);
        }
        let Some((repository, tag)) = value.rsplit_once(':') else {
            return Err(LocalAgenticsImageReferenceError);
        };
        if !SUPPORTED_LOCAL_AGENTICS_IMAGE_REPOSITORIES.contains(&repository)
            || tag.is_empty()
            || !tag
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
        {
            return Err(LocalAgenticsImageReferenceError);
        }

        Ok(Self {
            original: value.to_string(),
            repository: repository.to_string(),
            tag: tag.to_string(),
        })
    }

    /// Borrow the exact Docker reference used at runtime.
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Borrow the local repository name used by Agentics image-family policy checks.
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Borrow the explicit local Docker image tag.
    pub fn tag(&self) -> &str {
        &self.tag
    }
}

impl fmt::Display for LocalAgenticsImageReference {
    /// Render the Docker reference used by local development containers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LocalAgenticsImageReference {
    type Err = LocalAgenticsImageReferenceError;

    /// Parse a local Agentics Docker image reference.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

impl Serialize for LocalAgenticsImageReference {
    /// Serialize as the original Docker reference string.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LocalAgenticsImageReference {
    /// Deserialize and validate a local Agentics Docker image reference.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for LocalAgenticsImageReference {
    /// Render this domain value inline as a JSON string.
    fn inline_schema() -> bool {
        true
    }

    /// Return the schema name used when this value is referenced directly.
    fn schema_name() -> Cow<'static, str> {
        "LocalAgenticsImageReference".into()
    }

    /// Build a string schema matching supported local Agentics image references.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^(agentics-linux-arm64-cpu|agentics-linux-arm64-cuda):[A-Za-z0-9_.-]+$"
        })
    }
}

/// Validation failure for registry-backed OCI image references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciRegistryImageReferenceError(String);

impl fmt::Display for OciRegistryImageReferenceError {
    /// Render a stable user-facing validation message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OciRegistryImageReferenceError {}

/// Registry image reference parsed with the OCI Distribution reference type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OciRegistryImageReference {
    original: String,
    parsed: OciDistributionReference,
    digest: Option<OciSha256Digest>,
    tag: String,
}

impl OciRegistryImageReference {
    /// Parse and validate an OCI registry image reference for hosted execution.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, OciRegistryImageReferenceError> {
        let value = value.as_ref();
        if value.trim() != value
            || value.is_empty()
            || !has_explicit_registry(value)
            || !has_explicit_tag(value)
        {
            return Err(OciRegistryImageReferenceError(
                REGISTRY_IMAGE_REFERENCE_ERROR_MESSAGE.to_string(),
            ));
        }
        let parsed = OciDistributionReference::from_str(value).map_err(|error| {
            OciRegistryImageReferenceError(format!(
                "{REGISTRY_IMAGE_REFERENCE_ERROR_MESSAGE}: {error}"
            ))
        })?;
        let digest = parsed
            .digest()
            .map(|digest| {
                OciSha256Digest::try_new(digest).map_err(|_| {
                    OciRegistryImageReferenceError(OCI_SHA256_DIGEST_ERROR_MESSAGE.to_string())
                })
            })
            .transpose()?;
        let Some(tag) = parsed.tag().map(ToOwned::to_owned) else {
            return Err(OciRegistryImageReferenceError(
                REGISTRY_IMAGE_REFERENCE_ERROR_MESSAGE.to_string(),
            ));
        };

        Ok(Self {
            original: value.to_string(),
            parsed,
            digest,
            tag,
        })
    }

    /// Borrow the exact Docker reference used at runtime.
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Borrow the parsed OCI distribution reference.
    pub fn as_oci_reference(&self) -> &OciDistributionReference {
        &self.parsed
    }

    /// Borrow the registry-qualified repository used by Agentics policy checks.
    pub fn policy_repository(&self) -> String {
        format!("{}/{}", self.parsed.registry(), self.parsed.repository())
    }

    /// Borrow the explicit registry image tag.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Borrow the embedded immutable SHA-256 digest, when present.
    pub fn digest(&self) -> Option<&OciSha256Digest> {
        self.digest.as_ref()
    }
}

impl fmt::Display for OciRegistryImageReference {
    /// Render the Docker reference used by hosted containers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OciRegistryImageReference {
    type Err = OciRegistryImageReferenceError;

    /// Parse an OCI registry image reference.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

impl Serialize for OciRegistryImageReference {
    /// Serialize as the original registry image reference string.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OciRegistryImageReference {
    /// Deserialize and validate an OCI registry image reference.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for OciRegistryImageReference {
    /// Render this domain value inline as a JSON string.
    fn inline_schema() -> bool {
        true
    }

    /// Return the schema name used when this value is referenced directly.
    fn schema_name() -> Cow<'static, str> {
        "OciRegistryImageReference".into()
    }

    /// Build a string schema for explicit registry image references.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[^/.:]+[.:][^/]*/[^\\s@:]+(/[^\\s@:]+)*:[^\\s@]+(@sha256:[0-9a-f]{64})?$"
        })
    }
}

/// Image source declared for a challenge solution or evaluator container.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ChallengeImageReference {
    Local {
        reference: LocalAgenticsImageReference,
    },
    Registry {
        reference: OciRegistryImageReference,
    },
}

impl ChallengeImageReference {
    /// Borrow the Docker reference string used by runner containers.
    pub fn docker_reference(&self) -> &str {
        match self {
            Self::Local { reference } => reference.as_str(),
            Self::Registry { reference } => reference.as_str(),
        }
    }

    /// Borrow the repository string used by supported-image policy validation.
    pub fn policy_repository(&self) -> Cow<'_, str> {
        match self {
            Self::Local { reference } => Cow::Borrowed(reference.repository()),
            Self::Registry { reference } => Cow::Owned(reference.policy_repository()),
        }
    }

    /// Borrow the explicit Docker image tag.
    pub fn tag(&self) -> &str {
        match self {
            Self::Local { reference } => reference.tag(),
            Self::Registry { reference } => reference.tag(),
        }
    }

    /// Borrow the embedded immutable registry digest, when present.
    pub fn digest(&self) -> Option<&OciSha256Digest> {
        match self {
            Self::Local { .. } => None,
            Self::Registry { reference } => reference.digest(),
        }
    }

    /// Return whether the reference is local-development-only.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }
}

impl fmt::Display for ChallengeImageReference {
    /// Render the Docker reference used by runner containers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.docker_reference())
    }
}

/// Return whether the image text contains an explicit registry component.
fn has_explicit_registry(value: &str) -> bool {
    let Some((registry, _)) = value.split_once('/') else {
        return false;
    };
    registry == "localhost" || registry.contains('.') || registry.contains(':')
}

/// Return whether the image text contains an explicit tag before any digest suffix.
fn has_explicit_tag(value: &str) -> bool {
    let image_without_digest = value
        .split_once('@')
        .map_or(value, |(reference, _digest)| reference);
    let Some(tag_separator) = image_without_digest.rfind(':') else {
        return false;
    };
    let slash = image_without_digest.rfind('/');
    slash.is_none_or(|slash| tag_separator > slash)
}

#[cfg(test)]
mod tests {
    use super::{ChallengeImageReference, LocalAgenticsImageReference, OciRegistryImageReference};

    /// Verifies supported Agentics local images parse and preserve their runtime reference.
    #[test]
    fn local_agentics_image_accepts_supported_tagged_images() {
        let reference =
            LocalAgenticsImageReference::try_new("agentics-linux-arm64-cpu:ubuntu26.04-local")
                .expect("local Agentics image should parse");

        assert_eq!(reference.repository(), "agentics-linux-arm64-cpu");
        assert_eq!(reference.tag(), "ubuntu26.04-local");
        assert_eq!(
            reference.as_str(),
            "agentics-linux-arm64-cpu:ubuntu26.04-local"
        );
    }

    /// Verifies local image references reject registries, digests, and unsupported repos.
    #[test]
    fn local_agentics_image_rejects_non_local_or_unsupported_images() {
        for invalid in [
            "agentics-linux-arm64-cpu",
            "ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-local",
            "agentics-linux-arm64-cpu:ubuntu26.04-local@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "python:3.12-slim-bookworm",
            " agentics-linux-arm64-cpu:ubuntu26.04-local",
        ] {
            assert!(
                LocalAgenticsImageReference::try_new(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    /// Verifies registry image references require explicit registry and tag syntax.
    #[test]
    fn registry_image_requires_explicit_registry_and_tag() {
        for invalid in [
            "agentics-linux-arm64-cpu:ubuntu26.04-local",
            "ghcr.io/agentic-science/agentics-linux-arm64-cpu",
            "busybox",
            " ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0",
        ] {
            assert!(
                OciRegistryImageReference::try_new(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    /// Verifies registry image references parse tags and SHA-256 digests.
    #[test]
    fn registry_image_accepts_digest_pinned_ghcr_references() {
        let reference = OciRegistryImageReference::try_new(format!(
            "ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@sha256:{}",
            "a".repeat(64)
        ))
        .expect("digest-pinned registry image should parse");

        assert_eq!(reference.tag(), "ubuntu26.04-v0.1.0");
        assert_eq!(
            reference.policy_repository(),
            "ghcr.io/agentic-science/agentics-linux-arm64-cpu"
        );
        assert!(reference.digest().is_some());
    }

    /// Verifies non-SHA-256 registry digests are outside the Agentics image contract.
    #[test]
    fn registry_image_rejects_non_sha256_digests() {
        let invalid = format!(
            "ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@sha512:{}",
            "a".repeat(128)
        );

        assert!(OciRegistryImageReference::try_new(invalid).is_err());
    }

    /// Verifies the enum serializes to the explicit source-tagged JSON contract.
    #[test]
    fn challenge_image_reference_serializes_with_source_tag() {
        let reference = ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(
                "agentics-linux-arm64-cpu:ubuntu26.04-local",
            )
            .expect("local image should parse"),
        };

        let value = serde_json::to_value(reference).expect("image reference should serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "source": "local",
                "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
            })
        );
    }
}
