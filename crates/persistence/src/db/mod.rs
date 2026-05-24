//! Database access modules shared by the API server, worker, and tests.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "SQL primitives are intentionally private behind repository facades; some helpers remain reserved for narrowly scoped repository methods"
)]

pub mod agents;
pub mod challenge_creation;
pub mod challenges;
pub mod evaluation_jobs;
pub(crate) mod evaluation_policy;
pub mod evaluations;
mod ids;
mod json;
pub mod leaderboard;
pub mod maintenance;
pub mod pioneer_codes;
pub mod pool;
pub mod sessions;
pub mod solution_submissions;
pub mod validation_quotas;

use agentics_domain::error::ServiceError;

/// Local database workflow failures before conversion to service errors.
#[derive(Debug, thiserror::Error)]
pub enum DbWorkflowError {
    #[error("admission conflict: {0}")]
    AdmissionConflict(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("quota exhausted: {0}")]
    QuotaExhausted(String),
    #[error("invalid stored data: {0}")]
    InvalidStoredData(String),
    #[error("raw SQL failure: {0}")]
    Sql(#[from] sqlx::Error),
}

impl From<DbWorkflowError> for ServiceError {
    fn from(error: DbWorkflowError) -> Self {
        match error {
            DbWorkflowError::AdmissionConflict(_) => ServiceError::Conflict,
            DbWorkflowError::NotFound(_) => ServiceError::NotFound,
            DbWorkflowError::QuotaExhausted(message) => ServiceError::TooManyRequests(message),
            DbWorkflowError::InvalidStoredData(message) => ServiceError::Internal(message),
            DbWorkflowError::Sql(error) => ServiceError::Database(error),
        }
    }
}
