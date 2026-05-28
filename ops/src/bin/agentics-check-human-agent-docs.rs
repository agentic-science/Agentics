use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_dev_checks::human_agent_docs::run_from_process().await
}
