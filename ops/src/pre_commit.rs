//! Concurrent Rust implementation of the repository pre-commit hook.
//!
//! The tracked shell hook is only a launcher for this binary. The hook reads
//! staged file names from Git, runs Rust-native policy scanners directly, and
//! executes external tools only at true development-tool boundaries such as
//! `cargo`, `git`, and `bun`. Independent checks run concurrently with bounded
//! output capture so their diagnostics remain deterministic. The command is
//! read-only, idempotent, and has no rollback or dry-run mode because it does
//! not mutate repository state.

use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use clap::Parser;
use thiserror::Error;
use tokio::process::Command;

use crate::human_agent_docs::{
    DocCheckConfig, render_report as render_human_agent_doc_report,
    report_passed as human_agent_doc_report_passed, scan_human_agent_docs,
};
use crate::large_files::{
    DEFAULT_LINE_THRESHOLD, DEFAULT_WATCH_THRESHOLD, ScanConfig,
    render_report as render_large_file_report, report_passed as large_file_report_passed,
    scan_large_files,
};
use crate::support::{
    CommandOutput, DEFAULT_OUTPUT_LIMIT_BYTES, SupportError, run_command, run_with_ctrl_c,
};

const PREFIX: &str = "agentics-pre-commit";
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_OUTPUT_LIMIT_BYTES: usize = DEFAULT_OUTPUT_LIMIT_BYTES * 128;

/// CLI for running Agentics repository pre-commit checks.
#[derive(Debug, Parser)]
#[command(
    about = "Runs Agentics repository pre-commit checks concurrently.",
    long_about = "Finds staged files, runs repository policy scanners, and runs Rust or frontend tooling when matching staged files are present. The implementation keeps the tracked shell hook tiny while making check orchestration typed, cancellation-aware, and concurrent."
)]
pub struct Cli {
    /// Repository path. The Git root is resolved from this path.
    #[arg(long, default_value = ".")]
    root: PathBuf,

    /// Line count at or above which a code file fails the large-file check.
    #[arg(long, default_value_t = DEFAULT_LINE_THRESHOLD)]
    large_file_threshold: usize,

    /// Lower line count for the large-file watch list. Use 0 to disable watch output.
    #[arg(long, default_value_t = DEFAULT_WATCH_THRESHOLD)]
    large_file_watch_threshold: usize,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(summary) => {
                summary.print();
                summary.exit_code()
            }
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> Result<PreCommitSummary, PreCommitError> {
    let repo_root = resolve_git_root(&cli.root).await?;
    let staged = staged_files(&repo_root).await?;
    if staged.is_empty() {
        return Ok(PreCommitSummary::empty());
    }

    let human_agent_docs = run_human_agent_docs_check(repo_root.clone());
    let large_files = run_large_files_check(
        repo_root.clone(),
        cli.large_file_threshold,
        cli.large_file_watch_threshold,
    );
    let rust = run_rust_checks(repo_root.clone(), staged.has_rust);
    let web = run_web_lint_check(repo_root.join("frontends/web"), staged.has_web);

    let (human_agent_docs, large_files, rust, web) =
        tokio::join!(human_agent_docs, large_files, rust, web);

    let mut outcomes = vec![human_agent_docs, large_files];
    outcomes.extend(rust);
    outcomes.push(web);
    Ok(PreCommitSummary { outcomes })
}

async fn resolve_git_root(root: &Path) -> Result<PathBuf, PreCommitError> {
    let output = run_git(root, ["rev-parse", "--show-toplevel"]).await?;
    if !output.success() {
        return Err(PreCommitError::GitFailed {
            operation: "resolve repository root",
            detail: output.combined(),
        });
    }
    let root = output.stdout.trim();
    if root.is_empty() {
        return Err(PreCommitError::GitFailed {
            operation: "resolve repository root",
            detail: "git returned an empty repository root".to_string(),
        });
    }
    Ok(PathBuf::from(root))
}

async fn staged_files(repo_root: &Path) -> Result<StagedFiles, PreCommitError> {
    let output = run_git(
        repo_root,
        ["diff", "--cached", "--name-only", "-z", "--diff-filter=ACM"],
    )
    .await?;
    if !output.success() {
        return Err(PreCommitError::GitFailed {
            operation: "list staged files",
            detail: output.combined(),
        });
    }
    Ok(StagedFiles::from_git_output(&output.stdout))
}

async fn run_git<const N: usize>(
    root: &Path,
    args: [&'static str; N],
) -> Result<CommandOutput, PreCommitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args).stdin(Stdio::null());
    run_command(command, "git", Some(GIT_TIMEOUT), TOOL_OUTPUT_LIMIT_BYTES)
        .await
        .map_err(|source| PreCommitError::Process {
            program: "git",
            source,
        })
}

