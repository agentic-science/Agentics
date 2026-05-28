//! Staged-aware repository pre-commit hook.
//!
//! The tracked shell hook is only a launcher for this binary. This command
//! reads staged state with `gix`, classifies the commit surface, and runs only
//! the checks that apply to the staged paths.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use direct assertions for concise failure diagnostics"
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agentics_dev_checks::human_agent_docs::{
    DocCheckConfig, render_report as render_human_agent_doc_report,
    report_passed as human_agent_doc_report_passed, scan_human_agent_docs_with_cancel,
};
use agentics_dev_checks::large_files::{
    BlobScanConfig, DEFAULT_LINE_THRESHOLD, DEFAULT_WATCH_THRESHOLD, SourceBlob,
    is_source_file_path, render_report as render_large_file_report,
    report_passed as large_file_report_passed, scan_large_file_blobs,
};
use agentics_dev_checks::support::{
    CommandOutput, DEFAULT_OUTPUT_LIMIT_BYTES, INTERRUPTED_EXIT, run_command,
};
use clap::Parser;
use gix::bstr::ByteSlice;
use gix::index::entry::{Mode, Stage};
use thiserror::Error;
use tokio::process::Command;

const PREFIX: &str = "agentics-pre-commit";
const TOOL_OUTPUT_LIMIT_BYTES: usize = DEFAULT_OUTPUT_LIMIT_BYTES * 128;

