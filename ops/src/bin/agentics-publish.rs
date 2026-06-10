//! Entrypoint for Agentics release publishing.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::publish::run_from_process().await
}
