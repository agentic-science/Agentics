use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_ops::agents_claude_links::run_from_process().await
}
