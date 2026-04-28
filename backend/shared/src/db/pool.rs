//! Database pool creation and health checks.

use sqlx::PgPool;

use crate::config::Config;
use crate::error::Result;

/// Create a Postgres connection pool from application configuration.
pub async fn create_pool(config: &Config, max_connections: u32) -> Result<PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&config.database_url)
        .await?;
    Ok(pool)
}

/// Check database connectivity and return the server clock.
pub async fn check_database(pool: &PgPool) -> Result<crate::models::DatabaseHealth> {
    let row: (String,) = sqlx::query_as("SELECT NOW()::text").fetch_one(pool).await?;

    Ok(crate::models::DatabaseHealth {
        connected: true,
        current_time: row.0,
    })
}
