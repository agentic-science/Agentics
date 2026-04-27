use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = shared::config::Config::from_env()?;
    info!("starting api server on {}:{}", config.api_host, config.api_port);

    // TODO: initialize db pool, create axum router, start server

    Ok(())
}
