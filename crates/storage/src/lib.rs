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

//! Durable object storage for submissions, private assets, logs, and challenge bundles.
//!
//! A storage key is an opaque object locator inside the configured storage
//! backend. Local development maps it onto a filesystem path, while hosted
//! deployments may map it to an S3 object key. Runner writable storage is a
//! separate local filesystem concern and is not represented by this crate.

pub use agentics_domain::storage::{StorageKey, StorageKeyError};

mod backend;
mod error;
mod factory;
mod fs_utils;
mod intent;
mod local;
mod s3;
mod tar_archive;

pub use backend::Storage;
pub use error::{Result, StorageError};
pub use factory::{StorageFactoryOptions, build_storage, storage_work_root};
pub use fs_utils::ensure_private_directory;
pub use intent::StorageWriteIntent;
pub use local::{LocalStorage, LocalStorageOptions};
pub use s3::{S3Storage, S3StorageOptions};
pub use tar_archive::{pack_directory_to_tar, unpack_tar_to_directory};

#[cfg(test)]
mod tests;
