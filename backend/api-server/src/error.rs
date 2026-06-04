use agentics_domain::models::{ErrorBody, ErrorResponse};
use agentics_error::{ServiceError, ServiceErrorCode};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

#[derive(Debug)]
/// API-server error wrapper that maps transport-neutral service errors to HTTP.
pub struct ApiError(ServiceError);

pub type ApiResult<T> = std::result::Result<T, ApiError>;

impl ApiError {
    /// Builds an API error from a service error.
    pub fn new(error: ServiceError) -> Self {
        Self(error)
    }

    /// Returns the HTTP status associated with the wrapped service error.
    pub fn status(&self) -> StatusCode {
        match self.0.code() {
            ServiceErrorCode::BadRequest => StatusCode::BAD_REQUEST,
            ServiceErrorCode::ValidationFailed => StatusCode::UNPROCESSABLE_ENTITY,
            ServiceErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ServiceErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ServiceErrorCode::NotFound => StatusCode::NOT_FOUND,
            ServiceErrorCode::Conflict => StatusCode::CONFLICT,
            ServiceErrorCode::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            ServiceErrorCode::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ServiceErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Borrows the wrapped service error for focused tests and logging adapters.
    pub fn as_service_error(&self) -> &ServiceError {
        &self.0
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self::new(error)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::new(ServiceError::from(error))
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self::new(ServiceError::from(error))
    }
}

impl From<zip::result::ZipError> for ApiError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::new(ServiceError::from(error))
    }
}

impl From<agentics_storage::StorageError> for ApiError {
    fn from(error: agentics_storage::StorageError) -> Self {
        use agentics_storage::StorageError;

        let error = match error {
            StorageError::InvalidKey(message) | StorageError::SymlinkRejected(message) => {
                ServiceError::BadRequest(message)
            }
            StorageError::ObjectTooLarge { .. } => ServiceError::BadRequest(error.to_string()),
            StorageError::ObjectConflict(_) => ServiceError::Conflict,
            StorageError::ObjectNotFound(_) => ServiceError::NotFound,
            StorageError::Internal(message) | StorageError::Backend(message) => {
                ServiceError::Internal(message)
            }
            StorageError::Io(error) => ServiceError::Io(error),
        };
        Self::new(error)
    }
}

impl From<agentics_domain::storage::StorageKeyError> for ApiError {
    fn from(error: agentics_domain::storage::StorageKeyError) -> Self {
        Self::new(ServiceError::from(error))
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    /// Converts a service error into the public API error envelope.
    fn into_response(self) -> Response {
        if self.0.is_internal() {
            error!(error = %self.0, "internal application error");
        }

        let status = self.status();
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.0.code(),
                message: self.0.public_message().into_owned(),
                details: self.0.details().to_vec(),
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use agentics_error::{ServiceError, ServiceErrorCode};
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use serde_json::Value;

    use super::ApiError;

    #[tokio::test]
    async fn maps_bad_request_to_nested_error_envelope() {
        let response = ApiError::from(ServiceError::bad_request("bad input")).into_response();
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = response_body(response).await;
        assert_eq!(body["error"]["code"], "bad_request");
        assert_eq!(body["error"]["message"], "bad input");
        assert!(body["error"].get("details").is_none());
    }

    #[tokio::test]
    async fn internal_errors_are_redacted_in_http_responses() {
        let response =
            ApiError::from(ServiceError::internal("database password leaked here")).into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let body = response_body(response).await;
        assert_eq!(body["error"]["code"], "internal_error");
        assert_eq!(body["error"]["message"], "internal server error");
        assert!(!body.to_string().contains("database password"));
    }

    #[test]
    fn maps_every_service_error_code_to_status() {
        let cases = [
            (
                ServiceErrorCode::BadRequest,
                axum::http::StatusCode::BAD_REQUEST,
            ),
            (
                ServiceErrorCode::Unauthorized,
                axum::http::StatusCode::UNAUTHORIZED,
            ),
            (
                ServiceErrorCode::Forbidden,
                axum::http::StatusCode::FORBIDDEN,
            ),
            (
                ServiceErrorCode::NotFound,
                axum::http::StatusCode::NOT_FOUND,
            ),
            (ServiceErrorCode::Conflict, axum::http::StatusCode::CONFLICT),
            (
                ServiceErrorCode::TooManyRequests,
                axum::http::StatusCode::TOO_MANY_REQUESTS,
            ),
            (
                ServiceErrorCode::PayloadTooLarge,
                axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            ),
            (
                ServiceErrorCode::InternalError,
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (code, expected) in cases {
            let error = match code {
                ServiceErrorCode::BadRequest => ServiceError::bad_request("bad input"),
                ServiceErrorCode::ValidationFailed => {
                    ServiceError::validation_failed("request validation failed", [])
                }
                ServiceErrorCode::Unauthorized => ServiceError::Unauthorized,
                ServiceErrorCode::Forbidden => ServiceError::Forbidden("forbidden".to_string()),
                ServiceErrorCode::NotFound => ServiceError::not_found(),
                ServiceErrorCode::Conflict => ServiceError::conflict(),
                ServiceErrorCode::TooManyRequests => ServiceError::too_many_requests("try later"),
                ServiceErrorCode::PayloadTooLarge => {
                    ServiceError::PayloadTooLarge("too large".to_string())
                }
                ServiceErrorCode::InternalError => ServiceError::internal("boom"),
            };
            assert_eq!(ApiError::from(error).status(), expected);
        }
    }

    async fn response_body(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&body).expect("response body should be json")
    }
}
