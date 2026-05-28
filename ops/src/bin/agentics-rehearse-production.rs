//! Entrypoint for production rehearsal checks.

/// Run production rehearsal checks from process arguments and environment.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::production_rehearsal::run_from_process().await
}
