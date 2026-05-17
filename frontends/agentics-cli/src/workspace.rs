use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use shared::models::challenge::ChallengeDetailResponse;
use shared::models::names::ChallengeName;
use shared::models::paths::ScriptPath;
use shared::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZIP_PROJECT_PROTOCOL, ZIP_PROJECT_PROTOCOL_VERSION,
    ZipProjectCommands, ZipProjectDependencies, ZipProjectDependencyPolicy, ZipProjectInterface,
    ZipProjectInterfaceKind, ZipProjectManifest, ZipProjectPhases, ZipProjectRuntime,
};

use crate::cli::{SolutionInterface, SolutionRuntimeProfile};

const PRE_COMMIT_HOOK: &str = r#"#!/usr/bin/env bash
set -euo pipefail

if [ ! -f run.sh ]; then
  echo "pre-commit: run.sh must exist at the repository root." >&2
  exit 1
fi

if [ ! -f agentics.solution.json ]; then
  echo "pre-commit: agentics.solution.json must exist at the repository root." >&2
  exit 1
fi
"#;

#[derive(Debug, Clone, Serialize)]
/// Carries init solution summary data across this module boundary.
pub(crate) struct InitSolutionSummary {
    pub workspace_dir: PathBuf,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    pub runtime_profile: String,
    pub interface: String,
}

