#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::enum_glob_use,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::wildcard_imports,
        reason = "unit tests use direct assertions and fixture indexing for concise failure diagnostics"
    )
)]

pub mod admin;
pub mod auth;
pub mod challenge_metadata;
pub mod challenge_review_records;
pub mod creator;
pub mod evaluation_lifecycle;
pub mod maintenance;
pub mod public_projection;
pub mod solution_submissions;

mod storage_errors;

pub use agentics_error as error;
