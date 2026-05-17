//! Typed pioneer-code values used to gate MVP agent registration.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use rand::Rng;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Generic text returned for codes that cannot currently be consumed.
pub const INVALID_OR_UNAVAILABLE_PIONEER_CODE: &str = "invalid or unavailable pioneer code";

/// Error returned when a pioneer-code string violates the public grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PioneerCodeError;

/// Lifecycle state for an admin-created pioneer code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PioneerCodeStatus {
    Active,
    Revoked,
}

impl PioneerCodeStatus {
    /// Stable database string for a pioneer-code lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    /// Parse a stable database string for a pioneer-code lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

impl fmt::Display for PioneerCodeStatus {
    /// Format the pioneer-code status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Registration flow recorded for a consumed pioneer code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PioneerCodeUseKind {
    AgentApi,
    CreatorOauth,
}

impl PioneerCodeUseKind {
    /// Stable database string for a pioneer-code use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentApi => "agent_api",
            Self::CreatorOauth => "creator_oauth",
        }
    }

    /// Parse the stable database string for a pioneer-code use.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "agent_api" => Some(Self::AgentApi),
            "creator_oauth" => Some(Self::CreatorOauth),
            _ => None,
        }
    }
}

impl fmt::Display for PioneerCodeUseKind {
    /// Format the use kind as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for PioneerCodeError {
    /// Writes the stable validation error without revealing code contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "pioneer_code must be 8 lowercase hex chars or <label>-<8 lowercase hex chars>; label may use lowercase letters, digits, or _ and must be at most 6 chars",
        )
    }
}

impl std::error::Error for PioneerCodeError {}

/// Secret registration code supplied by agents and creator OAuth users.
#[derive(Clone)]
pub struct PioneerCode(SecretString);

impl PioneerCode {
    /// Parse a code and retain it in a redacted secret wrapper.
    pub fn try_new(value: impl Into<String>) -> Result<Self, PioneerCodeError> {
        let value = value.into();
        validate_pioneer_code(&value)?;
        Ok(Self(SecretString::from(value)))
    }

    /// Generate a random code, optionally prefixed by a validated label.
    pub fn generate(label: Option<&str>) -> Result<Self, PioneerCodeError> {
        let mut bytes = [0u8; 4];
        rand::rng().fill_bytes(&mut bytes);
        let random_hex = hex::encode(bytes);
        let code = match label {
            Some(label) => {
                validate_pioneer_label(label)?;
                format!("{label}-{random_hex}")
            }
            None => random_hex,
        };
        Self::try_new(code)
    }

    /// Expose the code at the boundary that must hash or transmit it.
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }

    /// Return the optional label encoded in the code display text.
    pub fn label(&self) -> Option<&str> {
        self.expose_secret()
            .split_once('-')
            .map(|(label, _random)| label)
    }
}

/// Redacted pioneer-code input accepted at public registration boundaries.
#[derive(Clone)]
pub struct PioneerCodeInput(SecretString);

impl PioneerCodeInput {
    /// Store a raw code candidate without validating its public grammar.
    pub fn try_new(value: impl Into<String>) -> Result<Self, PioneerCodeError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PioneerCodeError);
        }
        Ok(Self(SecretString::from(value)))
    }

    /// Expose the raw code only where it must be validated, hashed, or sent.
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for PioneerCodeInput {
    /// Redact the code from debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PioneerCodeInput([redacted])")
    }
}

impl Serialize for PioneerCodeInput {
    /// Serialize the secret at the outgoing HTTP request boundary.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for PioneerCodeInput {
    /// Deserialize the raw secret without exposing grammar-specific failures.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for PioneerCodeInput {
    /// Keep the boundary-input schema inline as a plain string.
    fn inline_schema() -> bool {
        true
    }

    /// Return the schema component name used by generated clients.
    fn schema_name() -> Cow<'static, str> {
        "PioneerCodeInput".into()
    }

    /// Describe only the wire type for public registration inputs.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string" })
    }
}

