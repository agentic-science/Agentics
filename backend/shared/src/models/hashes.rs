//! Validated hash-like values used by public API contracts.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use gix_hash::ObjectId;
use oci_spec::image::{Digest as OciDigest, DigestAlgorithm};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Number of bytes in a SHA-256 digest.
pub const SHA256_DIGEST_BYTES: usize = 32;

/// User-facing validation message for Git commit SHA values.
pub const GIT_COMMIT_SHA_ERROR_MESSAGE: &str =
    "commit_sha must be a full 40-character SHA-1 or 64-character SHA-256 Git object id";

/// User-facing validation message for plain SHA-256 digests.
pub const SHA256_DIGEST_ERROR_MESSAGE: &str =
    "SHA-256 digest must be exactly 64 hexadecimal characters";

/// User-facing validation message for OCI image SHA-256 digests.
pub const OCI_SHA256_DIGEST_ERROR_MESSAGE: &str =
    "OCI image digest must be exactly sha256: followed by 64 lowercase hexadecimal characters";

/// Validation failure for [`Sha256Digest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sha256DigestError;

impl fmt::Display for Sha256DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SHA256_DIGEST_ERROR_MESSAGE)
    }
}

impl std::error::Error for Sha256DigestError {}

/// Plain SHA-256 content digest stored as bytes and rendered as lowercase hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Digest([u8; SHA256_DIGEST_BYTES]);

impl Sha256Digest {
    /// Build a digest from its raw bytes.
    pub const fn from_bytes(bytes: [u8; SHA256_DIGEST_BYTES]) -> Self {
        Self(bytes)
    }

    /// Parse a 64-character hexadecimal SHA-256 digest.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, Sha256DigestError> {
        let value = value.as_ref();
        if value.trim() != value || value.len() != SHA256_DIGEST_BYTES * 2 {
            return Err(Sha256DigestError);
        }
        let mut bytes = [0; SHA256_DIGEST_BYTES];
        hex::decode_to_slice(value, &mut bytes).map_err(|_| Sha256DigestError)?;
        Ok(Self(bytes))
    }

    /// Borrow the digest bytes.
    pub fn as_bytes(&self) -> &[u8; SHA256_DIGEST_BYTES] {
        &self.0
    }

    /// Render the digest as lowercase hexadecimal text.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Sha256Digest {
    type Err = Sha256DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Sha256Digest {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Sha256Digest".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[0-9a-f]{64}$"
        })
    }
}

/// Validation failure for [`OciSha256Digest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OciSha256DigestError;

impl fmt::Display for OciSha256DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(OCI_SHA256_DIGEST_ERROR_MESSAGE)
    }
}

impl std::error::Error for OciSha256DigestError {}

/// OCI/Docker image digest serialized as `sha256:<64 lowercase hex>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OciSha256Digest(OciDigest);

impl OciSha256Digest {
    /// Parse and validate an OCI SHA-256 image digest.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, OciSha256DigestError> {
        let value = value.as_ref();
        if value.trim() != value {
            return Err(OciSha256DigestError);
        }
        let digest = OciDigest::from_str(value).map_err(|_| OciSha256DigestError)?;
        match digest.algorithm() {
            DigestAlgorithm::Sha256 => Ok(Self(digest)),
            _ => Err(OciSha256DigestError),
        }
    }

    /// Borrow the underlying OCI digest value.
    pub fn as_oci_digest(&self) -> &OciDigest {
        &self.0
    }

    /// Borrow the hex digest component without the `sha256:` algorithm prefix.
    pub fn hex_digest(&self) -> &str {
        self.0.digest()
    }
}

impl fmt::Display for OciSha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for OciSha256Digest {
    type Err = OciSha256DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

impl Serialize for OciSha256Digest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OciSha256Digest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for OciSha256Digest {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "OciSha256Digest".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^sha256:[0-9a-f]{64}$"
        })
    }
}

/// Validation failure for [`GitCommitSha`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitCommitShaError;

impl fmt::Display for GitCommitShaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(GIT_COMMIT_SHA_ERROR_MESSAGE)
    }
}

impl std::error::Error for GitCommitShaError {}

/// Full Git object id used to bind a challenge draft to a reviewed PR commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitCommitSha(ObjectId);