/// Handles init solution workspace for this module.
pub(crate) fn init_solution_workspace(
    challenge: &ChallengeDetailResponse,
    dir: Option<PathBuf>,
    runtime_profile: SolutionRuntimeProfile,
    interface: SolutionInterface,
) -> Result<InitSolutionSummary> {
    let workspace_dir = dir.unwrap_or_else(|| default_workspace_dir(&challenge.name));
    if fs::exists(&workspace_dir)
        .with_context(|| format!("failed to inspect workspace {}", workspace_dir.display()))?
    {
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

    let result = write_workspace_files(challenge, &workspace_dir, runtime_profile, interface)
        .and_then(|_| initialize_git_repository(&workspace_dir))
        .and_then(|_| install_pre_commit_hook(&workspace_dir));

    if let Err(error) = result {
        drop(fs::remove_dir_all(&workspace_dir));
        return Err(error);
    }

    let workspace_dir = fs::canonicalize(&workspace_dir)
        .with_context(|| format!("failed to resolve workspace {}", workspace_dir.display()))?;

    Ok(InitSolutionSummary {
        workspace_dir,
        challenge_name: challenge.name.clone(),
        challenge_title: challenge.title.clone(),
        runtime_profile: runtime_profile.manifest_value().to_string(),
        interface: interface.manifest_value().to_string(),
    })
}

/// Writes workspace files to the target path.
fn write_workspace_files(
    challenge: &ChallengeDetailResponse,
    workspace_dir: &Path,
    runtime_profile: SolutionRuntimeProfile,
    interface: SolutionInterface,
) -> Result<()> {
    let readme_path = workspace_dir.join("README.md");
    fs::write(
        readme_path,
        render_readme(challenge, runtime_profile, interface),
    )
    .with_context(|| {
        format!(
            "failed to write README.md in workspace {}",
            workspace_dir.display()
        )
    })?;
    fs::write(
        workspace_dir.join(ZIP_PROJECT_MANIFEST_FILE),
        render_manifest(runtime_profile, interface)?,
    )
    .with_context(|| {
        format!(
            "failed to write {} in workspace {}",
            ZIP_PROJECT_MANIFEST_FILE,
            workspace_dir.display()
        )
    })
}

/// Renders readme for user-facing output.
fn render_readme(
    challenge: &ChallengeDetailResponse,
    runtime_profile: SolutionRuntimeProfile,
    interface: SolutionInterface,
) -> String {
    format!(
        "# {}\n\nChallenge: `{}`\nStarts at: `{}`\nCloses at: `{}`\nEligibility: `{}`\nRuntime profile: `{}`\nInterface: `{}`\nTargets:\n{}\n\n{}\n\n## Workspace Contract\n\nThis workspace intentionally starts with only `README.md`, `{}`, and a Git repository.\n\nCreate a `run.sh` file at the repository root before committing. The generated pre-commit hook checks that `run.sh` and `{}` exist. Keep `run.sh` aligned with the generated manifest before packaging or submitting.\n",
        challenge.title.trim(),
        challenge.name,
        challenge.spec.starts_at.as_deref().unwrap_or("none"),
        challenge.spec.closes_at.as_deref().unwrap_or("none"),
        serde_json::to_value(challenge.spec.eligibility.eligibility_type)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_profile.manifest_value(),
        interface.manifest_value(),
        format_targets(challenge),
        challenge.statement_markdown.trim(),
        ZIP_PROJECT_MANIFEST_FILE,
        ZIP_PROJECT_MANIFEST_FILE,
    )
}

/// Handles format targets for this module.
fn format_targets(challenge: &ChallengeDetailResponse) -> String {
    challenge
        .spec
        .targets
        .iter()
        .map(|target| {
            format!(
                "- `{}`: {} {}, image `{}`",
                target.name,
                target.docker_platform.as_str(),
                target.accelerator.as_str(),
                target.resource_profile.solution_image
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders manifest for user-facing output.
fn render_manifest(
    runtime_profile: SolutionRuntimeProfile,
    interface: SolutionInterface,
) -> Result<String> {
    let profile = RuntimeProfileMetadata::for_profile(runtime_profile);
    let manifest = ZipProjectManifest {
        protocol: ZIP_PROJECT_PROTOCOL.to_string(),
        protocol_version: ZIP_PROJECT_PROTOCOL_VERSION,
        runtime: ZipProjectRuntime {
            language: profile.language.to_string(),
            language_version: profile.language_version.map(ToOwned::to_owned),
            runtime_profile: Some(runtime_profile.manifest_value().to_string()),
        },
        commands: ZipProjectCommands {
            setup: None,
            build: None,
            run: ScriptPath::try_new("run.sh")?,
        },
        phases: ZipProjectPhases::default(),
        interface: ZipProjectInterface {
            kind: interface.into(),
            input_contract: Some(interface.input_contract().to_string()),
            output_contract: Some(interface.output_contract().to_string()),
        },
        dependencies: ZipProjectDependencies {
            policy: ZipProjectDependencyPolicy::ImageProvided,
            lockfiles: Vec::new(),
            vendor_dirs: Vec::new(),
            notes: profile.dependency_notes.map(ToOwned::to_owned),
        },
    };

    Ok(serde_json::to_string_pretty(&manifest)?)
}

/// Handles initialize git repository for this module.
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

/// Handles install pre commit hook for this module.
fn install_pre_commit_hook(workspace_dir: &Path) -> Result<()> {
    let hook_path = workspace_dir.join(".git").join("hooks").join("pre-commit");
    fs::write(&hook_path, PRE_COMMIT_HOOK)
        .with_context(|| format!("failed to write {}", hook_path.display()))?;

    cfg_select! {
        unix => {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&hook_path)
                .with_context(|| format!("failed to stat {}", hook_path.display()))?
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&hook_path, permissions)
                .with_context(|| format!("failed to chmod {}", hook_path.display()))?;
        }
        _ => {}
    }

    Ok(())
}

/// Handles default workspace dir for this module.
fn default_workspace_dir(challenge_name: &ChallengeName) -> PathBuf {
    PathBuf::from(format!(
        "{}-solution",
        sanitize_path_segment(challenge_name.as_str())
    ))
}

/// Handles sanitize path segment for this module.
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

/// Carries runtime profile metadata data across this module boundary.
struct RuntimeProfileMetadata {
    language: &'static str,
    language_version: Option<&'static str>,
    dependency_notes: Option<&'static str>,
}

impl RuntimeProfileMetadata {
    /// Handles for profile for this module.
    fn for_profile(profile: SolutionRuntimeProfile) -> Self {
        match profile {
            SolutionRuntimeProfile::Python => Self {
                language: "python",
                language_version: Some("3.12"),
                dependency_notes: Some(
                    "Default generated manifest assumes dependencies are provided by the challenge image. Add setup/build scripts and lockfiles if your solution needs them.",
                ),
            },
            SolutionRuntimeProfile::Rust => Self {
                language: "rust",
                language_version: None,
                dependency_notes: Some(
                    "Add setup/build scripts and lockfiles before submitting if the solution needs Cargo dependencies or compilation.",
                ),
            },
            SolutionRuntimeProfile::Node => Self {
                language: "javascript",
                language_version: None,
                dependency_notes: Some(
                    "Add setup/build scripts and lockfiles before submitting if the solution needs package installation or bundling.",
                ),
            },
            SolutionRuntimeProfile::Generic => Self {
                language: "generic",
                language_version: None,
                dependency_notes: Some(
                    "Replace runtime metadata with the concrete language and dependency policy used by this solution.",
                ),
            },
        }
    }
}

impl SolutionRuntimeProfile {
    /// Handles manifest value for this module.
    fn manifest_value(self) -> &'static str {
        match self {
            Self::Python => "python-cpu",
            Self::Rust => "rust-cpu",
            Self::Node => "node-cpu",
            Self::Generic => "generic-cpu",
        }
    }
}

impl SolutionInterface {
    /// Handles manifest value for this module.
    fn manifest_value(self) -> &'static str {
        match self {
            Self::ChallengeDefined => "challenge_defined",
            Self::Stdio => "stdio",
            Self::FileSystem => "file_system",
        }
    }

    /// Handles input contract for this module.
    fn input_contract(self) -> &'static str {
        match self {
            Self::ChallengeDefined => "Challenge-defined input prepared by the Agentics runner.",
            Self::Stdio => "Input is provided on stdin for each runner invocation.",
            Self::FileSystem => "Input files are provided under AGENTICS_INPUT_DIR.",
        }
    }

    /// Handles output contract for this module.
    fn output_contract(self) -> &'static str {
        match self {
            Self::ChallengeDefined => {
                "Write output in the format required by the challenge statement."
            }
            Self::Stdio => "Write the answer for each invocation to stdout.",
            Self::FileSystem => "Write declared output files under AGENTICS_OUTPUT_DIR.",
        }
    }
}

