//! Entrypoint for isolated DGX Spark test storage preparation.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::dgx_storage::run_prepare_test_from_process().await
}
