use agentics_error::ServiceError;

/// Storage-layer failures before conversion to service/API errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{0}")]
    InvalidKey(String),
    #[error("{0}")]
    SymlinkRejected(String),
    #[error("storage object already exists: {0}")]
    ObjectConflict(String),
    #[error("storage object not found: {0}")]
    ObjectNotFound(String),
    #[error("{label} exceeds storage byte limit: {actual} > {limit} bytes")]
    ObjectTooLarge {
        label: &'static str,
        actual: u64,
        limit: u64,
    },
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("storage invariant violated: {0}")]
    Internal(String),
    #[error("storage IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
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
}