impl From<SolutionInterface> for ZipProjectInterfaceKind {
    /// Handles from for this module.
    fn from(value: SolutionInterface) -> Self {
        match value {
            SolutionInterface::ChallengeDefined => Self::ChallengeDefined,
            SolutionInterface::Stdio => Self::Stdio,
            SolutionInterface::FileSystem => Self::FileSystem,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shared::models::challenge::{
        ChallengeBundleSpec, ChallengeDetailResponse, ChallengeEligibilitySpec,
        ChallengeEligibilityType, ChallengeExecutionSpec, ChallengeResultDetailVisibility,
        ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
        ChallengeVisibilitySpec, DatasetsSpec, DockerPlatform, MetricSchemaSpec,
        PrivateBenchmarkPolicy, ResourceProfileSpec, ScorerSpec, SolutionSpec, TargetAccelerator,
    };
    use shared::models::evaluation::ScoreVisibility;
    use shared::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use shared::models::names::{ChallengeName, ResourceProfileName, TargetName};
    use shared::models::paths::BundleRelativePath;
    use shared::zip_project::{
        ZipProjectInterfaceKind, ZipProjectManifest, ZipProjectNetworkAccess,
    };

    use super::{default_workspace_dir, init_solution_workspace};
    use crate::cli::{SolutionInterface, SolutionRuntimeProfile};

    /// Build a local Agentics image reference for workspace initialization tests.
    fn local_image(value: &str) -> ChallengeImageReference {
        ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(value)
                .expect("test local image is valid"),
        }
    }

    /// Verifies that init solution creates readme manifest git repo and hook.
    #[test]
    fn init_solution_creates_readme_manifest_git_repo_and_hook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("sample-sum-work");

        let summary = init_solution_workspace(
            &challenge_detail(),
            Some(workspace_dir.clone()),
            SolutionRuntimeProfile::Python,
            SolutionInterface::ChallengeDefined,
        )
        .expect("workspace should initialize");

        let readme =
            fs::read_to_string(workspace_dir.join("README.md")).expect("README should be readable");
        let manifest_raw = fs::read_to_string(workspace_dir.join("agentics.solution.json"))
            .expect("manifest should be readable");
        let manifest =
            ZipProjectManifest::parse_json(&manifest_raw).expect("manifest should parse");
        let hook = fs::read_to_string(workspace_dir.join(".git/hooks/pre-commit"))
            .expect("hook should be readable");

        assert_eq!(summary.challenge_name.as_str(), "sample-sum");
        assert_eq!(summary.runtime_profile, "python-cpu");
        assert_eq!(summary.interface, "challenge_defined");
        assert!(readme.contains("# Sample Sum"));
        assert!(readme.contains("Return the sum."));
        assert!(readme.contains("Runtime profile: `python-cpu`"));
        assert_eq!(manifest.runtime.language, "python");
        assert_eq!(manifest.runtime.language_version.as_deref(), Some("3.12"));
        assert_eq!(
            manifest.runtime.runtime_profile.as_deref(),
            Some("python-cpu")
        );
        assert_eq!(
            manifest.interface.kind,
            ZipProjectInterfaceKind::ChallengeDefined
        );
        assert!(workspace_dir.join(".git").is_dir());
        assert!(hook.contains("run.sh must exist"));
        assert!(hook.contains("agentics.solution.json must exist"));
        assert!(workspace_dir.join("agentics.solution.json").is_file());
        assert!(!workspace_dir.join("run.sh").exists());
        assert_eq!(
            fs::read_dir(&workspace_dir)
                .expect("workspace should be readable")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect::<Vec<_>>()
                .len(),
            3
        );
    }

