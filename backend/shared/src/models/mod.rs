//! Shared API model types used by backend crates and mirrored by the frontend schemas.
//!
//! Response DTO policy: optional response fields should serialize absent values
//! by using `#[serde(skip_serializing_if = "Option::is_none")]`. The public JSON
//! contract avoids explicit `null` fields so generated frontend schemas can map
//! these fields to `field?: T`. Use explicit `null` only when the API semantics
//! require distinguishing "present but empty" from "not present", and document
//! that exception next to the field.

pub mod auth;
pub mod challenge;
pub mod challenge_creation;
pub mod evaluation;
pub mod request;

#[cfg(test)]
mod contract_tests;

use serde::{Deserialize, Serialize};

/// Standard error response shape used by all API extractors and handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Health-check response returned by the API server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub environment: String,
    pub database: DatabaseHealth,
}

/// Database portion of the health-check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHealth {
    pub connected: bool,
    pub current_time: String,
}

/// Generic response for endpoints that only need to return an id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdOnlyResponse {
    pub id: String,
}

/// Current published version summary embedded in challenge DTOs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentVersionDto {
    pub id: String,
    pub version: String,
}
