use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Stable API-facing error code derived from transport-neutral service errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    BadRequest,
    ValidationFailed,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TooManyRequests,
    PayloadTooLarge,
    InternalError,
}

/// Optional structured validation detail for one request problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
/// Transport-neutral backend error used across shared services and workflows.
pub enum ServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("too many requests: {0}")]
    TooManyRequests(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unauthorized: {0}")]
    UnauthorizedMessage(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("docker error: {0}")]
    Docker(String),
    #[error("runner error: {0}")]
    Runner(String),
    #[error("runner capacity unavailable: {0}")]
    RunnerCapacity(String),
    #[error("base64 decode error")]
    Base64,
    #[error("{message}")]
    ValidationDetails {
        message: String,
        details: Vec<ErrorDetail>,
    },
    #[error("payload too large: {0}")]
    PayloadTooLarge(String),
}

impl ServiceErrorCode {
    /// Returns the stable snake_case string for this public error code.
    pub const fn as_str(self) -> &'static str {
        match self {
            ServiceErrorCode::BadRequest => "bad_request",
            ServiceErrorCode::ValidationFailed => "validation_failed",
            ServiceErrorCode::Unauthorized => "unauthorized",
            ServiceErrorCode::Forbidden => "forbidden",
            ServiceErrorCode::NotFound => "not_found",
            ServiceErrorCode::Conflict => "conflict",
            ServiceErrorCode::TooManyRequests => "too_many_requests",
            ServiceErrorCode::PayloadTooLarge => "payload_too_large",
            ServiceErrorCode::InternalError => "internal_error",
        }
    }
}

impl std::fmt::Display for ServiceErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ServiceError {
    /// Builds a bad request error with a public message.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// Builds a field validation error with structured details.
    pub fn validation_failed(
        message: impl Into<String>,
        details: impl Into<Vec<ErrorDetail>>,
    ) -> Self {
        Self::ValidationDetails {
            message: message.into(),
            details: details.into(),
        }
    }

    /// Builds a not found error.
    pub fn not_found() -> Self {
        Self::NotFound
    }

    /// Builds a conflict error.
    pub fn conflict() -> Self {
        Self::Conflict
    }

    /// Builds a quota/rate-limit error with a public message.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests(message.into())
    }

    /// Builds an unauthorized error with a public message.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::UnauthorizedMessage(message.into())
    }

    /// Builds an internal error whose message must be redacted at HTTP boundaries.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Returns the stable public error code.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            ServiceError::BadRequest(_) | ServiceError::Base64 | ServiceError::Zip(_) => {
                ServiceErrorCode::BadRequest
            }
            ServiceError::Validation(_) | ServiceError::ValidationDetails { .. } => {
                ServiceErrorCode::ValidationFailed
            }
            ServiceError::Unauthorized | ServiceError::UnauthorizedMessage(_) => {
                ServiceErrorCode::Unauthorized
            }
            ServiceError::Forbidden(_) => ServiceErrorCode::Forbidden,
            ServiceError::NotFound => ServiceErrorCode::NotFound,
            ServiceError::Conflict => ServiceErrorCode::Conflict,
            ServiceError::TooManyRequests(_) => ServiceErrorCode::TooManyRequests,
            ServiceError::PayloadTooLarge(_) => ServiceErrorCode::PayloadTooLarge,
            ServiceError::Database(_)
            | ServiceError::Internal(_)
            | ServiceError::Io(_)
            | ServiceError::Docker(_)
            | ServiceError::Runner(_)
            | ServiceError::RunnerCapacity(_) => ServiceErrorCode::InternalError,
        }
    }

    /// Returns the safe public message for API clients.
    pub fn public_message(&self) -> Cow<'_, str> {
        match self {
            ServiceError::BadRequest(message)
            | ServiceError::TooManyRequests(message)
            | ServiceError::Forbidden(message)
            | ServiceError::Validation(message)
            | ServiceError::PayloadTooLarge(message) => Cow::Borrowed(message),
            ServiceError::ValidationDetails { message, .. } => Cow::Borrowed(message),
            ServiceError::Unauthorized => Cow::Borrowed("unauthorized"),
            ServiceError::UnauthorizedMessage(message) => Cow::Borrowed(message),
            ServiceError::NotFound => Cow::Borrowed("not found"),
            ServiceError::Conflict => Cow::Borrowed("conflict"),
            ServiceError::Base64 => Cow::Borrowed("invalid_base64"),
            ServiceError::Zip(_) => Cow::Borrowed("invalid_zip"),
            ServiceError::Database(_)
            | ServiceError::Internal(_)
            | ServiceError::Io(_)
            | ServiceError::Docker(_)
            | ServiceError::Runner(_)
            | ServiceError::RunnerCapacity(_) => Cow::Borrowed("internal server error"),
        }
    }

    /// Returns structured validation details for API clients.
    pub fn details(&self) -> &[ErrorDetail] {
        match self {
            ServiceError::ValidationDetails { details, .. } => details,
            _ => &[],
        }
    }

    /// Returns whether this error should be logged as an internal application failure.
    pub fn is_internal(&self) -> bool {
        matches!(self.code(), ServiceErrorCode::InternalError)
    }

    /// Maps a raw SQL unique-constraint failure into the domain conflict kind.
    pub fn unique_violation_as_conflict(self) -> Self {
        match self {
            ServiceError::Database(sqlx::Error::Database(db_err))
                if db_err.is_unique_violation() =>
            {
                ServiceError::Conflict
            }
            error => error,
        }
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

#[cfg(test)]
mod tests {
    use super::{ErrorDetail, ServiceError, ServiceErrorCode};

    #[test]
    fn constructors_preserve_public_error_data() {
        let error = ServiceError::validation_failed(
            "request validation failed",
            [ErrorDetail {
                field: Some("name".to_string()),
                message: "required".to_string(),
            }],
        );

        assert_eq!(error.code(), ServiceErrorCode::ValidationFailed);
        assert_eq!(error.public_message(), "request validation failed");
        assert_eq!(error.details().len(), 1);
    }

    #[test]
    fn internal_errors_are_redacted() {
        let error = ServiceError::internal("database password leaked here");

        assert_eq!(error.code(), ServiceErrorCode::InternalError);
        assert_eq!(error.public_message(), "internal server error");
        assert!(error.details().is_empty());
    }
}
