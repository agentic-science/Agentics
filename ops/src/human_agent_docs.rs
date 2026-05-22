//! Checks human-facing and agent-facing instruction documents.
//!
//! This command scans the repository with native Rust filesystem APIs and the
//! `ignore` walker. It excludes configured Git submodules and common dependency
//! or build directories. The check enforces three conventions:
//!
//! - every scanned `README.md` has a colocated `AGENTS.md`;
//! - `AGENTS.md` is normally the source file, but may be a symlink to a
//!   scanned `README.md`;
//! - every effective `AGENTS.md` instruction source has at least one
//!   `CLAUDE.md` symlink pointing to it, and every scanned `CLAUDE.md` is a
//!   valid symlink to such a source.
//!
//! The command is read-only and idempotent, so rollback and dry-run modes do
//! not apply. Ctrl-C sets a cooperative cancellation flag for the blocking
//! filesystem walk.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use ignore::WalkBuilder;
use thiserror::Error;

use crate::support::INTERRUPTED_EXIT;

const PREFIX: &str = "agentics-human-agent-docs";
const AGENTS_FILE: &str = "AGENTS.md";
const CLAUDE_FILE: &str = "CLAUDE.md";
const README_FILE: &str = "README.md";

const EXCLUDED_COMPONENTS: &[&str] = &[
    ".git",
    ".next",
    ".pytest_cache",
    ".turbo",
    "build",
    "coverage",
    "dist",
    "examples",
    "node_modules",
    "target",
];

/// CLI for checking Agentics human/agent instruction document policy.
#[derive(Debug, Parser)]
#[command(
    about = "Checks README.md, AGENTS.md, and CLAUDE.md instruction policy.",
    long_about = "Scans the repository, excluding Git submodules and common dependency/build directories. Verifies that every README.md has a colocated AGENTS.md, that AGENTS.md is a real source file or a symlink to a scanned README.md, and that CLAUDE.md files are symlinks to effective AGENTS.md instruction sources."
)]
pub struct Cli {
    /// Repository root to scan.
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    let cancel = Arc::new(AtomicBool::new(false));
    let task_cancel = Arc::clone(&cancel);
    let task = tokio::task::spawn_blocking(move || run(cli, Some(task_cancel)));
    tokio::select! {
        result = task => match result {
            Ok(Ok(report)) => print_report(&report),
            Ok(Err(HumanAgentDocError::Cancelled)) => ExitCode::from(INTERRUPTED_EXIT),
            Ok(Err(error)) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
            Err(error) if error.is_cancelled() => ExitCode::from(INTERRUPTED_EXIT),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: scan task failed: {error}");
                ExitCode::from(2)
            }
        },
        signal = tokio::signal::ctrl_c() => {
            cancel.store(true, Ordering::Relaxed);
            match signal {
                Ok(()) => eprintln!("[{PREFIX}] interrupted by Ctrl-C"),
                Err(error) => eprintln!("[{PREFIX}] failed to listen for Ctrl-C: {error}"),
            }
            ExitCode::from(INTERRUPTED_EXIT)
        }
    }
}

fn run(cli: Cli, cancel: Option<Arc<AtomicBool>>) -> Result<DocCheckReport, HumanAgentDocError> {
    let config = DocCheckConfig::from_cli(cli);
    scan_human_agent_docs_with_cancel(&config, cancel.as_deref())
}

/// Scanner configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocCheckConfig {
    root: PathBuf,
}

impl DocCheckConfig {
    fn from_cli(cli: Cli) -> Self {
        Self::new(cli.root)
    }

    /// Build scanner configuration from an explicit repository root.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

/// Full instruction-document policy report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocCheckReport {
    readme_files_seen: usize,
    effective_agent_sources: Vec<EffectiveAgentSource>,
    claude_files_seen: usize,
    valid_claude_links: Vec<ClaudeLink>,
    violations: Vec<DocViolation>,
}

/// One effective agent instruction source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAgentSource {
    path: PathBuf,
    canonical_path: PathBuf,
}