    /// Verifies that init solution rejects existing directory.
    #[test]
    fn init_solution_rejects_existing_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("existing");
        fs::create_dir(&workspace_dir).expect("existing dir should be created");

        let error = init_solution_workspace(
            &challenge_detail(),
            Some(workspace_dir),
            SolutionRuntimeProfile::Python,
            SolutionInterface::ChallengeDefined,
        )
        .expect_err("existing dir must be rejected");

        assert!(error.to_string().contains("already exists"));
    }

    /// Verifies that init solution can generate non python manifest profile.
    #[test]
    fn init_solution_can_generate_non_python_manifest_profile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("rust-work");

        init_solution_workspace(
            &challenge_detail(),
            Some(workspace_dir.clone()),
            SolutionRuntimeProfile::Rust,
            SolutionInterface::Stdio,
        )
        .expect("workspace should initialize");

        let manifest_raw = fs::read_to_string(workspace_dir.join("agentics.solution.json"))
            .expect("manifest should be readable");
        let manifest =
            ZipProjectManifest::parse_json(&manifest_raw).expect("manifest should parse");

        assert_eq!(manifest.runtime.language, "rust");
        assert!(manifest.runtime.language_version.is_none());
        assert_eq!(
            manifest.runtime.runtime_profile.as_deref(),
            Some("rust-cpu")
        );
        assert_eq!(manifest.interface.kind, ZipProjectInterfaceKind::Stdio);
        assert_eq!(manifest.commands.run.as_str(), "run.sh");
        assert!(manifest.commands.setup.is_none());
        assert!(manifest.commands.build.is_none());
    }

    /// Verifies that default workspace dir uses challenge name.
    #[test]
    fn default_workspace_dir_uses_challenge_name() {
        assert_eq!(
            default_workspace_dir(&challenge_name("sample-sum")),
            std::path::PathBuf::from("sample-sum-solution")
        );
    }

    /// Handles challenge detail for this module.
    fn challenge_detail() -> ChallengeDetailResponse {
        ChallengeDetailResponse {
            name: challenge_name("sample-sum"),
            title: "Sample Sum".to_string(),
            summary: "Add numbers".to_string(),
            spec: ChallengeBundleSpec {
                schema_version: 1,
                challenge_name: challenge_name("sample-sum"),
                challenge_title: "Sample Sum".to_string(),
                challenge_summary: "Add numbers".to_string(),
                starts_at: None,
                closes_at: None,
                eligibility: ChallengeEligibilitySpec {
                    eligibility_type: ChallengeEligibilityType::Open,
                },
                validation_submission_limit: None,
                official_submission_limit: None,
                visibility: ChallengeVisibilitySpec {
                    leaderboard: ChallengeVisibility::PublicLive,
                    score_distribution: ChallengeVisibility::PublicLive,
                    result_detail: ChallengeResultDetailVisibility::SubmitterLivePublicLive,
                },
                solution_publication: ChallengeSolutionPublicationPolicy::Public,
                solution: SolutionSpec {
                    protocol: "zip_project".to_string(),
                    manifest_file: bundle_path("agentics.solution.json"),
                },
                scorer: ScorerSpec {
                    command: vec!["python".to_string(), "scorer/run.py".to_string()],
                    result_file: bundle_path("result.json"),
                },
                targets: vec![ChallengeTargetSpec {
                    name: target_name("linux-arm64-cpu"),
                    docker_platform: DockerPlatform::LinuxArm64,
                    accelerator: TargetAccelerator::Cpu,
                    validation_enabled: false,
                    resource_profile: ResourceProfileSpec {
                        name: resource_profile_name("python-cpu-small"),
                        resource_description: None,
                        solution_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                        scorer_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                        timeout_sec: 30,
                        memory_limit_mb: 512,
                        cpu_limit_millis: 1000,
                        disk_limit_mb: 1024,
                        setup_network_access: ZipProjectNetworkAccess::Enabled,
                        build_network_access: ZipProjectNetworkAccess::Disabled,
                        run_network_access: ZipProjectNetworkAccess::Disabled,
                        scorer_network_access: ZipProjectNetworkAccess::Disabled,
                        hardware: None,
                    },
                }],
                execution: ChallengeExecutionSpec {
                    validation_runs: Some(bundle_path("public/runs.json")),
                    validation_prepare: None,
                    official_runs: Some(bundle_path("private-benchmark/runs.json")),
                    official_prepare: None,
                },
                datasets: DatasetsSpec {
                    public_dir: bundle_path("data/public"),
                    private_benchmark_dir: None,
                    public_policy: ScoreVisibility::Full,
                    private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                    private_benchmark_enabled: false,
                },
                metric_schema: MetricSchemaSpec::default(),
            }
            .into(),
            statement_markdown: "# Statement\n\nReturn the sum.".to_string(),
        }
    }

    /// Handles challenge name for this module.
    fn challenge_name(value: &str) -> ChallengeName {
        ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
    }

    /// Handles target name for this module.
    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("test target is valid")
    }

    /// Handles resource profile name for this module.
    fn resource_profile_name(value: &str) -> ResourceProfileName {
        ResourceProfileName::try_new(value.to_string())
            .expect("test resource profile name is valid")
    }

    /// Handles bundle path for this module.
    fn bundle_path(value: &str) -> BundleRelativePath {
        BundleRelativePath::try_new(value).expect("test bundle path is valid")
    }
}
