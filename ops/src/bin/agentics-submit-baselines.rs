//! Entrypoint for production baseline submission orchestration.

/// Run baseline submissions from process arguments and environment.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::baseline_submitter::run_from_process().await
}
