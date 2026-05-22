//! Checks that every `AGENTS.md` is the source of truth for Claude instructions.
//!
//! This command scans the repository with native Rust filesystem APIs and the
//! `ignore` walker. It excludes configured Git submodules, rejects symlinked
//! `AGENTS.md` files, requires every scanned `AGENTS.md` source file to have at
//! least one `CLAUDE.md` symlink pointing to it, and rejects non-symlink or
//! broken `CLAUDE.md` files. The command is read-only and idempotent, so
//! rollback and dry-run modes do not apply. Ctrl-C cancellation is handled by
//! the shared ops wrapper.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use ignore::WalkBuilder;
use thiserror::Error;

use crate::support::{INTERRUPTED_EXIT, run_with_ctrl_c};

const PREFIX: &str = "agentics-agents-claude-links";
const AGENTS_FILE: &str = "AGENTS.md";
const CLAUDE_FILE: &str = "CLAUDE.md";

/// CLI for checking Agentics agent-instruction symlink policy.
#[derive(Debug, Parser)]
#[command(
    about = "Checks AGENTS.md and CLAUDE.md symlink policy.",
    long_about = "Scans the repository, excluding Git submodules, and verifies that every AGENTS.md is a real source file with at least one CLAUDE.md symlink pointing to it. Non-symlink, broken, or non-AGENTS CLAUDE.md files are reported as failures."
)]
pub struct Cli {
    /// Repository root to scan.
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        let task = tokio::task::spawn_blocking(move || run(cli));
        match task.await {
            Ok(Ok(report)) => print_report(&report),
            Ok(Err(error)) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
            Err(error) if error.is_cancelled() => ExitCode::from(INTERRUPTED_EXIT),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: scan task failed: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

fn run(cli: Cli) -> Result<LinkReport, AgentsClaudeLinkError> {
    let config = LinkConfig::from_cli(cli)?;
    scan_agents_claude_links(&config)
}

/// Scanner configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkConfig {
    root: PathBuf,
}

impl LinkConfig {
    fn from_cli(cli: Cli) -> Result<Self, AgentsClaudeLinkError> {
        Ok(Self { root: cli.root })
    }

    #[cfg(test)]
    fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

/// Full symlink policy report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkReport {
    source_agents: Vec<AgentsSource>,
    claude_files_seen: usize,
    valid_links: Vec<ClaudeLink>,
    violations: Vec<LinkViolation>,
}

/// One canonical `AGENTS.md` source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsSource {
    path: PathBuf,
    canonical_path: PathBuf,
}

/// One valid `CLAUDE.md` symlink to an `AGENTS.md` source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLink {
    path: PathBuf,
    target: PathBuf,
}

/// One policy violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkViolation {
    /// `AGENTS.md` must be a source file, never a symlink.
    AgentsIsSymlink { path: PathBuf },
    /// No `CLAUDE.md` symlink points at this canonical source file.
    MissingClaudeSymlink { path: PathBuf },
    /// `CLAUDE.md` files must be symlinks.
    ClaudeIsNotSymlink { path: PathBuf },
    /// The symlink target could not be resolved.
    ClaudeBrokenSymlink { path: PathBuf, error: String },
    /// The symlink points at a file that is not one of the scanned `AGENTS.md` sources.
    ClaudeTargetIsNotAgentsSource { path: PathBuf, target: PathBuf },
}

/// Errors returned by the symlink-policy scanner.
#[derive(Debug, Error)]
pub enum AgentsClaudeLinkError {
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

