//! Shared support for Agentics operational executables.
//!
//! Operational binaries in this package are Rust-first replacements for
//! platform-owned shell automation. They use native Rust APIs where practical,
//! keep process and secret boundaries explicit, support cancellation for
//! long-running work, and document idempotence and rollback behavior next to the
//! command implementation.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use direct assertions for concise failure diagnostics"
    )
)]

pub mod check_dgx_spark_host;
pub mod check_dgx_spark_profile;
pub mod check_local_mvp;
pub mod dgx;
pub mod dgx_profile;
pub mod dgx_storage;
pub mod human_agent_docs;
pub mod large_files;
pub mod local_demo;
pub mod support;
