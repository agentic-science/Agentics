//! Authentication token creation, hashing, and header parsing helpers.

use rand::Rng;
use sha2::{Digest, Sha256};

/// Parsed bearer-token authorization header.
#[derive(Debug, Clone)]
pub struct ParsedBearerToken {
    pub token: String,
}

/// Parsed basic-auth authorization header.
#[derive(Debug, Clone)]
pub struct ParsedBasicAuth {
    pub username: String,
    pub password: String,
}

/// Create an opaque bearer token for an agent.
pub fn create_agent_token() -> String {
    format!("agentics_{}", random_url_token(24))
}

/// Create an opaque browser session token.
pub fn create_web_session_token() -> String {
    format!("agentics_session_{}", random_url_token(32))
}

/// Create an opaque CSRF token bound to a browser session.
pub fn create_csrf_token() -> String {
    format!("agentics_csrf_{}", random_url_token(32))
}

/// Handles random url token for this module.
fn random_url_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::rng().fill_bytes(&mut bytes);
    base64_urlencode(&bytes)
}

/// Create an opaque OAuth state token.
pub fn create_oauth_state() -> String {
    format!("agentics_oauth_{}", random_url_token(32))
}

/// Hash an opaque token before storing or comparing it.
pub fn hash_opaque_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Handles base64 urlencode for this module.
fn base64_urlencode(input: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(input)
}

/// Hash an agent token before storing or comparing it.
pub fn hash_agent_token(token: &str) -> String {
    hash_opaque_token(token)
}

/// Parse an `Authorization: Bearer ...` header.
pub fn parse_bearer_token(value: Option<&str>) -> Option<ParsedBearerToken> {
    let value = value?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;

    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    if token.is_empty() {
        return None;
    }

    Some(ParsedBearerToken {
        token: token.to_string(),
    })
}

/// Parse an `Authorization: Basic ...` header.
pub fn parse_basic_auth(value: Option<&str>) -> Option<ParsedBasicAuth> {
    let value = value?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let encoded = parts.next()?;

    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }

    let decoded = base64_decode(encoded)?;
    let (username, password) = decoded.split_once(':')?;

    if username.is_empty() || password.is_empty() {
        return None;
    }

    Some(ParsedBasicAuth {
        username: username.to_string(),
        password: password.to_string(),
    })
}

/// Handles base64 decode for this module.
fn base64_decode(input: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(input).ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{create_agent_token, hash_agent_token};

    /// Verifies that creates agentics prefixed tokens.
    #[test]
    fn creates_agentics_prefixed_tokens() {
        let token = create_agent_token();
        assert!(token.starts_with("agentics_"));
        assert_ne!(hash_agent_token(&token), token);
    }
}
