use agentics_error::ServiceError;
use agentics_storage::StorageError;

pub(crate) fn storage_error_to_service_error(error: StorageError) -> ServiceError {
    match error {
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
    }
}
