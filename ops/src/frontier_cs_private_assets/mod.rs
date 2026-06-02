//! Refresh Frontier-CS migrated challenge private asset overlays.
//!
//! This command rebuilds private `official-runs.zip` overlays from the synced
//! Frontier-CS upstream repository. It intentionally keeps generated ZIPs out
//! of Git and uploads them only to the persistent private-bundle backup store.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, anyhow};
use clap::Parser;
use url::Url;

use crate::support::{ReportLine, print_reports, run_with_ctrl_c};

mod backup;
mod generate;
mod validation;

use backup::{BackupEndpoint, load_env_file, upload_generated_artifact};
use generate::{GeneratedArtifact, generate_one};
use validation::validate_generated_zip;

const PREFIX: &str = "agentics-refresh-frontier-cs-private-assets";
const EXPECTED_FRONTIER_COMMIT: &str = "32fcb241";
const DEFAULT_WORKING_NOTE: &str = "working-notes/frontier-cs-upstream-refresh-2026-06-02.md";
const DEFAULT_AGENTICS_CHALLENGES_ROOT: &str = "challenge-repos/agentics-challenges";
const DEFAULT_FRONTIER_CS_ROOT: &str = "challenge-repos/Frontier-CS";
const DEFAULT_BACKUP_ENV_FILE: &str = "deploy/compose/env/rustfs-private-backup.env";
const DEFAULT_BACKUP_ENDPOINT_HOST: &str = "127.0.0.1";
const DEFAULT_BACKUP_API_PORT: u16 = 9100;
const DEFAULT_REGION: &str = "us-east-1";
const DEFAULT_MAX_OBJECT_BYTES: u64 = 1024 * 1024 * 1024;
const BACKUP_CREDENTIAL_PROVIDER_NAME: &str = "agentics-frontier-cs-private-refresh";
const PRIVATE_ZIP_NAME: &str = "official-runs.zip";
const PRIVATE_RUNS_PATH: &str = "private-benchmark/runs.json";
const PRIVATE_SESSION_PATH: &str = "private-benchmark/session.json";

/// CLI for refreshing Frontier-CS private asset overlays.
#[derive(Debug, Parser)]
#[command(
    about = "Refresh Frontier-CS migrated challenge private asset ZIPs.",
    long_about = "Generates private official-runs.zip overlays for migrated Frontier-CS algorithmic challenges, validates them against the Agentics bundle contract, and optionally uploads them to the persistent private-bundle RustFS backup store."
)]
pub struct Cli {
    /// Working note containing the private asset refresh candidate list.
    #[arg(long, default_value = DEFAULT_WORKING_NOTE)]
    working_note: PathBuf,

    /// Root of the agentics-challenges repository.
    #[arg(long, default_value = DEFAULT_AGENTICS_CHALLENGES_ROOT)]
    agentics_challenges_root: PathBuf,

    /// Root of the synced Frontier-CS upstream repository.
    #[arg(long, default_value = DEFAULT_FRONTIER_CS_ROOT)]
    frontier_cs_root: PathBuf,

    /// Required Frontier-CS commit prefix.
    #[arg(long, default_value = EXPECTED_FRONTIER_COMMIT)]
    expected_frontier_commit: String,

    /// Directory where generated private ZIPs are staged.
    #[arg(long)]
    staging_dir: Option<PathBuf>,

    /// Upload generated ZIPs to the persistent backup RustFS/S3 store.
    #[arg(long)]
    upload: bool,

    /// Refresh only one challenge. Repeatable.
    #[arg(long)]
    challenge: Vec<String>,

    /// Verify and report without writing staged ZIPs or uploading.
    #[arg(long)]
    dry_run: bool,

    /// Permit overwriting backup objects whose bytes differ from generated ZIPs.
    #[arg(long)]
    confirm_overwrite: bool,

    /// Backup RustFS env file.
    #[arg(long, default_value = DEFAULT_BACKUP_ENV_FILE)]
    backup_env_file: PathBuf,

    /// Override source backup endpoint URL.
    #[arg(long)]
    backup_endpoint_url: Option<Url>,

    /// Maximum accepted generated object size in bytes.
    #[arg(long, default_value_t = DEFAULT_MAX_OBJECT_BYTES)]
    max_object_bytes: u64,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error:#}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> anyhow::Result<Vec<ReportLine>> {
    if cli.max_object_bytes == 0 {
        anyhow::bail!("--max-object-bytes must be greater than zero");
    }
    verify_frontier_commit(&cli.frontier_cs_root, &cli.expected_frontier_commit)?;
    let mut candidates = read_refresh_candidates(&cli.working_note)?;
    if !cli.challenge.is_empty() {
        let requested = cli.challenge.iter().cloned().collect::<HashSet<_>>();
        let known = candidates.iter().cloned().collect::<HashSet<_>>();
        for challenge in &requested {
            if !known.contains(challenge) {
                anyhow::bail!(
                    "requested challenge `{challenge}` is not in the refresh candidate list"
                );
            }
        }
        candidates.retain(|challenge| requested.contains(challenge));
    }
    if candidates.is_empty() {
        anyhow::bail!("no private asset refresh candidates found");
    }

