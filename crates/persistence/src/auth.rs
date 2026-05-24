//! Token hashing helpers used by repository authentication queries.

use sha2::{Digest, Sha256};

/// Hash an opaque token before storing or comparing it.
pub(crate) fn hash_opaque_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Hash an agent bearer token before comparing it.
pub(crate) fn hash_agent_token(token: &str) -> String {
    hash_opaque_token(token)
}
