use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    agentics_ops::human_agent_docs::run_from_process().await
}
