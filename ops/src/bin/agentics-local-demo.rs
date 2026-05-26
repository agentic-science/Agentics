//! Entrypoint for Compose-local development database preparation.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::local_demo::run_from_process().await
}