impl GitCommitSha {
    /// Parse and canonicalize a full Git SHA-1 or SHA-256 object id.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, GitCommitShaError> {
        let value = value.as_ref();
        if value.trim() != value {
            return Err(GitCommitShaError);
        }
        let value = value.to_ascii_lowercase();
        let object_id = ObjectId::from_hex(value.as_bytes()).map_err(|_| GitCommitShaError)?;
        Ok(Self(object_id))
    }

    /// Borrow the parsed Git object id.
    pub fn as_object_id(&self) -> &ObjectId {
        &self.0
    }
}

impl fmt::Display for GitCommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for GitCommitSha {
    type Err = GitCommitShaError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

impl Serialize for GitCommitSha {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GitCommitSha {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GitCommitSha {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "GitCommitSha".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^(?:[0-9a-f]{40}|[0-9a-f]{64})$"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{GitCommitSha, OciSha256Digest, Sha256Digest};

    #[test]
    fn validates_and_canonicalizes_sha256_digest() {
        let digest = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let parsed = Sha256Digest::try_new(digest).expect("digest is valid");

        assert_eq!(parsed.to_string(), digest);
        assert_eq!(parsed.as_bytes().len(), 32);
        assert_eq!(
            Sha256Digest::try_new(digest.to_ascii_uppercase())
                .expect("hex case should canonicalize")
                .to_string(),
            digest
        );
        assert!(Sha256Digest::try_new("abcdef").is_err());
        assert!(Sha256Digest::try_new(format!(" {digest}")).is_err());
        assert!(
            Sha256Digest::try_new(
                "g23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            )
            .is_err()
        );
    }

    #[test]
    fn serde_rejects_invalid_sha256_digest() {
        let digest = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let parsed: Sha256Digest =
            serde_json::from_str(&format!("\"{digest}\"")).expect("valid digest should parse");
        assert_eq!(parsed.to_string(), digest);
        assert!(serde_json::from_str::<Sha256Digest>("\"abcdef\"").is_err());
    }

    #[test]
    fn validates_oci_sha256_digest() {
        let hex = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let digest = format!("sha256:{hex}");
        let parsed = OciSha256Digest::try_new(&digest).expect("OCI digest is valid");

        assert_eq!(parsed.to_string(), digest);
        assert_eq!(parsed.hex_digest(), hex);
        assert!(OciSha256Digest::try_new(hex).is_err());
        assert!(OciSha256Digest::try_new(format!(" {digest}")).is_err());
        assert!(OciSha256Digest::try_new(format!("sha512:{hex}")).is_err());
        assert!(OciSha256Digest::try_new(digest.to_ascii_uppercase()).is_err());
    }

    #[test]
    fn serde_rejects_invalid_oci_sha256_digest() {
        let digest = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let parsed: OciSha256Digest =
            serde_json::from_str(&format!("\"{digest}\"")).expect("valid digest should parse");
        assert_eq!(parsed.to_string(), digest);
        assert!(serde_json::from_str::<OciSha256Digest>("\"abcdef\"").is_err());
    }

    #[test]
    fn validates_and_canonicalizes_git_commit_sha() {
        let sha1 = "0123456789abcdef0123456789abcdef01234567";
        let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        assert_eq!(
            GitCommitSha::try_new(sha1)
                .expect("sha1 is valid")
                .to_string(),
            sha1
        );
        assert_eq!(
            GitCommitSha::try_new(sha256)
                .expect("sha256 is valid")
                .to_string(),
            sha256
        );
        assert_eq!(
            GitCommitSha::try_new(sha1.to_ascii_uppercase())
                .expect("hex case should canonicalize")
                .to_string(),
            sha1
        );
        assert!(GitCommitSha::try_new("0123456789abcdef").is_err());
        assert!(GitCommitSha::try_new(format!(" {sha1}")).is_err());
        assert!(GitCommitSha::try_new("g123456789abcdef0123456789abcdef01234567").is_err());
    }

    #[test]
    fn serde_rejects_invalid_git_commit_sha() {
        let sha1 = "0123456789abcdef0123456789abcdef01234567";
        let parsed: GitCommitSha =
            serde_json::from_str(&format!("\"{sha1}\"")).expect("valid sha should deserialize");
        assert_eq!(parsed.to_string(), sha1);
        assert!(serde_json::from_str::<GitCommitSha>("\"0123456789abcdef\"").is_err());
    }
}
