//! Entrypoint for DGX Spark host inventory checks.

/// Run DGX Spark host inventory checks from process arguments and environment.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::check_dgx_spark_host::run_from_process().await
}
