//! Reusable development-time checks for Agentics tooling.
//!
//! This crate intentionally stays small and independent from product crates so
//! local hooks can run even when unrelated platform code is mid-edit.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use direct assertions for concise failure diagnostics"
    )
)]

pub mod human_agent_docs;
pub mod large_files;
pub mod support;
