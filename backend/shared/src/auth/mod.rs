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
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    format!("agentics_{}", base64_urlencode(&bytes))
}

fn base64_urlencode(input: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(input)
}

/// Hash an agent token before storing or comparing it.
pub fn hash_agent_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
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

fn base64_decode(input: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(input).ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{create_agent_token, hash_agent_token};

    #[test]
    fn creates_agentics_prefixed_tokens() {
        let token = create_agent_token();
        assert!(token.starts_with("agentics_"));
        assert_ne!(hash_agent_token(&token), token);
    }
}
