//! Process-local throttling for failed administrator authentication attempts.

use std::net::SocketAddr;
use std::num::NonZeroU32;

use axum::extract::ConnectInfo;
use axum::http::request::Parts;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};

use shared::error::{AppError, Result};

const ADMIN_AUTH_FAILURES_PER_MINUTE: u32 = 5;

/// In-memory limiter shared by admin session login and Basic auth failures.
pub struct AdminAuthThrottle {
    limiter: DefaultKeyedRateLimiter<String>,
}

impl std::fmt::Debug for AdminAuthThrottle {
    /// Formats the throttle without exposing implementation internals.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminAuthThrottle").finish_non_exhaustive()
    }
}

impl AdminAuthThrottle {
    /// Create a limiter with the MVP failed-attempt policy.
    pub fn new() -> Result<Self> {
        let attempts = NonZeroU32::new(ADMIN_AUTH_FAILURES_PER_MINUTE).ok_or_else(|| {
            AppError::Internal("admin auth throttle limit must be non-zero".to_string())
        })?;
        Ok(Self {
            limiter: RateLimiter::keyed(Quota::per_minute(attempts)),
        })
    }

    /// Record a failed attempt and reject callers over the configured threshold.
    pub fn record_failed_attempt(&self, username: &str, remote_addr: &str) -> Result<()> {
        let key = throttle_key(username, remote_addr);
        self.limiter.check_key(&key).map_err(|_| {
            AppError::TooManyRequests(
                "too many failed admin authentication attempts; try again later".to_string(),
            )
        })
    }
}

/// Return the remote socket address carried by Axum connect-info.
pub fn remote_addr_from_parts(parts: &Parts) -> String {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Build a stable limiter key from the attempted username and remote address.
pub fn throttle_key(username: &str, remote_addr: &str) -> String {
    let username = username.trim();
    let username = if username.is_empty() {
        "<empty>"
    } else {
        username
    };
    format!("{username}@{remote_addr}")
}
