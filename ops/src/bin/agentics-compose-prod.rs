//! Entrypoint for production Docker Compose operations.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::compose_prod::run_from_process().await
}
