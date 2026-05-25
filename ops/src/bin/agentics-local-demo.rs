//! Entrypoint for Compose-local demo database preparation.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::local_demo::run_from_process().await
}
