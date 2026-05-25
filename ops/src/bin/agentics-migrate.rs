//! Entrypoint for production database migrations.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::migrate::run_from_process().await
}