impl fmt::Debug for PioneerCode {
    /// Redact the code from debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PioneerCode([redacted])")
    }
}

impl fmt::Display for PioneerCode {
    /// Redact the code from display output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted pioneer code]")
    }
}

impl FromStr for PioneerCode {
    type Err = PioneerCodeError;

    /// Parse a pioneer code from its wire string.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value.to_string())
    }
}

impl Serialize for PioneerCode {
    /// Serialize the secret at the outgoing HTTP request boundary.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for PioneerCode {
    /// Deserialize and validate the incoming secret code.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for PioneerCode {
    /// Keep the code schema inline so request DTOs stay string-shaped.
    fn inline_schema() -> bool {
        true
    }

    /// Return the schema component name used by generated clients.
    fn schema_name() -> Cow<'static, str> {
        "PioneerCode".into()
    }

    /// Describe the public code grammar without exposing examples from storage.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^([a-z0-9_]{1,6}-)?[0-9a-f]{8}$"
        })
    }
}

/// Validate and normalize no part of a supplied code.
fn validate_pioneer_code(value: &str) -> Result<(), PioneerCodeError> {
    if let Some((label, random_hex)) = value.split_once('-') {
        if random_hex.contains('-') {
            return Err(PioneerCodeError);
        }
        validate_pioneer_label(label)?;
        validate_random_hex(random_hex)?;
    } else {
        validate_random_hex(value)?;
    }
    Ok(())
}

/// Validate the optional human-selected prefix that is part of the code.
fn validate_pioneer_label(label: &str) -> Result<(), PioneerCodeError> {
    if label.is_empty() || label.len() > 6 {
        return Err(PioneerCodeError);
    }
    if !label
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
    {
        return Err(PioneerCodeError);
    }
    Ok(())
}

/// Validate the random suffix carried by every pioneer code.
fn validate_random_hex(random_hex: &str) -> Result<(), PioneerCodeError> {
    if random_hex.len() != 8 {
        return Err(PioneerCodeError);
    }
    if !random_hex
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(PioneerCodeError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PioneerCode;

    /// Verifies accepted pioneer-code grammar variants.
    #[test]
    fn accepts_plain_and_labeled_codes() {
        let plain = PioneerCode::try_new("deadbeef").expect("plain code should parse");
        assert_eq!(plain.expose_secret(), "deadbeef");
        assert_eq!(plain.label(), None);

        let labeled = PioneerCode::try_new("jack_1-deadbeef").expect("labeled code should parse");
        assert_eq!(labeled.expose_secret(), "jack_1-deadbeef");
        assert_eq!(labeled.label(), Some("jack_1"));
    }

    /// Verifies invalid code forms are rejected before hashing or storage.
    #[test]
    fn rejects_invalid_codes() {
        for value in [
            "",
            "DEADBEEF",
            "deadbee",
            "deadbeef00",
            "labeltoolong-deadbeef",
            "bad-label-deadbeef",
            "bad!-deadbeef",
            "-deadbeef",
            "jack-DEADBEEF",
            "jack-deadbee!",
        ] {
            assert!(PioneerCode::try_new(value).is_err(), "{value}");
        }
    }

    /// Verifies generated labeled codes preserve the requested label.
    #[test]
    fn generated_labeled_code_keeps_label() {
        let code = PioneerCode::generate(Some("jack")).expect("generated code should be valid");
        assert_eq!(code.label(), Some("jack"));
        assert!(code.expose_secret().starts_with("jack-"));
    }

    /// Verifies serde keeps the public wire shape as a JSON string.
    #[test]
    fn serde_uses_string_wire_shape() {
        let code: PioneerCode =
            serde_json::from_str("\"deadbeef\"").expect("valid code should deserialize");
        assert_eq!(code.expose_secret(), "deadbeef");
        assert_eq!(
            serde_json::to_string(&code).expect("code should serialize"),
            "\"deadbeef\""
        );
    }
}
