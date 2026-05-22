use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_ops::pre_commit::run_from_process().await
}
