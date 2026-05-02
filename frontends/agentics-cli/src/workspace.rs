use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use shared::models::challenge::ChallengeDetailResponse;

const PRE_COMMIT_HOOK: &str = r#"#!/usr/bin/env bash
set -euo pipefail

if [ ! -f run.sh ]; then
  echo "pre-commit: run.sh must exist at the repository root." >&2
  exit 1
fi
"#;

#[derive(Debug, Clone, Serialize)]
pub struct InitSolutionSummary {
    pub workspace_dir: PathBuf,
    pub challenge_id: String,
    pub challenge_title: String,
    pub challenge_version: String,
}

pub fn init_solution_workspace(
    challenge: &ChallengeDetailResponse,
    dir: Option<PathBuf>,
) -> Result<InitSolutionSummary> {
    let workspace_dir = dir.unwrap_or_else(|| default_workspace_dir(&challenge.id));
    if workspace_dir.exists() {
        bail!(
            "solution workspace already exists: {}",
            workspace_dir.display()
        );
    }

    if let Some(parent) = workspace_dir.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    fs::create_dir(&workspace_dir)
        .with_context(|| format!("failed to create workspace {}", workspace_dir.display()))?;

    let result = write_workspace_files(challenge, &workspace_dir)
        .and_then(|_| initialize_git_repository(&workspace_dir))
        .and_then(|_| install_pre_commit_hook(&workspace_dir));

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&workspace_dir);
        return Err(error);
    }

    let workspace_dir = fs::canonicalize(&workspace_dir)
        .with_context(|| format!("failed to resolve workspace {}", workspace_dir.display()))?;

    Ok(InitSolutionSummary {
        workspace_dir,
        challenge_id: challenge.id.clone(),
        challenge_title: challenge.title.clone(),
        challenge_version: challenge.current_version.version.clone(),
    })
}

fn write_workspace_files(challenge: &ChallengeDetailResponse, workspace_dir: &Path) -> Result<()> {
    let readme_path = workspace_dir.join("README.md");
    fs::write(readme_path, render_readme(challenge)).with_context(|| {
        format!(
            "failed to write README.md in workspace {}",
            workspace_dir.display()
        )
    })
}

fn render_readme(challenge: &ChallengeDetailResponse) -> String {
    format!(
        "# {}\n\nChallenge: `{}`\nVersion: `{}` (`{}`)\n\n{}\n\n## Workspace Contract\n\nCreate a `run.sh` file at the repository root before committing. The generated pre-commit hook checks that this file exists.\n",
        challenge.title.trim(),
        challenge.id,
        challenge.current_version.version,
        challenge.current_version.id,
        challenge.statement_markdown.trim()
    )
}

fn initialize_git_repository(workspace_dir: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("init")
        .arg(workspace_dir)
        .output()
        .context("failed to execute `git init`; is git installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to initialize git repository in {}: {}",
            workspace_dir.display(),
            stderr.trim()
        );
    }

    Ok(())
}

fn install_pre_commit_hook(workspace_dir: &Path) -> Result<()> {
    let hook_path = workspace_dir.join(".git").join("hooks").join("pre-commit");
    fs::write(&hook_path, PRE_COMMIT_HOOK)
        .with_context(|| format!("failed to write {}", hook_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&hook_path)
            .with_context(|| format!("failed to stat {}", hook_path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)
            .with_context(|| format!("failed to chmod {}", hook_path.display()))?;
    }

    Ok(())
}

fn default_workspace_dir(challenge_id: &str) -> PathBuf {
    PathBuf::from(format!("{}-solution", sanitize_path_segment(challenge_id)))
}

fn sanitize_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.', '_'])
        .to_string();

    if sanitized.is_empty() {
        "solution".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shared::models::CurrentVersionDto;
    use shared::models::challenge::{
        ChallengeBundleSpec, ChallengeDetailResponse, DatasetsSpec, LimitsSpec, MetricSchemaSpec,
        ScorerSpec, SubmissionSpec,
    };
    use shared::models::evaluation::ScoreVisibility;

    use super::{default_workspace_dir, init_solution_workspace};

    #[test]
    fn init_solution_creates_readme_git_repo_and_hook_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("sample-sum-work");

        let summary = init_solution_workspace(&challenge_detail(), Some(workspace_dir.clone()))
            .expect("workspace should initialize");

        let readme =
            fs::read_to_string(workspace_dir.join("README.md")).expect("README should be readable");
        let hook = fs::read_to_string(workspace_dir.join(".git/hooks/pre-commit"))
            .expect("hook should be readable");

        assert_eq!(summary.challenge_id, "sample-sum");
        assert!(readme.contains("# Sample Sum"));
        assert!(readme.contains("Return the sum."));
        assert!(workspace_dir.join(".git").is_dir());
        assert!(hook.contains("run.sh must exist"));
        assert!(!workspace_dir.join("run.sh").exists());
        assert_eq!(
            fs::read_dir(&workspace_dir)
                .expect("workspace should be readable")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect::<Vec<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn init_solution_rejects_existing_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("existing");
        fs::create_dir(&workspace_dir).expect("existing dir should be created");

        let error = init_solution_workspace(&challenge_detail(), Some(workspace_dir))
            .expect_err("existing dir must be rejected");

        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn default_workspace_dir_is_sanitized() {
        assert_eq!(
            default_workspace_dir("../bad challenge!*"),
            std::path::PathBuf::from("bad-challenge-solution")
        );
    }

    fn challenge_detail() -> ChallengeDetailResponse {
        ChallengeDetailResponse {
            id: "sample-sum".to_string(),
            slug: "sum".to_string(),
            title: "Sample Sum".to_string(),
            description: "Add numbers".to_string(),
            current_version: CurrentVersionDto {
                id: "version-1".to_string(),
                version: "v1".to_string(),
            },
            spec: ChallengeBundleSpec {
                schema_version: 1,
                challenge_id: "sample-sum".to_string(),
                challenge_title: "Sample Sum".to_string(),
                challenge_version: "v1".to_string(),
                submission: SubmissionSpec {
                    format: "python_zip_project".to_string(),
                    language: "python".to_string(),
                    entrypoint: "main.py".to_string(),
                },
                scorer: ScorerSpec {
                    entrypoint: "scorer/run.py".to_string(),
                    result_file: "result.json".to_string(),
                },
                limits: LimitsSpec {
                    time_limit_sec: 30.0,
                    memory_limit_mb: 512,
                },
                datasets: DatasetsSpec {
                    shown_dir: "data/shown".to_string(),
                    hidden_dir: "data/hidden".to_string(),
                    heldout_dir: None,
                    shown_policy: ScoreVisibility::Full,
                    hidden_policy: "score_only".to_string(),
                    validation_enabled: false,
                    heldout_enabled: false,
                },
                metric_schema: MetricSchemaSpec::default(),
            },
            statement_markdown: "# Statement\n\nReturn the sum.".to_string(),
        }
    }
}
