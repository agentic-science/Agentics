//! Entrypoint for the local MVP operational checker.

use std::process::ExitCode;

/// Run the local MVP checker from process arguments and environment.
#[tokio::main]
async fn main() -> ExitCode {
    agentics_ops::check_local_mvp::run_from_process().await
}
