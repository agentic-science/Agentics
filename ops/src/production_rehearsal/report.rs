//! Production rehearsal report model and writers.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::fs;

use super::ProductionRehearsalError;

/// Stable status for one rehearsal phase or check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RehearsalStatus {
    Passed,
    Failed,
    Skipped,
}

impl RehearsalStatus {
    /// Label used in the human report.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASS",
            Self::Failed => "FAIL",
            Self::Skipped => "SKIP",
        }
    }
}

/// One granular rehearsal check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CheckEvidence {
    pub(super) name: String,
    pub(super) status: RehearsalStatus,
    pub(super) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) evidence_path: Option<PathBuf>,
    pub(super) required: bool,
}

impl CheckEvidence {
    /// Passed check.
    pub(super) fn passed(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: RehearsalStatus::Passed,
            message: message.into(),
            evidence_path: None,
            required: true,
        }
    }

    /// Failed check.
    pub(super) fn failed(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: RehearsalStatus::Failed,
            message: message.into(),
            evidence_path: None,
            required: true,
        }
    }

    /// Skipped check.
    pub(super) fn skipped(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: RehearsalStatus::Skipped,
            message: message.into(),
            evidence_path: None,
            required: false,
        }
    }
}

/// One coarse rehearsal phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PhaseEvidence {
    pub(super) name: String,
    pub(super) status: RehearsalStatus,
    pub(super) duration_ms: u128,
    pub(super) checks: Vec<CheckEvidence>,
}

impl PhaseEvidence {
    /// Build a phase result from child checks.
    pub(super) fn from_checks(
        name: impl Into<String>,
        duration: Duration,
        checks: Vec<CheckEvidence>,
    ) -> Self {
        let has_required_failure = checks
            .iter()
            .any(|check| check.required && check.status == RehearsalStatus::Failed);
        let status = if has_required_failure {
            RehearsalStatus::Failed
        } else if checks
            .iter()
            .all(|check| check.status == RehearsalStatus::Skipped)
        {
            RehearsalStatus::Skipped
        } else {
            RehearsalStatus::Passed
        };
        Self {
            name: name.into(),
            status,
            duration_ms: duration.as_millis(),
            checks,
        }
    }
}

/// Published fixture metadata used by API and browser phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RehearsalChallengeEvidence {
    pub(super) name: String,
    pub(super) title: String,
    pub(super) mode: String,
    pub(super) target: String,
}

/// Solution submission ids created during rehearsal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct RehearsalSubmissionEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) separated_validation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) separated_official_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) piped_stdio_validation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) piped_stdio_official_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) coexecuted_validation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) coexecuted_official_id: Option<String>,
}

/// Browser-check manifest written separately so Playwright never needs secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserRehearsalManifest {
    run_id: String,
    web_base_url: String,
    challenges: Vec<RehearsalChallengeEvidence>,
    submissions: RehearsalSubmissionEvidence,
}

/// Full production rehearsal report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RehearsalReport {
    pub(super) run_id: String,
    pub(super) started_at_unix: u64,
    pub(super) completed_at_unix: Option<u64>,
    pub(super) api_base_url: String,
    pub(super) web_base_url: Option<String>,
    pub(super) disposable_staging_confirmed: bool,
    pub(super) gpu_mode: String,
    pub(super) output_dir: PathBuf,
    pub(super) phases: Vec<PhaseEvidence>,
    pub(super) challenges: Vec<RehearsalChallengeEvidence>,
    pub(super) submissions: RehearsalSubmissionEvidence,
}

impl RehearsalReport {
    /// Create an empty report.
    pub(super) fn new(
        run_id: String,
        api_base_url: String,
        web_base_url: Option<String>,
        disposable_staging_confirmed: bool,
        gpu_mode: String,
        output_dir: PathBuf,
    ) -> Self {
        Self {
            run_id,
            started_at_unix: unix_now(),
            completed_at_unix: None,
            api_base_url,
            web_base_url,
            disposable_staging_confirmed,
            gpu_mode,
            output_dir,
            phases: Vec::new(),
            challenges: Vec::new(),
            submissions: RehearsalSubmissionEvidence::default(),
        }
    }

