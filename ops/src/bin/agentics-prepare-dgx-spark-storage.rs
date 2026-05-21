//! Entrypoint for DGX Spark production storage preparation.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::dgx_storage::run_prepare_from_process().await
}
