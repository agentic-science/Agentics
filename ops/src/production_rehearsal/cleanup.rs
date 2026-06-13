//! Cleanup and fixture archival phase for production rehearsal.

use std::time::Instant;

use agentics_config::Config;
use agentics_domain::models::names::ChallengeName;
use agentics_persistence::Repositories;
use reqwest::Client;
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;

use super::http::admin_post_json;
use super::report::{CheckEvidence, PhaseEvidence, RehearsalReport};
use super::{ProductionRehearsalError, RehearsalState, RunArgs, runtime};

pub(super) async fn run_cleanup_phase(
    client: &Client,
    resolved: &runtime::ResolvedRunConfig,
    args: &RunArgs,
    report: &RehearsalReport,
    state: &RehearsalState,
) -> PhaseEvidence {
    let start = Instant::now();
    if args.keep_artifacts {
        return PhaseEvidence::from_checks(
            "cleanup",
            start.elapsed(),
            vec![CheckEvidence::skipped(
                "fixture archival",
                "--keep-artifacts was supplied",
            )],
        );
    }
    let mut checks = Vec::new();
    if let Some(code_id) = state.pioneer_code_id.as_deref() {
        checks.push(
            match admin_post_json(
                client,
                &resolved.api_base_url,
                &format!("admin/pioneer-codes/{code_id}/revoke"),
                &resolved.admin_service_token,
                &serde_json::json!({}),
            )
            .await
            {
                Ok(_) => CheckEvidence::passed(
                    "revoke pioneer code",
                    "revoked rehearsal registration code and dependent credentials",
                ),
                Err(error) => CheckEvidence::failed("revoke pioneer code", error.to_string()),
            },
        );
    } else {
        checks.push(CheckEvidence::skipped(
            "revoke pioneer code",
            "identity phase did not create a pioneer code id",
        ));
    }

    let challenge_names = report
        .challenges
        .iter()
        .map(|challenge| challenge.name.clone())
        .collect::<Vec<_>>();
    checks.push(
        match archive_rehearsal_challenges(&resolved.config, &challenge_names).await {
            Ok(count) => CheckEvidence::passed(
                "fixture archival",
                format!("archived {count} rehearsal challenge fixture(s)"),
            ),
            Err(error) => CheckEvidence::failed("fixture archival", error.to_string()),
        },
    );
    PhaseEvidence::from_checks("cleanup", start.elapsed(), checks)
}

async fn archive_rehearsal_challenges(
    config: &Config,
    challenge_names: &[String],
) -> Result<u64, ProductionRehearsalError> {
    if challenge_names.is_empty() {
        return Ok(0);
    }
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(config.database.url.expose_secret())
        .await?;
    let repos = Repositories::new(&pool);
    let mut archived = 0u64;
    for name in challenge_names {
        let challenge_name = ChallengeName::try_new(name.clone()).map_err(|error| {
            ProductionRehearsalError::InvalidResponse(format!(
                "generated invalid challenge name `{name}`: {error}"
            ))
        })?;
        repos.challenges().archive(&challenge_name).await?;
        archived = archived.checked_add(1).ok_or_else(|| {
            ProductionRehearsalError::InvalidResponse("archive count overflow".to_string())
        })?;
    }
    Ok(archived)
}
