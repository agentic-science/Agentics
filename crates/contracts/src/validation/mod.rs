//! Shared validation helpers for external Agentics contracts.
//!
//! These modules intentionally cover reusable boundary validation only. Durable
//! admission controls, authorization checks, and state-machine transitions stay
//! with the database and API modules that own those invariants.

pub mod archive;
pub mod github;
pub mod public_api;
pub mod schemas;
pub mod targets;
pub mod text;