    /// Return whether any required check failed.
    pub(super) fn failed(&self) -> bool {
        self.phases
            .iter()
            .any(|phase| phase.status == RehearsalStatus::Failed)
    }

    /// Mark completion time.
    pub(super) fn finish(&mut self) {
        self.completed_at_unix = Some(unix_now());
    }

    /// Return the path to the JSON report.
    pub(super) fn json_path(&self) -> PathBuf {
        self.output_dir.join("report.json")
    }

    /// Return the path to the Markdown report.
    pub(super) fn markdown_path(&self) -> PathBuf {
        self.output_dir.join("report.md")
    }

    /// Return the browser manifest path.
    pub(super) fn browser_manifest_path(&self) -> PathBuf {
        self.output_dir.join("browser-manifest.json")
    }

    /// Write JSON, Markdown, and browser manifest evidence.
    pub(super) async fn write(&self) -> Result<(), ProductionRehearsalError> {
        fs::create_dir_all(&self.output_dir).await?;
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(self.json_path(), json).await?;
        fs::write(self.markdown_path(), self.markdown()).await?;
        self.write_browser_manifest().await?;
        Ok(())
    }

    /// Write only the browser manifest used by the Playwright phase.
    pub(super) async fn write_browser_manifest(&self) -> Result<(), ProductionRehearsalError> {
        fs::create_dir_all(&self.output_dir).await?;
        fs::write(
            self.browser_manifest_path(),
            serde_json::to_vec_pretty(&BrowserRehearsalManifest {
                run_id: self.run_id.clone(),
                web_base_url: self.web_base_url.clone().unwrap_or_default(),
                challenges: self.challenges.clone(),
                submissions: self.submissions.clone(),
            })?,
        )
        .await?;
        Ok(())
    }

    fn markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# Agentics Production Rehearsal `{}`\n\n",
            self.run_id
        ));
        out.push_str(&format!("- API: `{}`\n", self.api_base_url));
        if let Some(web) = &self.web_base_url {
            out.push_str(&format!("- Web: `{web}`\n"));
        }
        out.push_str(&format!("- GPU mode: `{}`\n", self.gpu_mode));
        out.push_str(&format!(
            "- Disposable staging confirmed: `{}`\n\n",
            self.disposable_staging_confirmed
        ));
        out.push_str("## Phases\n\n");
        for phase in &self.phases {
            out.push_str(&format!(
                "### {} `{}` ({} ms)\n\n",
                phase.status.label(),
                phase.name,
                phase.duration_ms
            ));
            for check in &phase.checks {
                out.push_str(&format!(
                    "- {} `{}`: {}\n",
                    check.status.label(),
                    check.name,
                    check.message
                ));
                if let Some(path) = &check.evidence_path {
                    out.push_str(&format!("  Evidence: `{}`\n", path.display()));
                }
            }
            out.push('\n');
        }
        if !self.challenges.is_empty() {
            out.push_str("## Rehearsal Challenges\n\n");
            for challenge in &self.challenges {
                out.push_str(&format!(
                    "- `{}`: {} on `{}` using `{}`\n",
                    challenge.name, challenge.title, challenge.target, challenge.mode
                ));
            }
            out.push('\n');
        }
        out
    }
}

/// Return the current Unix timestamp.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Convert a path to a display string for evidence messages.
fn display_path(path: &Path) -> String {
    path.display().to_string()
}

/// Print a compact report summary to stderr.
pub(super) fn print_report_summary(prefix: &str, report: &RehearsalReport) {
    for phase in &report.phases {
        eprintln!(
            "[{prefix}] {} {} ({} ms)",
            phase.status.label(),
            phase.name,
            phase.duration_ms
        );
        for check in &phase.checks {
            eprintln!(
                "[{prefix}]   {} {}: {}",
                check.status.label(),
                check.name,
                check.message
            );
        }
    }
    eprintln!("[{prefix}] report: {}", display_path(&report.json_path()));
}
