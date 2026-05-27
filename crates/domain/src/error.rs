pub use agentics_error::{ErrorDetail, Result, ServiceError, ServiceErrorCode};

impl From<crate::storage::StorageKeyError> for ServiceError {
    fn from(error: crate::storage::StorageKeyError) -> Self {
        ServiceError::BadRequest(error.to_string())
    }
}
