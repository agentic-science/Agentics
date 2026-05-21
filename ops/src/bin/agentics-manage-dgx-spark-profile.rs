//! Entrypoint for DGX Spark profile management.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::dgx_profile::run_from_process().await
}
