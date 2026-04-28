use rand::Rng;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct ParsedBearerToken {
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct ParsedBasicAuth {
    pub username: String,
    pub password: String,
}

pub fn create_agent_token() -> String {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    format!("llmoj_{}", base64_urlencode(&bytes))
}

fn base64_urlencode(input: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(input)
}

pub fn hash_agent_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn parse_bearer_token(value: Option<&str>) -> Option<ParsedBearerToken> {
    let value = value?;
    let parts: Vec<&str> = value.trim().split_whitespace().collect();

    if parts.len() != 2 || parts[0].to_lowercase() != "bearer" {
        return None;
    }

    let token = parts[1];
    if token.is_empty() {
        return None;
    }

    Some(ParsedBearerToken {
        token: token.to_string(),
    })
}

pub fn parse_basic_auth(value: Option<&str>) -> Option<ParsedBasicAuth> {
    let value = value?;
    let parts: Vec<&str> = value.trim().split_whitespace().collect();

    if parts.len() != 2 || parts[0].to_lowercase() != "basic" {
        return None;
    }

    let decoded = base64_decode(parts[1])?;
    let separator_idx = decoded.find(':')?;

    let username = &decoded[..separator_idx];
    let password = &decoded[separator_idx + 1..];

    if username.is_empty() || password.is_empty() {
        return None;
    }

    Some(ParsedBasicAuth {
        username: username.to_string(),
        password: password.to_string(),
    })
}

fn base64_decode(input: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let bytes = STANDARD.decode(input).ok()?;
    String::from_utf8(bytes).ok()
}
