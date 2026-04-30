#[tokio::main]
async fn main() {
    if let Err(error) = agentics_cli::run_from_env().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