/// One valid `CLAUDE.md` symlink to an effective agent instruction source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLink {
    path: PathBuf,
    target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadmeEntry {
    path: PathBuf,
    canonical_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentEntry {
    path: PathBuf,
    canonical_path: PathBuf,
    symlink_target: Option<PathBuf>,
}

/// One policy violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocViolation {
    /// `README.md` must have a colocated `AGENTS.md`.
    MissingColocatedAgents { path: PathBuf },
    /// `AGENTS.md` symlinks may target only a scanned `README.md`.
    AgentsSymlinkTargetIsNotReadme { path: PathBuf, target: PathBuf },
    /// The symlink target could not be resolved.
    AgentsBrokenSymlink { path: PathBuf, error: String },
    /// No `CLAUDE.md` symlink points at this effective source.
    MissingClaudeSymlink { path: PathBuf },
    /// `CLAUDE.md` files must be symlinks.
    ClaudeIsNotSymlink { path: PathBuf },
    /// The symlink target could not be resolved.
    ClaudeBrokenSymlink { path: PathBuf, error: String },
    /// The symlink points at a file that is not an effective agent source.
    ClaudeTargetIsNotAgentSource { path: PathBuf, target: PathBuf },
}

/// Errors returned by the instruction-document scanner.
#[derive(Debug, Error)]
pub enum HumanAgentDocError {
    /// The requested scan root could not be resolved.
    #[error("failed to resolve scan root {path}: {source}")]
    ResolveRoot {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The `.gitmodules` file could not be read.
    #[error("failed to read {path}: {source}")]
    ReadGitmodules {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The source tree walk failed.
    #[error("failed to walk source tree: {0}")]
    Walk(#[from] ignore::Error),

    /// File metadata could not be read.
    #[error("failed to inspect {path}: {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },

    /// A discovered path could not be canonicalized.
    #[error("failed to canonicalize {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The scan was cancelled.
    #[error("scan cancelled")]
    Cancelled,
}

/// Scan the repository for human/agent instruction document policy.
pub fn scan_human_agent_docs(
    config: &DocCheckConfig,
) -> Result<DocCheckReport, HumanAgentDocError> {
    scan_human_agent_docs_with_cancel(config, None)
}

/// Scan the repository for human/agent instruction document policy with cancellation.
pub fn scan_human_agent_docs_with_cancel(
    config: &DocCheckConfig,
    cancel: Option<&AtomicBool>,
) -> Result<DocCheckReport, HumanAgentDocError> {
    let root =
        std::fs::canonicalize(&config.root).map_err(|source| HumanAgentDocError::ResolveRoot {
            path: config.root.clone(),
            source,
        })?;
    let submodule_paths = submodule_paths(&root)?;
    let mut readmes_by_directory = BTreeMap::<PathBuf, ReadmeEntry>::new();
    let mut readme_canonicals = BTreeSet::<PathBuf>::new();
    let mut agents_by_directory = BTreeMap::<PathBuf, AgentEntry>::new();
    let mut claude_targets = BTreeMap::<PathBuf, BTreeSet<PathBuf>>::new();
    let mut seen_claude_links = BTreeSet::<PathBuf>::new();
    let mut claude_files_seen = 0usize;
    let mut violations = Vec::new();

    let filter_root = root.clone();
    let filter_submodule_paths = submodule_paths.clone();
    for entry in WalkBuilder::new(&root)
        .parents(true)
        .hidden(false)
        .follow_links(false)
        .filter_entry(move |entry| {
            let relative = relative_path(&filter_root, entry.path());
            !is_submodule_path(&relative, &filter_submodule_paths)
                && !is_git_internal_path(&relative)
                && !has_excluded_component(&relative)
        })
        .build()
    {
        ensure_not_cancelled(cancel)?;
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        if !is_instruction_doc_name(file_name) {
            continue;
        }

        let relative = relative_path(&root, path);
        let directory = relative
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let metadata =
            std::fs::symlink_metadata(path).map_err(|source| HumanAgentDocError::Metadata {
                path: relative.clone(),
                source,
            })?;
        let file_type = metadata.file_type();

        if file_name == OsStr::new(README_FILE) {
            let canonical =
                std::fs::canonicalize(path).map_err(|source| HumanAgentDocError::Canonicalize {
                    path: relative.clone(),
                    source,
                })?;
            readme_canonicals.insert(canonical.clone());
            readmes_by_directory
                .entry(directory)
                .or_insert(ReadmeEntry {
                    path: relative,
                    canonical_path: canonical,
                });
            continue;
        }

        if file_name == OsStr::new(AGENTS_FILE) {
            let canonical = match std::fs::canonicalize(path) {
                Ok(canonical) => canonical,
                Err(error) if file_type.is_symlink() => {
                    violations.push(DocViolation::AgentsBrokenSymlink {
                        path: relative,
                        error: error.to_string(),
                    });
                    continue;
                }
                Err(source) => {
                    return Err(HumanAgentDocError::Canonicalize {
                        path: relative,
                        source,
                    });
                }
            };
            let symlink_target = if file_type.is_symlink() {
                Some(display_target(&root, &canonical))
            } else {
                None
            };
            agents_by_directory.entry(directory).or_insert(AgentEntry {
                path: relative,
                canonical_path: canonical,
                symlink_target,
            });
            continue;
        }

        claude_files_seen = claude_files_seen.saturating_add(1);
        if !file_type.is_symlink() {
            violations.push(DocViolation::ClaudeIsNotSymlink { path: relative });
            continue;
        }

        let canonical_link =
            canonical_link_path(path).map_err(|source| HumanAgentDocError::Canonicalize {
                path: relative.clone(),
                source,
            })?;
        if !seen_claude_links.insert(canonical_link) {
            continue;
        }
        match std::fs::canonicalize(path) {
            Ok(target) => {
                claude_targets.entry(target).or_default().insert(relative);
            }
            Err(error) => violations.push(DocViolation::ClaudeBrokenSymlink {
                path: relative,
                error: error.to_string(),
            }),
        }
    }

    for (directory, readme) in &readmes_by_directory {
        if !agents_by_directory.contains_key(directory) {
            violations.push(DocViolation::MissingColocatedAgents {
                path: readme.path.clone(),
            });
        }
    }

    for agent in agents_by_directory.values() {
        if let Some(target) = &agent.symlink_target
            && !readme_canonicals.contains(&agent.canonical_path)
        {
            violations.push(DocViolation::AgentsSymlinkTargetIsNotReadme {
                path: agent.path.clone(),
                target: target.clone(),
            });
        }
    }

    let mut source_by_canonical = BTreeMap::<PathBuf, PathBuf>::new();
    for agent in agents_by_directory.values() {
        if agent.symlink_target.is_none() || readme_canonicals.contains(&agent.canonical_path) {
            source_by_canonical
                .entry(agent.canonical_path.clone())
                .or_insert_with(|| agent.path.clone());
        }
    }

    for target in claude_targets.keys() {
        if !source_by_canonical.contains_key(target) {
            let display_target = display_target(&root, target);
            if let Some(paths) = claude_targets.get(target) {
                for path in paths {
                    violations.push(DocViolation::ClaudeTargetIsNotAgentSource {
                        path: path.clone(),
                        target: display_target.clone(),
                    });
                }
            }
        }
    }

    for (canonical_path, display_path) in &source_by_canonical {
        if !claude_targets.contains_key(canonical_path) {
            violations.push(DocViolation::MissingClaudeSymlink {
                path: display_path.clone(),
            });
        }
    }

    let mut effective_agent_sources = source_by_canonical
        .into_iter()
        .map(|(canonical_path, path)| EffectiveAgentSource {
            path,
            canonical_path,
        })
        .collect::<Vec<_>>();
    effective_agent_sources.sort_by(|left, right| left.path.cmp(&right.path));

    let mut valid_claude_links = Vec::new();
    for source in &effective_agent_sources {
        if let Some(paths) = claude_targets.get(&source.canonical_path) {
            for path in paths {
                valid_claude_links.push(ClaudeLink {
                    path: path.clone(),
                    target: source.path.clone(),
                });
            }
        }
    }
    valid_claude_links.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then_with(|| left.path.cmp(&right.path))
    });
    sort_violations(&mut violations);

    Ok(DocCheckReport {
        readme_files_seen: readmes_by_directory.len(),
        effective_agent_sources,
        claude_files_seen,
        valid_claude_links,
        violations,
    })
}

fn ensure_not_cancelled(cancel: Option<&AtomicBool>) -> Result<(), HumanAgentDocError> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        Err(HumanAgentDocError::Cancelled)
    } else {
        Ok(())
    }
}

