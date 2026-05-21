use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_ops::large_files::run_from_process().await
}
