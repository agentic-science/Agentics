use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::error;

#[derive(Debug, thiserror::Error)]
/// Enumerates app error variants supported by this module.
pub enum AppError {
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
}

impl IntoResponse for AppError {
    /// Handles into response for this module.
    fn into_response(self) -> Response {
        let (status, error, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::Conflict => (StatusCode::CONFLICT, "conflict", self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::TooManyRequests(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                msg.clone(),
            ),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg.clone()),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::Base64 => (
                StatusCode::BAD_REQUEST,
                "bad_request",
                "invalid_base64".to_string(),
            ),
            AppError::Zip(_) => (
                StatusCode::BAD_REQUEST,
                "bad_request",
                "invalid_zip".to_string(),
            ),
            AppError::Database(_)
            | AppError::Internal(_)
            | AppError::Io(_)
            | AppError::Docker(_)
            | AppError::Runner(_)
            | AppError::RunnerCapacity(_) => {
                error!(error = %self, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({ "error": error, "message": message }));
        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    use super::AppError;

    /// Verifies that internal errors are redacted in http responses.
    #[tokio::test]
    async fn internal_errors_are_redacted_in_http_responses() {
        let response =
            AppError::Internal("database password leaked here".to_string()).into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("response body should be utf8");

        assert!(body.contains("internal server error"));
        assert!(!body.contains("database password"));
    }
}