/// CLI for running Agentics repository pre-commit checks.
#[derive(Debug, Parser)]
#[command(
    about = "Runs staged-aware Agentics repository pre-commit checks.",
    long_about = "Reads the staged index with gix, classifies touched paths, and runs Rust, frontend, documentation, and large-file checks only when the staged commit requires them."
)]
struct Cli {
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

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let cancel = Arc::new(AtomicBool::new(false));
    let run_cancel = Arc::clone(&cancel);
    tokio::select! {
        result = run(cli, run_cancel) => match result {
            Ok(summary) => {
                summary.print();
                summary.exit_code()
            }
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
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

async fn run(cli: Cli, cancel: Arc<AtomicBool>) -> Result<PreCommitSummary, PreCommitError> {
    let (repo_root, plan, large_file_blobs) = {
        let repo = StagedRepository::open(&cli.root)?;
        let staged = repo.staged_changes()?;
        if staged.is_empty() {
            return Ok(PreCommitSummary::empty());
        }
        let plan = CheckPlan::from_changes(&staged);
        let large_file_blobs = repo.source_blobs(&staged)?;
        (repo.root.clone(), plan, large_file_blobs)
    };

    let human_agent_docs =
        run_human_agent_docs_check(repo_root.clone(), plan.has_docs, Arc::clone(&cancel));
    let large_files = run_large_files_check(
        large_file_blobs,
        cli.large_file_threshold,
        cli.large_file_watch_threshold,
        plan.large_files,
    );
    let rust = run_rust_checks(repo_root.clone(), plan.has_rust);
    let web = run_web_lint_check(repo_root.join("frontends/web"), plan.has_web);

    let (human_agent_docs, large_files, rust, web) =
        tokio::join!(human_agent_docs, large_files, rust, web);

    let mut outcomes = vec![human_agent_docs, large_files];
    outcomes.extend(rust);
    outcomes.push(web);
    Ok(PreCommitSummary {
        staged: Some(plan.summary()),
        outcomes,
    })
}

struct StagedRepository {
    repo: gix::Repository,
    root: PathBuf,
}

impl StagedRepository {
    fn open(root: &Path) -> Result<Self, PreCommitError> {
        let repo = gix::discover(root).map_err(|source| PreCommitError::Discover {
            path: root.to_path_buf(),
            source: Box::new(source),
        })?;
        let workdir = repo.workdir().ok_or(PreCommitError::BareRepository)?;
        let root =
            std::fs::canonicalize(workdir).map_err(|source| PreCommitError::ResolveRoot {
                path: workdir.to_path_buf(),
                source,
            })?;
        Ok(Self { repo, root })
    }

    fn staged_changes(&self) -> Result<Vec<StagedChange>, PreCommitError> {
        let head_tree_id =
            self.repo
                .head_tree_id_or_empty()
                .map_err(|source| PreCommitError::HeadTree {
                    source: Box::new(source),
                })?;
        let head_index = self
            .repo
            .index_from_tree(head_tree_id.as_ref())
            .map_err(|source| PreCommitError::HeadIndex {
                source: Box::new(source),
            })?;
        let worktree_index =
            self.repo
                .index_or_empty()
                .map_err(|source| PreCommitError::Index {
                    source: Box::new(source),
                })?;

        let head_entries = collect_index_entries(&head_index)?;
        let staged_entries = collect_index_entries(&worktree_index)?;
        let all_paths = head_entries
            .keys()
            .chain(staged_entries.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut changes = Vec::new();
        for path in all_paths {
            match (head_entries.get(&path), staged_entries.get(&path)) {
                (None, Some(new)) => changes.push(StagedChange::added(new)),
                (Some(old), None) => changes.push(StagedChange::deleted(old)),
                (Some(old), Some(new)) if old.id == new.id && old.mode == new.mode => {}
                (Some(old), Some(new)) if old.mode != new.mode => {
                    changes.push(StagedChange::type_changed(old, new));
                }
                (Some(old), Some(new)) => changes.push(StagedChange::modified(old, new)),
                (None, None) => {}
            }
        }
        Ok(changes)
    }

    fn source_blobs(&self, changes: &[StagedChange]) -> Result<Vec<SourceBlob>, PreCommitError> {
        let mut blobs = Vec::new();
        for change in changes {
            if !change.has_staged_blob()
                || change.is_submodule
                || !change.mode_has_blob()
                || !is_source_file_path(&change.path)
            {
                continue;
            }
            let id = change
                .object_id
                .ok_or_else(|| PreCommitError::MissingBlobId {
                    path: change.path.clone(),
                })?;
            let mut blob = self
                .repo
                .find_blob(id)
                .map_err(|source| PreCommitError::Blob {
                    path: change.path.clone(),
                    source: Box::new(source),
                })?;
            blobs.push(SourceBlob::new(change.path.clone(), blob.take_data()));
        }
        Ok(blobs)
    }
}

fn collect_index_entries(
    index: &gix::index::State,
) -> Result<BTreeMap<Vec<u8>, IndexEntry>, PreCommitError> {
    let mut entries = BTreeMap::new();
    for entry in index.entries() {
        let path = entry.path(index);
        if entry.stage() != Stage::Unconflicted {
            return Err(PreCommitError::UnresolvedConflict {
                path: path_to_path_buf(path.as_bytes())?,
            });
        }
        let path_bytes = path.as_bytes().to_vec();
        let path_buf = path_to_path_buf(&path_bytes)?;
        let info = IndexEntry {
            path: path_buf.clone(),
            id: entry.id,
            mode: entry.mode,
        };
        if entries.insert(path_bytes, info).is_some() {
            return Err(PreCommitError::DuplicateIndexPath { path: path_buf });
        }
    }
    Ok(entries)
}

fn path_to_path_buf(path: &[u8]) -> Result<PathBuf, PreCommitError> {
    let path = std::str::from_utf8(path).map_err(|source| PreCommitError::NonUtf8Path {
        source,
        path: path.to_vec(),
    })?;
    Ok(PathBuf::from(path))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexEntry {
    path: PathBuf,
    id: gix::ObjectId,
    mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StagedChange {
    path: PathBuf,
    kind: ChangeKind,
    object_id: Option<gix::ObjectId>,
    mode: Option<Mode>,
    is_submodule: bool,
}

impl StagedChange {
    fn added(new: &IndexEntry) -> Self {
        Self::from_entries(new.path.clone(), ChangeKind::Added, None, Some(new))
    }

    fn modified(_old: &IndexEntry, new: &IndexEntry) -> Self {
        Self::from_entries(new.path.clone(), ChangeKind::Modified, None, Some(new))
    }

    fn deleted(old: &IndexEntry) -> Self {
        Self::from_entries(old.path.clone(), ChangeKind::Deleted, Some(old), None)
    }

    fn type_changed(old: &IndexEntry, new: &IndexEntry) -> Self {
        Self::from_entries(
            new.path.clone(),
            ChangeKind::TypeChanged,
            Some(old),
            Some(new),
        )
    }

    fn from_entries(
        path: PathBuf,
        kind: ChangeKind,
        old: Option<&IndexEntry>,
        new: Option<&IndexEntry>,
    ) -> Self {
        let effective = new.or(old);
        let mode = new.map(|entry| entry.mode);
        let is_submodule = effective.is_some_and(|entry| entry.mode == Mode::COMMIT);
        Self {
            path,
            kind,
            object_id: new.map(|entry| entry.id),
            mode,
            is_submodule,
        }
    }

    fn has_staged_blob(&self) -> bool {
        matches!(
            self.kind,
            ChangeKind::Added | ChangeKind::Modified | ChangeKind::TypeChanged
        ) && self.object_id.is_some()
    }

    fn mode_has_blob(&self) -> bool {
        self.mode
            .is_some_and(|mode| matches!(mode, Mode::FILE | Mode::FILE_EXECUTABLE | Mode::SYMLINK))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeKind {
    Added,
    Modified,
    Deleted,
    TypeChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckPlan {
    has_docs: bool,
    has_rust: bool,
    has_web: bool,
    large_files: usize,
    submodules: usize,
}

impl CheckPlan {
    fn from_changes(changes: &[StagedChange]) -> Self {
        let mut has_docs = false;
        let mut has_rust = false;
        let mut has_web = false;
        let mut large_files = 0usize;
        let mut submodules = 0usize;

        for change in changes {
            if change.is_submodule {
                submodules = submodules.saturating_add(1);
                continue;
            }
            if is_docs_policy_path(&change.path) {
                has_docs = true;
            }
            if is_rust_affecting_path(&change.path) {
                has_rust = true;
            }
            if is_web_affecting_path(&change.path) {
                has_web = true;
            }
            if change.has_staged_blob()
                && change.mode_has_blob()
                && is_source_file_path(&change.path)
            {
                large_files = large_files.saturating_add(1);
            }
        }

        Self {
            has_docs,
            has_rust,
            has_web,
            large_files,
            submodules,
        }
    }

    fn summary(&self) -> String {
        format!(
            "staged: rust={} web={} docs={} large_files={} submodules={}",
            usize::from(self.has_rust),
            usize::from(self.has_web),
            usize::from(self.has_docs),
            self.large_files,
            self.submodules,
        )
    }
}

fn is_docs_policy_path(path: &Path) -> bool {
    if path == Path::new(".gitmodules") {
        return true;
    }
    path.file_name().is_some_and(|file_name| {
        matches!(
            file_name.to_str(),
            Some("README.md" | "AGENTS.md" | "CLAUDE.md")
        )
    })
}

fn is_rust_affecting_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "rs")
        || path.file_name().is_some_and(|file_name| {
            matches!(file_name.to_str(), Some("Cargo.toml" | "Cargo.lock"))
        })
}

fn is_web_affecting_path(path: &Path) -> bool {
    if !path.starts_with("frontends/web") {
        return false;
    }
    if path.file_name().is_some_and(|file_name| {
        matches!(
            file_name.to_str(),
            Some(
                "package.json"
                    | "bun.lock"
                    | "tsconfig.json"
                    | "next.config.js"
                    | "next.config.mjs"
                    | "next.config.ts"
                    | "playwright.config.ts"
                    | "playwright.rehearsal.config.ts"
                    | "biome.json"
                    | "postcss.config.js"
                    | "postcss.config.mjs"
                    | "tailwind.config.js"
                    | "tailwind.config.ts"
            )
        )
    }) {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "css" | "scss" | "sass"
            )
        })
}

async fn run_human_agent_docs_check(
    repo_root: PathBuf,
    enabled: bool,
    cancel: Arc<AtomicBool>,
) -> CheckOutcome {
    if !enabled {
        return CheckOutcome::skip("human/agent docs", "no staged instruction docs");
    }
    match tokio::task::spawn_blocking(move || {
        let config = DocCheckConfig::new(repo_root);
        scan_human_agent_docs_with_cancel(&config, Some(&cancel))
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
    blobs: Vec<SourceBlob>,
    threshold: usize,
    watch_threshold: usize,
    expected_files: usize,
) -> CheckOutcome {
    if expected_files == 0 {
        return CheckOutcome::skip("large files", "no staged source blobs");
    }
    let watch_threshold = if watch_threshold == 0 {
        None
    } else {
        Some(watch_threshold)
    };
    let config = match BlobScanConfig::new(blobs, threshold, watch_threshold) {
        Ok(config) => config,
        Err(error) => return CheckOutcome::error("large files", error.to_string()),
    };
    match tokio::task::spawn_blocking(move || scan_large_file_blobs(&config)).await {
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
        return vec![CheckOutcome::skip(
            "rust checks",
            "no staged Rust-affecting files",
        )];
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
        return CheckOutcome::skip("web lint", "no staged frontend files");
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

#[derive(Debug, Clone)]
struct PreCommitSummary {
    staged: Option<String>,
    outcomes: Vec<CheckOutcome>,
}

impl PreCommitSummary {
    fn empty() -> Self {
        Self {
            staged: None,
            outcomes: Vec::new(),
        }
    }

    fn print(&self) {
        if let Some(staged) = &self.staged {
            println!("[{PREFIX}] {staged}");
        } else {
            println!("[{PREFIX}] no staged changes");
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

#[derive(Debug, Error)]
enum PreCommitError {
    #[error("failed to discover Git repository from {path}: {source}")]
    Discover {
        path: PathBuf,
        source: Box<gix::discover::Error>,
    },
    #[error("pre-commit requires a non-bare repository with a worktree")]
    BareRepository,
    #[error("failed to resolve repository root {path}: {source}")]
    ResolveRoot {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to resolve HEAD tree: {source}")]
    HeadTree {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("failed to build HEAD index: {source}")]
    HeadIndex {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("failed to read staged index: {source}")]
    Index {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error(
        "index contains unresolved conflict entry for {path:?}; resolve conflicts before committing"
    )]
    UnresolvedConflict { path: PathBuf },
    #[error("index contains duplicate path {path:?}")]
    DuplicateIndexPath { path: PathBuf },
    #[error("index path is not UTF-8: {path:?}: {source}")]
    NonUtf8Path {
        path: Vec<u8>,
        source: std::str::Utf8Error,
    },
    #[error("staged source file {path:?} has no blob id")]
    MissingBlobId { path: PathBuf },
    #[error("failed to read staged blob for {path:?}: {source}")]
    Blob {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentics_dev_checks::large_files::report_passed as large_file_report_passed;
    use std::fs;
    use std::process::Command as StdCommand;

    fn change(path: &str, kind: ChangeKind, is_submodule: bool) -> StagedChange {
        StagedChange {
            path: PathBuf::from(path),
            kind,
            object_id: None,
            mode: Some(Mode::FILE),
            is_submodule,
        }
    }

    fn blob_change(path: &str) -> StagedChange {
        StagedChange {
            path: PathBuf::from(path),
            kind: ChangeKind::Added,
            object_id: Some(gix::ObjectId::empty_blob(gix::hash::Kind::Sha1)),
            mode: Some(Mode::FILE),
            is_submodule: false,
        }
    }

    #[test]
    fn staged_file_classification_detects_rust_and_frontend_work() {
        let staged = vec![
            change("crates/domain/src/lib.rs", ChangeKind::Modified, false),
            change(
                "frontends/web/src/app/page.tsx",
                ChangeKind::Modified,
                false,
            ),
        ];

        let plan = CheckPlan::from_changes(&staged);

        assert!(plan.has_rust);
        assert!(plan.has_web);
        assert!(!plan.has_docs);
    }

    #[test]
    fn staged_file_classification_detects_cargo_files() {
        let manifest = CheckPlan::from_changes(&[change(
            "crates/domain/Cargo.toml",
            ChangeKind::Modified,
            false,
        )]);
        let lock = CheckPlan::from_changes(&[change("Cargo.lock", ChangeKind::Modified, false)]);

        assert!(manifest.has_rust);
        assert!(lock.has_rust);
    }

    #[test]
    fn staged_file_classification_detects_frontend_config() {
        let plan = CheckPlan::from_changes(&[change(
            "frontends/web/playwright.config.ts",
            ChangeKind::Modified,
            false,
        )]);

        assert!(plan.has_web);
    }

    #[test]
    fn staged_file_classification_detects_docs_policy_files() {
        for path in [
            "README.md",
            "docs/AGENTS.md",
            "docs/CLAUDE.md",
            ".gitmodules",
        ] {
            let plan = CheckPlan::from_changes(&[change(path, ChangeKind::Modified, false)]);
            assert!(plan.has_docs, "{path}");
        }
    }

    #[test]
    fn staged_file_classification_ignores_ordinary_markdown_for_docs_policy() {
        let plan = CheckPlan::from_changes(&[change("docs/notes.md", ChangeKind::Modified, false)]);

        assert!(!plan.has_docs);
    }

    #[test]
    fn staged_file_classification_treats_submodule_as_single_pointer() {
        let plan = CheckPlan::from_changes(&[change(
            "challenge-repos/agentics-challenges",
            ChangeKind::Modified,
            true,
        )]);

        assert_eq!(plan.submodules, 1);
        assert!(!plan.has_rust);
        assert!(!plan.has_web);
        assert!(!plan.has_docs);
        assert_eq!(plan.large_files, 0);
    }

    #[test]
    fn staged_file_classification_counts_source_blobs_for_large_file_check() {
        let plan = CheckPlan::from_changes(&[
            blob_change("ops/pre-commit/src/main.rs"),
            change("ops/pre-commit/src/old.rs", ChangeKind::Deleted, false),
        ]);

        assert_eq!(plan.large_files, 1);
        assert!(plan.has_rust);
    }

    #[test]
    fn summary_exit_code_distinguishes_failures_from_infra_errors() {
        let passing = PreCommitSummary {
            staged: Some("staged: rust=0 web=0 docs=0 large_files=0 submodules=0".to_string()),
            outcomes: vec![
                CheckOutcome::pass("ok", Vec::new()),
                CheckOutcome::skip("skip", "not applicable"),
            ],
        };
        assert_eq!(passing.exit_code(), ExitCode::SUCCESS);

        let failing = PreCommitSummary {
            staged: None,
            outcomes: vec![CheckOutcome::fail("bad", Vec::new())],
        };
        assert_eq!(failing.exit_code(), ExitCode::from(1));

        let errored = PreCommitSummary {
            staged: None,
            outcomes: vec![CheckOutcome::error("tool", "missing".to_string())],
        };
        assert_eq!(errored.exit_code(), ExitCode::from(2));
    }

    #[test]
    fn gix_staged_changes_read_index_not_worktree() {
        let repo_dir = initialized_repo();
        let source = repo_dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().expect("source parent")).expect("create source dir");
        fs::write(&source, "fn original() {}\n").expect("write original");
        git(repo_dir.path(), &["add", "src/lib.rs"]);
        git(repo_dir.path(), &["commit", "-m", "initial"]);

        fs::write(&source, "fn staged() {}\n").expect("write staged");
        git(repo_dir.path(), &["add", "src/lib.rs"]);
        fs::write(&source, "fn unstaged() {}\n".repeat(2000)).expect("write unstaged");

        let repo = StagedRepository::open(repo_dir.path()).expect("open repo");
        let changes = repo.staged_changes().expect("staged changes");
        let blobs = repo.source_blobs(&changes).expect("source blobs");
        let change = changes.first().expect("one change");
        let blob = blobs.first().expect("one blob");

        assert_eq!(changes.len(), 1);
        assert_eq!(change.kind, ChangeKind::Modified);
        assert_eq!(blobs.len(), 1);
        assert_eq!(blob.path(), Path::new("src/lib.rs"));
        assert_eq!(blob.content(), b"fn staged() {}\n");

        let config = BlobScanConfig::new(blobs, 100, Some(50)).expect("valid config");
        let report = scan_large_file_blobs(&config).expect("scan blobs");
        assert!(large_file_report_passed(&report));
    }

    #[test]
    fn gix_staged_changes_treat_unborn_head_entries_as_added() {
        let repo_dir = tempfile::tempdir().expect("create temp repo");
        git(repo_dir.path(), &["init"]);
        fs::write(
            repo_dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .expect("write manifest");
        git(repo_dir.path(), &["add", "Cargo.toml"]);

        let repo = StagedRepository::open(repo_dir.path()).expect("open repo");
        let changes = repo.staged_changes().expect("staged changes");
        let change = changes.first().expect("one change");

        assert_eq!(changes.len(), 1);
        assert_eq!(change.kind, ChangeKind::Added);
        assert_eq!(change.path, PathBuf::from("Cargo.toml"));
    }

    fn initialized_repo() -> tempfile::TempDir {
        let repo_dir = tempfile::tempdir().expect("create temp repo");
        git(repo_dir.path(), &["init"]);
        git(repo_dir.path(), &["config", "user.name", "Agentics Test"]);
        git(
            repo_dir.path(),
            &["config", "user.email", "agentics@example.test"],
        );
        repo_dir
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = StdCommand::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("start git");
        if !output.status.success() {
            panic!(
                "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
                args,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