    let staging_dir = cli.staging_dir.unwrap_or_else(|| {
        PathBuf::from("target")
            .join("frontier-cs-private-assets")
            .join(format!("frontier-cs-{}", cli.expected_frontier_commit))
    });
    if !cli.dry_run {
        fs::create_dir_all(&staging_dir)
            .with_context(|| format!("failed to create staging dir {}", staging_dir.display()))?;
    }

    let backup = if cli.upload && !cli.dry_run {
        let env = load_env_file(&cli.backup_env_file)?;
        Some(
            BackupEndpoint::from_env(&env, cli.backup_endpoint_url)?
                .client()
                .await?,
        )
    } else {
        None
    };

    let mut stats = RefreshStats::default();
    let mut reports = Vec::new();
    for challenge_name in candidates {
        eprintln!("[{PREFIX}] refreshing {challenge_name}");
        let artifact = generate_one(
            &cli.agentics_challenges_root,
            &cli.frontier_cs_root,
            &challenge_name,
        )
        .with_context(|| format!("failed to generate private asset for `{challenge_name}`"))?;
        if u64::try_from(artifact.zip_bytes.len()).unwrap_or(u64::MAX) > cli.max_object_bytes {
            anyhow::bail!(
                "generated object for `{}` exceeds --max-object-bytes",
                artifact.challenge_name
            );
        }
        validate_generated_zip(
            &cli.agentics_challenges_root,
            &artifact.challenge_name,
            &artifact.zip_bytes,
            &artifact.required_paths,
        )
        .await
        .with_context(|| {
            format!(
                "failed to validate generated ZIP for `{}`",
                artifact.challenge_name
            )
        })?;

        stats.record(&artifact)?;
        if !cli.dry_run {
            let zip_path = staging_dir
                .join(&artifact.challenge_name)
                .join(PRIVATE_ZIP_NAME);
            if let Some(parent) = zip_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&zip_path, &artifact.zip_bytes)
                .with_context(|| format!("failed to write {}", zip_path.display()))?;
        }
        if let Some(backup) = &backup {
            upload_generated_artifact(backup, &artifact, cli.confirm_overwrite).await?;
        }
        reports.push(ReportLine::pass(
            &artifact.challenge_name,
            format!(
                "{} adapter, {} case(s), {} byte ZIP{}",
                artifact.adapter_label,
                artifact.case_count,
                artifact.zip_bytes.len(),
                artifact
                    .selection_note
                    .as_deref()
                    .map(|note| format!(", {note}"))
                    .unwrap_or_default()
            ),
        ));
    }

    reports.insert(
        0,
        ReportLine::pass(
            "refresh",
            format!(
                "{} challenge(s), {} case(s), {} byte(s), upload={}, dry_run={}",
                stats.challenges,
                stats.cases,
                stats.bytes,
                cli.upload && !cli.dry_run,
                cli.dry_run
            ),
        ),
    );
    Ok(reports)
}

#[derive(Default)]
struct RefreshStats {
    challenges: u64,
    cases: u64,
    bytes: u64,
}

impl RefreshStats {
    fn record(&mut self, artifact: &GeneratedArtifact) -> anyhow::Result<()> {
        self.challenges = self
            .challenges
            .checked_add(1)
            .ok_or_else(|| anyhow!("refresh challenge count overflow"))?;
        self.cases = self
            .cases
            .checked_add(u64::try_from(artifact.case_count)?)
            .ok_or_else(|| anyhow!("refresh case count overflow"))?;
        self.bytes = self
            .bytes
            .checked_add(u64::try_from(artifact.zip_bytes.len())?)
            .ok_or_else(|| anyhow!("refresh byte count overflow"))?;
        Ok(())
    }
}

fn read_refresh_candidates(path: &Path) -> anyhow::Result<Vec<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let section = content
        .split("## Private Asset Refresh Candidates")
        .nth(1)
        .ok_or_else(|| anyhow!("working note is missing private asset refresh section"))?;
    let block = section
        .split("```text")
        .nth(1)
        .and_then(|tail| tail.split("```").next())
        .ok_or_else(|| anyhow!("working note is missing private asset candidate code block"))?;
    Ok(block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn verify_frontier_commit(root: &std::path::Path, expected_prefix: &str) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .with_context(|| format!("failed to inspect Frontier-CS git repo {}", root.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to inspect Frontier-CS commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let commit = String::from_utf8(output.stdout)?.trim().to_string();
    if !commit.starts_with(expected_prefix) {
        anyhow::bail!(
            "Frontier-CS commit `{commit}` does not match expected prefix `{expected_prefix}`"
        );
    }
    Ok(())
}
