#[tokio::main]
async fn main() -> std::process::ExitCode {
    agentics_ops::frontier_cs_private_assets::run_from_process().await
}
