//! Large source-file scanner for Agentics code reviews.
//!
//! This executable is intentionally Rust-native: it walks the repository with
//! the `ignore` crate, counts lines without shelling out to `git`, `find`, or
//! `wc`, and reports deterministic results for review logs. It is read-only,
//! idempotent, has no rollback or dry-run mode because it never mutates state,
//! and uses the shared Ctrl-C handling wrapper for cancellation.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use ignore::WalkBuilder;
use thiserror::Error;

use crate::support::{INTERRUPTED_EXIT, run_with_ctrl_c};

const PREFIX: &str = "agentics-large-files";

/// Default line count where a source file requires refactor review.
pub const DEFAULT_LINE_THRESHOLD: usize = 1_200;

/// Default line count where a source file should be watched for decomposition.
pub const DEFAULT_WATCH_THRESHOLD: usize = 900;

const CODE_EXTENSIONS: &[&str] = &[
    "bash", "cjs", "css", "html", "js", "jsx", "mjs", "py", "rs", "sass", "scss", "sh", "sql",
    "toml", "ts", "tsx", "vue", "yaml", "yml", "zsh",
];

const CODE_BASENAMES: &[&str] = &["Dockerfile", "Justfile", "Makefile", "Rakefile", "justfile"];

const EXCLUDED_COMPONENTS: &[&str] = &[
    ".git",
    ".next",
    ".turbo",
    "build",
    "challenge-repos",
    "coverage",
    "dist",
    "generated",
    "node_modules",
    "target",
];

const EXCLUDED_FILENAMES: &[&str] = &[
    "Cargo.lock",
    "bun.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
];

/// CLI for scanning large source files.
#[derive(Debug, Parser)]
#[command(
    about = "Scans Agentics code files for oversized modules.",
    long_about = "Walks the repository with gitignore-aware native Rust code, skips generated/build/lock artifacts, reports code files at or above the refactor threshold, and prints a lower watch list for files nearing the limit."
)]
pub struct Cli {
    /// Repository root to scan.
    #[arg(long, default_value = ".")]
    root: PathBuf,

    /// Line count at or above which a code file requires refactor review.
    #[arg(long, default_value_t = DEFAULT_LINE_THRESHOLD)]
    threshold: usize,

    /// Lower line count for the watch list. Use 0 to disable watch output.
    #[arg(long, default_value_t = DEFAULT_WATCH_THRESHOLD)]
    watch_threshold: usize,
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

fn run(cli: Cli) -> Result<ScanReport, LargeFileScanError> {
    let config = ScanConfig::from_cli(cli)?;
    scan_large_files(&config)
}

/// Scanner configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanConfig {
    root: PathBuf,
    threshold: usize,
    watch_threshold: Option<usize>,
}

impl ScanConfig {
    fn from_cli(cli: Cli) -> Result<Self, LargeFileScanError> {
        let watch_threshold = if cli.watch_threshold == 0 {
            None
        } else {
            Some(cli.watch_threshold)
        };
        Self::new(cli.root, cli.threshold, watch_threshold)
    }

    /// Build scanner configuration from explicit inputs.
    pub fn new(
        root: PathBuf,
        threshold: usize,
        watch_threshold: Option<usize>,
    ) -> Result<Self, LargeFileScanError> {
        if threshold == 0 {
            return Err(LargeFileScanError::InvalidConfig(
                "threshold must be greater than zero".to_string(),
            ));
        }
        if let Some(watch_threshold) = watch_threshold
            && watch_threshold >= threshold
        {
            return Err(LargeFileScanError::InvalidConfig(format!(
                "watch threshold {} must be lower than threshold {}",
                watch_threshold, threshold
            )));
        }
        Ok(Self {
            root,
            threshold,
            watch_threshold,
        })
    }
}

/// Full scan result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    scanned_files: usize,
    threshold: usize,
    watch_threshold: Option<usize>,
    oversized: Vec<FileLineCount>,
    watch: Vec<FileLineCount>,
}

/// One code file and its line count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLineCount {
    path: PathBuf,
    lines: usize,
}

/// Errors returned by the large-file scanner.
#[derive(Debug, Error)]
pub enum LargeFileScanError {
    /// The requested scanner configuration is invalid.
    #[error("invalid scan config: {0}")]
    InvalidConfig(String),