/// Whether a human/agent document report has no policy violations.
pub fn report_passed(report: &DocCheckReport) -> bool {
    report.violations.is_empty()
}

/// Render a human/agent document report into deterministic output lines.
pub fn render_report(report: &DocCheckReport) -> Vec<String> {
    let mut lines = Vec::new();
    if report.violations.is_empty() {
        lines.push(format!(
            "[{PREFIX}] PASS scanned {} README.md files, {} effective AGENTS.md sources, and {} CLAUDE.md files; {} valid Claude symlink(s)",
            report.readme_files_seen,
            report.effective_agent_sources.len(),
            report.claude_files_seen,
            report.valid_claude_links.len()
        ));
        return lines;
    }

    lines.push(format!(
        "[{PREFIX}] FAIL {} human/agent doc violation(s)",
        report.violations.len()
    ));
    for violation in &report.violations {
        lines.push(format!("[{PREFIX}] FAIL {}", violation.message()));
    }
    lines
}

fn print_report(report: &DocCheckReport) -> ExitCode {
    for line in render_report(report) {
        println!("{line}");
    }
    if report_passed(report) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

impl DocViolation {
    fn message(&self) -> String {
        match self {
            Self::MissingColocatedAgents { path } => {
                format!("{} has no colocated AGENTS.md", display_path(path))
            }
            Self::AgentsSymlinkTargetIsNotReadme { path, target } => format!(
                "{} is a symlink to {}; AGENTS.md symlinks may only target scanned README.md files",
                display_path(path),
                display_path(target)
            ),
            Self::AgentsBrokenSymlink { path, error } => {
                format!("{} is a broken symlink: {error}", display_path(path))
            }
            Self::MissingClaudeSymlink { path } => format!(
                "{} has no CLAUDE.md symlink pointing to its effective instruction source",
                display_path(path)
            ),
            Self::ClaudeIsNotSymlink { path } => {
                format!("{} is not a symlink", display_path(path))
            }
            Self::ClaudeBrokenSymlink { path, error } => {
                format!("{} is a broken symlink: {error}", display_path(path))
            }
            Self::ClaudeTargetIsNotAgentSource { path, target } => format!(
                "{} points to {}, which is not an effective AGENTS.md instruction source",
                display_path(path),
                display_path(target)
            ),
        }
    }

    fn sort_key(&self) -> (&Path, u8, Option<&Path>) {
        match self {
            Self::MissingColocatedAgents { path } => (path.as_path(), 0, None),
            Self::AgentsSymlinkTargetIsNotReadme { path, target } => {
                (path.as_path(), 1, Some(target.as_path()))
            }
            Self::AgentsBrokenSymlink { path, .. } => (path.as_path(), 2, None),
            Self::MissingClaudeSymlink { path } => (path.as_path(), 3, None),
            Self::ClaudeIsNotSymlink { path } => (path.as_path(), 4, None),
            Self::ClaudeBrokenSymlink { path, .. } => (path.as_path(), 5, None),
            Self::ClaudeTargetIsNotAgentSource { path, target } => {
                (path.as_path(), 6, Some(target.as_path()))
            }
        }
    }
}

fn sort_violations(violations: &mut [DocViolation]) {
    violations.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
}

fn submodule_paths(root: &Path) -> Result<Vec<PathBuf>, HumanAgentDocError> {
    let path = root.join(".gitmodules");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(HumanAgentDocError::ReadGitmodules {
                path: relative_path(root, &path),
                source,
            });
        }
    };
    let mut paths = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "path" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            continue;
        }
        let submodule_path = PathBuf::from(value);
        if submodule_path.is_absolute() {
            continue;
        }
        paths.push(submodule_path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn is_submodule_path(path: &Path, submodule_paths: &[PathBuf]) -> bool {
    submodule_paths
        .iter()
        .any(|submodule_path| path == submodule_path || path.starts_with(submodule_path))
}

fn is_git_internal_path(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|component| component.as_os_str() == OsStr::new(".git"))
}

