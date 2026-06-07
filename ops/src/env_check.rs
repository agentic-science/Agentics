//! Stage-aware environment policy checker.

use std::process::ExitCode;

use agentics_config::{EnvPolicyReport, EnvServiceRole};
use clap::Parser;

use crate::support::{ReportLine, print_reports, run_with_ctrl_c};

const PREFIX: &str = "agentics-env-check";

#[derive(Debug, Parser)]
#[command(
    about = "Validate Agentics stage environment variables.",
    long_about = "Checks that required env vars for a stage and service role are set, optional env vars have documented defaults, and removed env vars are not still present."
)]
pub struct Cli {
    /// Service role to validate: compose, api, worker, migrate, web, local-dev, or test-harness.
    #[arg(long)]
    role: EnvServiceRole,
}

pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli) {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

fn run(cli: Cli) -> anyhow::Result<Vec<ReportLine>> {
    let report = agentics_config::validate_current_env_policy(cli.role)?;
    print_env_policy_warnings(&report);
    Ok(vec![ReportLine::pass(
        "env policy",
        format!("validated {} environment for {}", report.stage, report.role),
    )])
}

fn print_env_policy_warnings(report: &EnvPolicyReport) {
    for warning in &report.warnings {
        eprintln!("[{PREFIX}] WARN env {}: {}", warning.name, warning.message);
    }
}
