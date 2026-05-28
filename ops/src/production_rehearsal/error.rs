//! Error type for production rehearsal operations.

use reqwest::StatusCode;
use thiserror::Error;

/// Production rehearsal error.
#[derive(Debug, Error)]
pub enum ProductionRehearsalError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid {field} `{value}`: {source}")]
    InvalidUrl {
        field: &'static str,
        value: String,
        source: url::ParseError,
    },
    #[error("HTTP client error: {0}")]
    HttpClient(reqwest::Error),
    #[error("HTTP request failed with {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },
    #[error("invalid HTTP JSON response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("configuration error: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("env file error: {0}")]
    Dotenv(#[from] dotenvy::Error),
    #[error("storage error: {0}")]
    Storage(#[from] agentics_storage::StorageError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("service error: {0}")]
    Service(#[from] agentics_error::ServiceError),
}
