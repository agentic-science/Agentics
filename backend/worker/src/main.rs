use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = shared::config::Config::from_env()?;
    info!("starting worker");

    // TODO: initialize db pool, start evaluation polling loop

    Ok(())
}