fn has_excluded_component(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str();
        EXCLUDED_COMPONENTS
            .iter()
            .any(|excluded| name == OsStr::new(*excluded))
    })
}

fn is_instruction_doc_name(file_name: &OsStr) -> bool {
    [AGENTS_FILE, CLAUDE_FILE, README_FILE]
        .into_iter()
        .any(|expected| file_name == OsStr::new(expected))
}

fn canonical_link_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let Some(parent) = path.parent() else {
        return std::fs::canonicalize(path);
    };
    let Some(file_name) = path.file_name() else {
        return std::fs::canonicalize(path);
    };
    Ok(std::fs::canonicalize(parent)?.join(file_name))
}

fn display_target(root: &Path, target: &Path) -> PathBuf {
    target
        .strip_prefix(root)
        .map_or_else(|_| target.to_path_buf(), Path::to_path_buf)
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[cfg(unix)]
    #[test]
    fn scan_passes_when_readme_agents_and_claude_are_aligned() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "root readme");
        write_file(&temp.path().join("AGENTS.md"), "root agents");
        std::fs::create_dir_all(temp.path().join(".claude")).expect("create claude dir");
        symlink("../AGENTS.md", temp.path().join(".claude/CLAUDE.md")).expect("link root claude");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
        assert_eq!(report.readme_files_seen, 1);
        assert_eq!(report.effective_agent_sources.len(), 1);
        assert_eq!(report.valid_claude_links.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn readme_symlink_to_agents_is_allowed() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("docs/AGENTS.md"), "docs");
        symlink("AGENTS.md", temp.path().join("docs/README.md")).expect("link readme");
        symlink("AGENTS.md", temp.path().join("docs/CLAUDE.md")).expect("link claude");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn agents_symlink_to_readme_is_allowed() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("docs/README.md"), "docs");
        symlink("README.md", temp.path().join("docs/AGENTS.md")).expect("link agents");
        symlink("AGENTS.md", temp.path().join("docs/CLAUDE.md")).expect("link claude");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
    }

    #[test]
    fn scan_stops_when_cancelled() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "root");

        let cancel = AtomicBool::new(true);
        let error = scan_human_agent_docs_with_cancel(
            &DocCheckConfig::new(temp.path().to_path_buf()),
            Some(&cancel),
        )
        .expect_err("cancelled scan should stop");

        assert!(matches!(error, HumanAgentDocError::Cancelled));
    }

    #[cfg(unix)]
    #[test]
    fn scan_reports_readme_without_colocated_agents() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("frontends/web/README.md"), "web");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert_eq!(
            report.violations,
            vec![DocViolation::MissingColocatedAgents {
                path: PathBuf::from("frontends/web/README.md"),
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_agents_symlink_to_non_readme() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "root");
        write_file(&temp.path().join("SOURCE.md"), "source");
        symlink("SOURCE.md", temp.path().join("AGENTS.md")).expect("link agents");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(
            report
                .violations
                .contains(&DocViolation::AgentsSymlinkTargetIsNotReadme {
                    path: PathBuf::from("AGENTS.md"),
                    target: PathBuf::from("SOURCE.md"),
                })
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_regular_claude_file() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "root");
        write_file(&temp.path().join("AGENTS.md"), "root");
        write_file(&temp.path().join("CLAUDE.md"), "not a symlink");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(
            report
                .violations
                .contains(&DocViolation::ClaudeIsNotSymlink {
                    path: PathBuf::from("CLAUDE.md"),
                })
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_claude_symlink_to_non_agent_source() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "readme");
        write_file(&temp.path().join("AGENTS.md"), "agents");
        write_file(&temp.path().join("OTHER.md"), "other");
        symlink("OTHER.md", temp.path().join("CLAUDE.md")).expect("link claude");

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(
            report
                .violations
                .contains(&DocViolation::ClaudeTargetIsNotAgentSource {
                    path: PathBuf::from("CLAUDE.md"),
                    target: PathBuf::from("OTHER.md"),
                })
        );
    }

    #[test]
    fn submodule_paths_and_dependency_dirs_are_excluded() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(
            &temp.path().join(".gitmodules"),
            r#"
[submodule "vendor/challenge"]
    path = vendor/challenge
    url = git@example.com:challenge.git
"#,
        );
        write_file(&temp.path().join("vendor/challenge/README.md"), "submodule");
        write_file(
            &temp.path().join("node_modules/package/README.md"),
            "dependency",
        );
        write_file(
            &temp.path().join("examples/solutions/sample/README.md"),
            "example",
        );

        let report = scan_human_agent_docs(&DocCheckConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
        assert_eq!(report.readme_files_seen, 0);
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, content).expect("write file");
    }
}