    /// A discovered source path could not be canonicalized.
    #[error("failed to canonicalize {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Scan the repository for `AGENTS.md` and `CLAUDE.md` symlink policy.
pub fn scan_agents_claude_links(config: &LinkConfig) -> Result<LinkReport, AgentsClaudeLinkError> {
    let root = std::fs::canonicalize(&config.root).map_err(|source| {
        AgentsClaudeLinkError::ResolveRoot {
            path: config.root.clone(),
            source,
        }
    })?;
    let submodule_paths = submodule_paths(&root)?;
    let mut agents_by_canonical = BTreeMap::<PathBuf, PathBuf>::new();
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
        })
        .build()
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        if file_name != OsStr::new(AGENTS_FILE) && file_name != OsStr::new(CLAUDE_FILE) {
            continue;
        }

        let relative = relative_path(&root, path);
        let metadata =
            std::fs::symlink_metadata(path).map_err(|source| AgentsClaudeLinkError::Metadata {
                path: relative.clone(),
                source,
            })?;
        let file_type = metadata.file_type();

        if file_name == OsStr::new(AGENTS_FILE) {
            if file_type.is_symlink() {
                violations.push(LinkViolation::AgentsIsSymlink { path: relative });
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let canonical = std::fs::canonicalize(path).map_err(|source| {
                AgentsClaudeLinkError::Canonicalize {
                    path: relative.clone(),
                    source,
                }
            })?;
            agents_by_canonical.entry(canonical).or_insert(relative);
            continue;
        }

        claude_files_seen = claude_files_seen.saturating_add(1);
        if !file_type.is_symlink() {
            violations.push(LinkViolation::ClaudeIsNotSymlink { path: relative });
            continue;
        }

        let canonical_link =
            canonical_link_path(path).map_err(|source| AgentsClaudeLinkError::Canonicalize {
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
            Err(error) => violations.push(LinkViolation::ClaudeBrokenSymlink {
                path: relative,
                error: error.to_string(),
            }),
        }
    }

    for target in claude_targets.keys() {
        if !agents_by_canonical.contains_key(target) {
            let display_target = display_target(&root, target);
            if let Some(paths) = claude_targets.get(target) {
                for path in paths {
                    violations.push(LinkViolation::ClaudeTargetIsNotAgentsSource {
                        path: path.clone(),
                        target: display_target.clone(),
                    });
                }
            }
        }
    }

    for (canonical_path, display_path) in &agents_by_canonical {
        if !claude_targets.contains_key(canonical_path) {
            violations.push(LinkViolation::MissingClaudeSymlink {
                path: display_path.clone(),
            });
        }
    }

    let mut source_agents = agents_by_canonical
        .into_iter()
        .map(|(canonical_path, path)| AgentsSource {
            path,
            canonical_path,
        })
        .collect::<Vec<_>>();
    source_agents.sort_by(|left, right| left.path.cmp(&right.path));

    let mut valid_links = Vec::new();
    for source in &source_agents {
        if let Some(paths) = claude_targets.get(&source.canonical_path) {
            for path in paths {
                valid_links.push(ClaudeLink {
                    path: path.clone(),
                    target: source.path.clone(),
                });
            }
        }
    }
    valid_links.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then_with(|| left.path.cmp(&right.path))
    });
    sort_violations(&mut violations);

    Ok(LinkReport {
        source_agents,
        claude_files_seen,
        valid_links,
        violations,
    })
}

fn print_report(report: &LinkReport) -> ExitCode {
    if report.violations.is_empty() {
        println!(
            "[{PREFIX}] PASS scanned {} AGENTS.md source files and {} CLAUDE.md files; {} valid symlink(s)",
            report.source_agents.len(),
            report.claude_files_seen,
            report.valid_links.len()
        );
        return ExitCode::SUCCESS;
    }

    println!(
        "[{PREFIX}] FAIL {} agent-instruction symlink violation(s)",
        report.violations.len()
    );
    for violation in &report.violations {
        println!("[{PREFIX}] FAIL {}", violation.message());
    }
    ExitCode::from(1)
}

impl LinkViolation {
    fn message(&self) -> String {
        match self {
            Self::AgentsIsSymlink { path } => {
                format!(
                    "{} is a symlink; AGENTS.md must be the source file",
                    display_path(path)
                )
            }
            Self::MissingClaudeSymlink { path } => format!(
                "{} has no CLAUDE.md symlink pointing to it",
                display_path(path)
            ),
            Self::ClaudeIsNotSymlink { path } => {
                format!("{} is not a symlink", display_path(path))
            }
            Self::ClaudeBrokenSymlink { path, error } => {
                format!("{} is a broken symlink: {error}", display_path(path))
            }
            Self::ClaudeTargetIsNotAgentsSource { path, target } => format!(
                "{} points to {}, which is not a scanned AGENTS.md source",
                display_path(path),
                display_path(target)
            ),
        }
    }

