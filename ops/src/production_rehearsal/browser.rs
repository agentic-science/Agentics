//! Browser-facing checks for production rehearsal runs.

use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command;

use super::RunArgs;
use super::report::{CheckEvidence, PhaseEvidence, RehearsalReport};

/// Run Playwright against the public observer surfaces using a secret-free manifest.
pub(super) async fn run_browser_phase(args: &RunArgs, report: &RehearsalReport) -> PhaseEvidence {
    let start = Instant::now();
    if args.no_browser {
        return PhaseEvidence::from_checks(
            "browser",
            start.elapsed(),
            vec![CheckEvidence::skipped(
                "Playwright",
                "--no-browser was supplied",
            )],
        );
    }
    if report.web_base_url.is_none() {
        return PhaseEvidence::from_checks(
            "browser",
            start.elapsed(),
            vec![CheckEvidence::skipped(
                "Playwright",
                "web base URL was not provided",
            )],
        );
    }

    let manifest_path = report.browser_manifest_path();
    if let Err(error) = report.write_browser_manifest().await {
        return PhaseEvidence::from_checks(
            "browser",
            start.elapsed(),
            vec![CheckEvidence::failed(
                "Playwright manifest",
                error.to_string(),
            )],
        );
    }
    let manifest_env_path = match std::fs::canonicalize(&manifest_path) {
        Ok(path) => path,
        Err(error) => {
            return PhaseEvidence::from_checks(
                "browser",
                start.elapsed(),
                vec![CheckEvidence::failed(
                    "Playwright manifest",
                    format!("failed to resolve manifest path: {error}"),
                )],
            );
        }
    };
    let mut command = Command::new("bun");
    command
        .arg("run")
        .arg("rehearse:e2e")
        .current_dir("frontends/web")
        .env("AGENTICS_REHEARSAL_MANIFEST", &manifest_env_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let check = match command.output().await {
        Ok(output) if output.status.success() => {
            CheckEvidence::passed("Playwright", "browser rehearsal checks passed")
        }
        Ok(output) => CheckEvidence::failed(
            "Playwright",
            format!(
                "browser checks failed with status {}; stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ),
        Err(error) => CheckEvidence::failed("Playwright", error.to_string()),
    };
    PhaseEvidence::from_checks("browser", start.elapsed(), vec![check])
}
