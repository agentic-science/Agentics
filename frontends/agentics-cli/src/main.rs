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

#[tokio::main]
async fn main() {
    if let Err(error) = agentics_cli::run_from_env().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
