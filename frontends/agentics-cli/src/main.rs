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

#[tokio::main]
/// Handles main for this module.
async fn main() {
    if let Err(error) = agentics_cli::run_from_env().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
