//! Entrypoint for copying backed-up private challenge bundles into production storage.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::private_bundle_backups::run_from_process().await
}
