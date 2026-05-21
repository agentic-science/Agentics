//! Entrypoint for DGX Spark hosted profile checks.

/// Run DGX Spark hosted profile checks from process arguments and environment.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::check_dgx_spark_profile::run_from_process().await
}
