//! Entrypoint for stage-aware environment policy checks.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::env_check::run_from_process().await
}