    fn sort_key(&self) -> (&Path, u8, Option<&Path>) {
        match self {
            Self::AgentsIsSymlink { path } => (path.as_path(), 0, None),
            Self::MissingClaudeSymlink { path } => (path.as_path(), 1, None),
            Self::ClaudeIsNotSymlink { path } => (path.as_path(), 2, None),
            Self::ClaudeBrokenSymlink { path, .. } => (path.as_path(), 3, None),
            Self::ClaudeTargetIsNotAgentsSource { path, target } => {
                (path.as_path(), 4, Some(target.as_path()))
            }
        }
    }
}

fn sort_violations(violations: &mut [LinkViolation]) {
    violations.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
}

fn submodule_paths(root: &Path) -> Result<Vec<PathBuf>, AgentsClaudeLinkError> {
    let path = root.join(".gitmodules");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(AgentsClaudeLinkError::ReadGitmodules {
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
    fn scan_passes_when_every_agents_source_has_claude_symlink() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("AGENTS.md"), "root");
        std::fs::create_dir_all(temp.path().join(".claude")).expect("create claude dir");
        symlink("../AGENTS.md", temp.path().join(".claude/CLAUDE.md")).expect("link root claude");
        std::fs::create_dir_all(temp.path().join("ops/.claude")).expect("create ops claude dir");
        write_file(&temp.path().join("ops/AGENTS.md"), "ops");
        symlink("../AGENTS.md", temp.path().join("ops/.claude/CLAUDE.md"))
            .expect("link ops claude");

        let report = scan_agents_claude_links(&LinkConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
        assert_eq!(report.source_agents.len(), 2);
        assert_eq!(report.valid_links.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn scan_reports_missing_agents_link_and_regular_claude_file() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("frontends/web/AGENTS.md"), "web");
        write_file(
            &temp.path().join("frontends/web/CLAUDE.md"),
            "not a symlink",
        );

        let report = scan_agents_claude_links(&LinkConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(
            report
                .violations
                .contains(&LinkViolation::ClaudeIsNotSymlink {
                    path: PathBuf::from("frontends/web/CLAUDE.md"),
                })
        );
        assert!(
            report
                .violations
                .contains(&LinkViolation::MissingClaudeSymlink {
                    path: PathBuf::from("frontends/web/AGENTS.md"),
                })
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_symlinked_agents_file() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("SOURCE.md"), "source");
        symlink("SOURCE.md", temp.path().join("AGENTS.md")).expect("link agents");

        let report = scan_agents_claude_links(&LinkConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert_eq!(
            report.violations,
            vec![LinkViolation::AgentsIsSymlink {
                path: PathBuf::from("AGENTS.md"),
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_claude_symlink_to_non_agents_source() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(&temp.path().join("README.md"), "readme");
        symlink("README.md", temp.path().join("CLAUDE.md")).expect("link claude");

        let report = scan_agents_claude_links(&LinkConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert_eq!(
            report.violations,
            vec![LinkViolation::ClaudeTargetIsNotAgentsSource {
                path: PathBuf::from("CLAUDE.md"),
                target: PathBuf::from("README.md"),
            }]
        );
    }

    #[test]
    fn submodule_paths_are_excluded() {
        let temp = tempfile::tempdir().expect("create temp repo");
        write_file(
            &temp.path().join(".gitmodules"),
            r#"
[submodule "vendor/challenge"]
    path = vendor/challenge
    url = git@example.com:challenge.git
"#,
        );
        write_file(&temp.path().join("vendor/challenge/AGENTS.md"), "submodule");

        let report = scan_agents_claude_links(&LinkConfig::new(temp.path().to_path_buf()))
            .expect("scan temp repo");

        assert!(report.violations.is_empty());
        assert!(report.source_agents.is_empty());
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, content).expect("write file");
    }
}
