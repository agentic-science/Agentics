//! Production database migration runner.
//!
//! This command is intentionally narrow: it reads `AGENTICS_DATABASE_URL`,
//! connects to Postgres, and applies the SQLx migrations embedded from
//! `backend/migrations`. It does not seed development data and does not start or stop
//! any services. Ctrl-C exits with the shared interrupted code; SQLx migration
//! transactions remain responsible for database-level atomicity.

use std::process::ExitCode;

use agentics_config::{Config, EnvPolicyReport, EnvServiceRole};
use agentics_persistence::pool::create_pool;

use crate::support::{ReportLine, print_reports, run_with_ctrl_c};

const PREFIX: &str = "agentics-migrate";

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../backend/migrations");

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    run_with_ctrl_c(PREFIX, async move {
        match run().await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run() -> anyhow::Result<Vec<ReportLine>> {
    let env_report = agentics_config::validate_current_env_policy(EnvServiceRole::Migrate)?;
    print_env_policy_warnings(&env_report);
    let config = Config::from_env()?;
    let pool = create_pool(&config, 5).await?;
    MIGRATOR.run(&pool).await?;
    pool.close().await;
    Ok(vec![ReportLine::pass(
        "migrate",
        "applied database migrations",
    )])
}

fn print_env_policy_warnings(report: &EnvPolicyReport) {
    for warning in &report.warnings {
        eprintln!("[{PREFIX}] WARN env {}: {}", warning.name, warning.message);
    }
}