    /// The requested scan root could not be resolved.
    #[error("failed to resolve scan root {path}: {source}")]
    ResolveRoot {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The source tree walk failed.
    #[error("failed to walk source tree: {0}")]
    Walk(#[from] ignore::Error),

    /// A code file could not be read.
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    /// A line counter overflowed.
    #[error("line count overflowed while reading {0}")]
    LineCountOverflow(PathBuf),
}

/// Scan for large code files.
pub fn scan_large_files(config: &ScanConfig) -> Result<ScanReport, LargeFileScanError> {
    let root =
        std::fs::canonicalize(&config.root).map_err(|source| LargeFileScanError::ResolveRoot {
            path: config.root.clone(),
            source,
        })?;
    let mut oversized = Vec::new();
    let mut watch = Vec::new();
    let mut scanned_files = 0usize;

    for entry in WalkBuilder::new(&root).parents(true).build() {
        let entry = entry?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.path();
        let relative_path = relative_path(&root, path);
        if should_skip(&relative_path) || !is_code_file(&relative_path) {
            continue;
        }

        scanned_files = scanned_files
            .checked_add(1)
            .ok_or_else(|| LargeFileScanError::LineCountOverflow(relative_path.clone()))?;
        let lines = count_lines(path).map_err(|source| LargeFileScanError::ReadFile {
            path: relative_path.clone(),
            source,
        })?;
        let file = FileLineCount {
            path: relative_path,
            lines,
        };
        if lines >= config.threshold {
            oversized.push(file);
        } else if config
            .watch_threshold
            .is_some_and(|watch_threshold| lines >= watch_threshold)
        {
            watch.push(file);
        }
    }

    sort_counts(&mut oversized);
    sort_counts(&mut watch);
    Ok(ScanReport {
        scanned_files,
        threshold: config.threshold,
        watch_threshold: config.watch_threshold,
        oversized,
        watch,
    })
}

/// Whether a large-file report passes the hard threshold.
pub fn report_passed(report: &ScanReport) -> bool {
    report.oversized.is_empty()
}

/// Render a large-file report into deterministic output lines.
pub fn render_report(report: &ScanReport) -> Vec<String> {
    let mut lines = Vec::new();
    if report.oversized.is_empty() {
        lines.push(format!(
            "[{PREFIX}] PASS scanned {} code files; no files at or above {} lines",
            report.scanned_files, report.threshold
        ));
    } else {
        lines.push(format!(
            "[{PREFIX}] FAIL {} code files are at or above {} lines",
            report.oversized.len(),
            report.threshold
        ));
        for file in &report.oversized {
            lines.push(format!(
                "[{PREFIX}] FAIL {:>5} {}",
                file.lines,
                display_path(&file.path)
            ));
        }
    }

    if let Some(watch_threshold) = report.watch_threshold
        && !report.watch.is_empty()
    {
        lines.push(format!(
            "[{PREFIX}] WARN {} code files are between {} and {} lines",
            report.watch.len(),
            watch_threshold,
            report.threshold
        ));
        for file in &report.watch {
            lines.push(format!(
                "[{PREFIX}] WARN {:>5} {}",
                file.lines,
                display_path(&file.path)
            ));
        }
    }

    lines
}

fn print_report(report: &ScanReport) -> ExitCode {
    for line in render_report(report) {
        println!("{line}");
    }

    if report_passed(report) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn sort_counts(files: &mut [FileLineCount]) {
    files.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

fn count_lines(path: &Path) -> Result<usize, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let mut lines = 0usize;
    loop {
        buffer.clear();
        let bytes_read = reader.read_until(b'\n', &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        lines = lines.checked_add(1).ok_or_else(|| {
            std::io::Error::other(format!("line count overflowed for {}", path.display()))
        })?;
    }
    Ok(lines)
}

fn should_skip(path: &Path) -> bool {
    has_excluded_component(path) || has_excluded_filename(path)
}

fn has_excluded_component(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        EXCLUDED_COMPONENTS.iter().any(|excluded| name == *excluded)
    })
}

fn has_excluded_filename(path: &Path) -> bool {
    let Some(file_name) = path.file_name() else {
        return false;
    };
    EXCLUDED_FILENAMES
        .iter()
        .any(|excluded| file_name == *excluded)
}

fn is_code_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name() else {
        return false;
    };
    if CODE_BASENAMES.iter().any(|basename| file_name == *basename) {
        return true;
    }

    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    CODE_EXTENSIONS
        .iter()
        .any(|code_extension| extension.eq_ignore_ascii_case(code_extension))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_file_detection_skips_generated_and_locks() {
        assert!(is_code_file(Path::new("backend/shared/src/lib.rs")));
        assert!(is_code_file(Path::new("frontends/web/src/app/globals.css")));
        assert!(is_code_file(Path::new("Dockerfile")));
        assert!(should_skip(Path::new(
            "frontends/web/src/lib/generated/schemas.ts"
        )));
        assert!(should_skip(Path::new("Cargo.lock")));
        assert!(!is_code_file(Path::new("docs/PRD/en.md")));
    }

    #[test]
    fn scan_reports_oversized_and_watch_files_deterministically() {
        let temp = tempfile::tempdir().expect("create temp repo");
        let source_root = temp.path().join("src");
        std::fs::create_dir_all(&source_root).expect("create source dir");
        write_lines(&source_root.join("small.rs"), 1);
        write_lines(&source_root.join("watch.ts"), 2);
        write_lines(&source_root.join("large.rs"), 3);
        write_lines(&source_root.join("larger.rs"), 4);
        std::fs::create_dir_all(source_root.join("generated")).expect("create generated dir");
        write_lines(&source_root.join("generated").join("huge.rs"), 10);

        let config = ScanConfig::new(temp.path().to_path_buf(), 3, Some(2)).expect("valid config");
        let report = scan_large_files(&config).expect("scan temp repo");

        assert_eq!(report.scanned_files, 4);
        assert_eq!(
            report.oversized,
            vec![
                FileLineCount {
                    path: PathBuf::from("src/larger.rs"),
                    lines: 4,
                },
                FileLineCount {
                    path: PathBuf::from("src/large.rs"),
                    lines: 3,
                },
            ]
        );
        assert_eq!(
            report.watch,
            vec![FileLineCount {
                path: PathBuf::from("src/watch.ts"),
                lines: 2,
            }]
        );
    }

    fn write_lines(path: &Path, line_count: usize) {
        let mut text = String::new();
        for _ in 0..line_count {
            text.push_str("line\n");
        }
        std::fs::write(path, text).expect("write source file");
    }
}
