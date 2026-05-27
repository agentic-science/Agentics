//! Database pool creation and health checks.

use secrecy::ExposeSecret;
use sqlx::PgPool;

use agentics_config::Config;
use agentics_error::Result;

/// Create a Postgres connection pool from application configuration.
pub async fn create_pool(config: &Config, max_connections: u32) -> Result<PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(config.database.url.expose_secret())
        .await?;
    Ok(pool)
}

/// Check database connectivity and return the server clock.
pub async fn check_database(pool: &PgPool) -> Result<agentics_domain::models::DatabaseHealth> {
    let row: (String,) = sqlx::query_as("SELECT NOW()::text").fetch_one(pool).await?;

    Ok(agentics_domain::models::DatabaseHealth {
        connected: true,
        current_time: row.0,
    })
}
