pub mod evaluation;
pub mod problem;
pub mod request;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub environment: String,
    pub database: DatabaseHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHealth {
    pub connected: bool,
    pub current_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdOnlyResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentVersionDto {
    pub id: String,
    pub version: String,
}