async fn run_human_agent_docs_check(repo_root: PathBuf) -> CheckOutcome {
    match tokio::task::spawn_blocking(move || {
        let config = DocCheckConfig::new(repo_root);
        scan_human_agent_docs(&config)
    })
    .await
    {
        Ok(Ok(report)) => {
            if human_agent_doc_report_passed(&report) {
                CheckOutcome::pass("human/agent docs", render_human_agent_doc_report(&report))
            } else {
                CheckOutcome::fail("human/agent docs", render_human_agent_doc_report(&report))
            }
        }
        Ok(Err(error)) => CheckOutcome::error("human/agent docs", error.to_string()),
        Err(error) if error.is_cancelled() => {
            CheckOutcome::error("human/agent docs", "check task was cancelled".to_string())
        }
        Err(error) => {
            CheckOutcome::error("human/agent docs", format!("check task failed: {error}"))
        }
    }
}

async fn run_large_files_check(
    repo_root: PathBuf,
    threshold: usize,
    watch_threshold: usize,
) -> CheckOutcome {
    let watch_threshold = if watch_threshold == 0 {
        None
    } else {
        Some(watch_threshold)
    };
    let config = match ScanConfig::new(repo_root, threshold, watch_threshold) {
        Ok(config) => config,
        Err(error) => return CheckOutcome::error("large files", error.to_string()),
    };
    match tokio::task::spawn_blocking(move || scan_large_files(&config)).await {
        Ok(Ok(report)) => {
            if large_file_report_passed(&report) {
                CheckOutcome::pass("large files", render_large_file_report(&report))
            } else {
                CheckOutcome::fail("large files", render_large_file_report(&report))
            }
        }
        Ok(Err(error)) => CheckOutcome::error("large files", error.to_string()),
        Err(error) if error.is_cancelled() => {
            CheckOutcome::error("large files", "check task was cancelled".to_string())
        }
        Err(error) => CheckOutcome::error("large files", format!("check task failed: {error}")),
    }
}

async fn run_rust_checks(repo_root: PathBuf, enabled: bool) -> Vec<CheckOutcome> {
    if !enabled {
        return vec![CheckOutcome::skip("rust checks", "no staged Rust files")];
    }

    let rustfmt = run_tool_check(
        "rustfmt",
        "cargo",
        vec![
            "fmt".to_string(),
            "--all".to_string(),
            "--check".to_string(),
        ],
        repo_root.clone(),
    );
    let clippy = run_tool_check(
        "rust clippy",
        "cargo",
        [
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        repo_root,
    );

    let (rustfmt, clippy) = tokio::join!(rustfmt, clippy);
    vec![rustfmt, clippy]
}

async fn run_web_lint_check(web_root: PathBuf, enabled: bool) -> CheckOutcome {
    if !enabled {
        return CheckOutcome::skip("web lint", "no staged frontend JS/TS files");
    }
    run_tool_check(
        "web lint",
        "bun",
        vec!["run".to_string(), "lint".to_string()],
        web_root,
    )
    .await
}

async fn run_tool_check(
    name: &'static str,
    program: &'static str,
    args: Vec<String>,
    cwd: PathBuf,
) -> CheckOutcome {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd).stdin(Stdio::null());
    match run_command(command, program, None, TOOL_OUTPUT_LIMIT_BYTES).await {
        Ok(output) if output.success() => {
            CheckOutcome::pass(name, render_command_output(name, &output))
        }
        Ok(output) => {
            let mut lines = vec![format!(
                "[{PREFIX}] FAIL {name} exited with status {}",
                output
                    .status
                    .map_or_else(|| "signal".to_string(), |status| status.to_string())
            )];
            lines.extend(render_command_output(name, &output));
            CheckOutcome::fail(name, lines)
        }
        Err(error) => CheckOutcome::error(name, error.to_string()),
    }
}

fn render_command_output(name: &str, output: &CommandOutput) -> Vec<String> {
    let mut lines = Vec::new();
    push_stream_lines(&mut lines, name, "stdout", &output.stdout);
    push_stream_lines(&mut lines, name, "stderr", &output.stderr);
    if output.truncated {
        lines.push(format!("[{PREFIX}] WARN {name} output was truncated"));
    }
    lines
}

