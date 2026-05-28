use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_dev_checks::large_files::run_from_process().await
}
