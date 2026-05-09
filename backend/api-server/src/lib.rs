#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod auth_handlers;
pub mod challenge_creation_handlers;
pub mod extractors;
pub mod handlers;
pub mod presenters;
pub mod router;
pub mod state;
