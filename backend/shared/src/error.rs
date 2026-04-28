use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
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
    #[error("base64 decode error")]
    Base64,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::Conflict => (StatusCode::CONFLICT, "conflict", self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
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
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                self.to_string(),
            ),
        };

        let body = Json(json!({ "error": error, "message": message }));
        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