fn push_stream_lines(lines: &mut Vec<String>, name: &str, stream: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    for line in text.lines() {
        lines.push(format!("[{PREFIX}] {name} {stream}: {line}"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StagedFiles {
    paths: Vec<PathBuf>,
    has_rust: bool,
    has_web: bool,
}

impl StagedFiles {
    fn from_git_output(output: &str) -> Self {
        let paths = output
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        Self::from_paths(paths)
    }

    fn from_paths(paths: Vec<PathBuf>) -> Self {
        let has_rust = paths
            .iter()
            .any(|path| path.extension().is_some_and(|extension| extension == "rs"));
        let has_web = paths.iter().any(|path| {
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                return false;
            };
            matches!(extension, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
        });
        Self {
            paths,
            has_rust,
            has_web,
        }
    }

    fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[derive(Debug, Clone)]
struct PreCommitSummary {
    outcomes: Vec<CheckOutcome>,
}

impl PreCommitSummary {
    fn empty() -> Self {
        Self {
            outcomes: Vec::new(),
        }
    }

    fn print(&self) {
        if self.outcomes.is_empty() {
            return;
        }
        for outcome in &self.outcomes {
            outcome.print();
        }
        println!("[{PREFIX}] Done.");
    }

    fn exit_code(&self) -> ExitCode {
        if self
            .outcomes
            .iter()
            .any(|outcome| matches!(outcome.status, CheckStatus::Error(_)))
        {
            return ExitCode::from(2);
        }
        if self
            .outcomes
            .iter()
            .any(|outcome| matches!(outcome.status, CheckStatus::Fail))
        {
            return ExitCode::from(1);
        }
        ExitCode::SUCCESS
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckOutcome {
    name: &'static str,
    status: CheckStatus,
    lines: Vec<String>,
}

impl CheckOutcome {
    fn pass(name: &'static str, lines: Vec<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            lines,
        }
    }

    fn skip(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skip(message.into()),
            lines: Vec::new(),
        }
    }

    fn fail(name: &'static str, lines: Vec<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            lines,
        }
    }

    fn error(name: &'static str, message: String) -> Self {
        Self {
            name,
            status: CheckStatus::Error(message),
            lines: Vec::new(),
        }
    }

    fn print(&self) {
        match &self.status {
            CheckStatus::Pass => println!("[{PREFIX}] PASS {}", self.name),
            CheckStatus::Skip(message) => println!("[{PREFIX}] SKIP {} - {message}", self.name),
            CheckStatus::Fail => println!("[{PREFIX}] FAIL {}", self.name),
            CheckStatus::Error(message) => {
                println!("[{PREFIX}] ERROR {} - {message}", self.name);
            }
        }
        for line in &self.lines {
            println!("{line}");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CheckStatus {
    Pass,
    Skip(String),
    Fail,
    Error(String),
}

/// Errors returned by pre-commit orchestration.
#[derive(Debug, Error)]
enum PreCommitError {
    /// A process boundary failed before the tool could report a result.
    #[error("failed to run {program}: {source}")]
    Process {
        program: &'static str,
        source: SupportError,
    },

    /// Git returned an unsuccessful status for a repository query.
    #[error("git failed to {operation}: {detail}")]
    GitFailed {
        operation: &'static str,
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_file_classification_detects_rust_and_frontend_work() {
        let staged = StagedFiles::from_paths(vec![
            PathBuf::from("backend/shared/src/lib.rs"),
            PathBuf::from("frontends/web/src/app/page.tsx"),
            PathBuf::from("docs/README.md"),
        ]);

        assert!(staged.has_rust);
        assert!(staged.has_web);
        assert!(!staged.is_empty());
    }

    #[test]
    fn staged_file_classification_ignores_non_matching_files() {
        let staged = StagedFiles::from_git_output("docs/README.md\0justfile\0");

        assert!(!staged.has_rust);
        assert!(!staged.has_web);
        assert_eq!(
            staged.paths,
            vec![PathBuf::from("docs/README.md"), PathBuf::from("justfile")]
        );
    }

    #[test]
    fn summary_exit_code_distinguishes_failures_from_infra_errors() {
        let passing = PreCommitSummary {
            outcomes: vec![
                CheckOutcome::pass("ok", Vec::new()),
                CheckOutcome::skip("skip", "not applicable"),
            ],
        };
        assert_eq!(passing.exit_code(), ExitCode::SUCCESS);

        let failing = PreCommitSummary {
            outcomes: vec![CheckOutcome::fail("bad", Vec::new())],
        };
        assert_eq!(failing.exit_code(), ExitCode::from(1));

        let errored = PreCommitSummary {
            outcomes: vec![CheckOutcome::error("tool", "missing".to_string())],
        };
        assert_eq!(errored.exit_code(), ExitCode::from(2));
    }
}
