//! Submit checked-in challenge baseline solutions to an Agentics deployment.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentics_contracts::validation::archive::NormalizedArchivePath;
use agentics_contracts::zip_project::{
    MAX_ZIP_PROJECT_ARTIFACT_BYTES, MAX_ZIP_PROJECT_FILE_COUNT, MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, validate_zip_project_archive_envelope,
};
use agentics_domain::models::challenge::{ChallengeDetailResponse, ChallengeListResponse};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse, SolutionSubmissionResponse,
};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use clap::Parser;
use ignore::{DirEntry, WalkBuilder};
use reqwest::{Client, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, sleep};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

const DEFAULT_API_BASE_URL: &str = "https://agentics.reify.ing";
const DEFAULT_CHALLENGE_REPO: &str = "challenge-repos/agentics-challenges";
const DEFAULT_STATE_FILE: &str = "target/agentics-baseline-submissions.jsonl";
const DEFAULT_DELAY_SECS: u64 = 5;
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 900;
const DEFAULT_POLL_SECS: u64 = 5;
const DEFAULT_EXPLANATION: &str =
    "Agentics official baseline submission from the checked-in challenge test solution.";
const DEFAULT_CREDIT_TEXT: &str = "Agentics official baseline";
const REQUIRED_MANIFEST: &str = ZIP_PROJECT_MANIFEST_FILE;

const DEFAULT_DEFERRED_CHALLENGES: &[&str] = &[
    "adaptive-impostor-search-frontier-cs-algorithmic-245",
    "average-permutation-frontier-cs-algorithmic-124",
    "distinct-bakery-types-frontier-cs-algorithmic-141",
    "editor-width-discovery-frontier-cs-algorithmic-122",
    "heap-tree-sum-frontier-cs-algorithmic-209",
    "imagenet-200k-frontier-cs-imagenet-200k",
    "imagenet-500k-frontier-cs-imagenet-500k",
    "imagenet-1m-frontier-cs-imagenet-1m",
    "imagenet-2-5m-frontier-cs-imagenet-2-5m",
    "imagenet-5m-frontier-cs-imagenet-5m",
    "limited-shuffle-restore-frontier-cs-algorithmic-59",
    "line-recovery-frontier-cs-algorithmic-117",
    "llm-sql-small-frontier-cs-llm-sql-small",
    "llm-sql-large-frontier-cs-llm-sql-large",
    "palindromic-grid-paths-frontier-cs-algorithmic-256",
    "poker-action-seeds-frontier-cs-algorithmic-143",
    "repaired-road-set-frontier-cs-algorithmic-253",
    "snake-path-minima-frontier-cs-algorithmic-233",
    "sorted-mode-array-frontier-cs-algorithmic-257",
    "symreg-sincos-frontier-cs-symreg-sincos",
    "symreg-mccormick-frontier-cs-symreg-mccormick",
    "symreg-mixed-polyexp-frontier-cs-symreg-mixed-polyexp",
    "symreg-peaks-frontier-cs-symreg-peaks",
    "symreg-ripple-frontier-cs-symreg-ripple",
    "treasure-hunt-choices-frontier-cs-algorithmic-70",
    "uniform-cave-explorer-frontier-cs-algorithmic-80",
    "world-map-frontier-cs-algorithmic-6",
];

/// CLI for the production baseline submitter.
#[derive(Debug, Parser)]
#[command(about = "Submit checked-in challenge baseline solutions to Agentics")]
pub struct SubmitBaselinesArgs {
    /// Agentics API base URL.
    #[arg(long, default_value = DEFAULT_API_BASE_URL)]
    api_base_url: String,
    /// Local agentics-challenges repository root.
    #[arg(long, default_value = DEFAULT_CHALLENGE_REPO)]
    challenge_repo: PathBuf,
    /// JSONL state file used for resumable submissions.
    #[arg(long, default_value = DEFAULT_STATE_FILE)]
    state_file: PathBuf,
    /// Read AGENTICS agent bearer token from stdin.
    #[arg(long)]
    token_stdin: bool,
    /// Submit only these challenge names.
    #[arg(long = "challenge")]
    challenges: Vec<ChallengeName>,
    /// Submit only these target names.
    #[arg(long = "target")]
    targets: Vec<TargetName>,
    /// Include challenges that are deferred by the baseline audit.
    #[arg(long)]
    include_deferred: bool,
    /// Additional newline-delimited challenge names to include.
    #[arg(long)]
    allowlist_file: Option<PathBuf>,
    /// Additional newline-delimited challenge names to defer.
    #[arg(long)]
    deferlist_file: Option<PathBuf>,
    /// Resubmit targets that already completed in the state file.
    #[arg(long)]
    resubmit: bool,
    /// Print planned submissions without sending requests.
    #[arg(long)]
    dry_run: bool,
    /// Maximum number of challenge-target pairs to submit.
    #[arg(long)]
    limit: Option<usize>,
    /// Delay between terminal submissions.
    #[arg(long, default_value_t = DEFAULT_DELAY_SECS)]
    delay_secs: u64,
    /// Timeout while waiting for one submission.
    #[arg(long, default_value_t = DEFAULT_WAIT_TIMEOUT_SECS)]
    wait_timeout_secs: u64,
    /// Poll interval while waiting for one submission.
    #[arg(long, default_value_t = DEFAULT_POLL_SECS)]
    poll_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct CliConfig {
    token: Option<String>,
}

#[derive(Debug, Clone)]
struct SolutionPackage {
    bytes: Vec<u8>,
    file_count: usize,
    uncompressed_bytes: u64,
}

#[derive(Debug, Clone)]
struct PackageFile {
    path: PathBuf,
    archive_name: String,
    unix_permissions: u32,
}

#[derive(Debug, Clone, Copy)]
struct PackageLimits {
    max_zip_bytes: u64,
    max_file_count: usize,
    max_uncompressed_bytes: u64,
}

impl PackageLimits {
    const DEFAULT: Self = Self {
        max_zip_bytes: MAX_ZIP_PROJECT_ARTIFACT_BYTES,
        max_file_count: MAX_ZIP_PROJECT_FILE_COUNT,
        max_uncompressed_bytes: MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    };
}

#[derive(Debug, Clone)]
struct CollectedPackageFiles {
    files: Vec<PackageFile>,
    uncompressed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaselineStateRecord {
    challenge_name: ChallengeName,
    target: TargetName,
    solution_path: PathBuf,
    submission_id: Option<SolutionSubmissionId>,
    status: String,
    note: String,
    recorded_at_unix_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BaselineKey {
    challenge_name: ChallengeName,
    target: TargetName,
}

impl BaselineKey {
    fn new(challenge_name: ChallengeName, target: TargetName) -> Self {
        Self {
            challenge_name,
            target,
        }
    }
}

#[derive(Debug, Default)]
struct RunSummary {
    planned: usize,
    submitted: usize,
    completed: usize,
    failed: usize,
    skipped: usize,
    deferred: usize,
}

impl RunSummary {
    fn increment_planned(&mut self) -> Result<()> {
        self.planned = checked_increment(self.planned)?;
        Ok(())
    }

    fn increment_submitted(&mut self) -> Result<()> {
        self.submitted = checked_increment(self.submitted)?;
        Ok(())
    }

    fn increment_completed(&mut self) -> Result<()> {
        self.completed = checked_increment(self.completed)?;
        Ok(())
    }

    fn increment_failed(&mut self) -> Result<()> {
        self.failed = checked_increment(self.failed)?;
        Ok(())
    }

    fn increment_skipped(&mut self) -> Result<()> {
        self.skipped = checked_increment(self.skipped)?;
        Ok(())
    }

    fn increment_deferred(&mut self) -> Result<()> {
        self.deferred = checked_increment(self.deferred)?;
        Ok(())
    }

    fn print(&self) {
        println!(
            "summary: planned={}, submitted={}, completed={}, failed={}, skipped={}, deferred={}",
            self.planned, self.submitted, self.completed, self.failed, self.skipped, self.deferred
        );
    }
}

/// Run this command from process args and env.
pub async fn run_from_process() -> std::process::ExitCode {
    match run(SubmitBaselinesArgs::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(args: SubmitBaselinesArgs) -> Result<()> {
    let api_base_url = Url::parse(&args.api_base_url).context("parse API base URL")?;
    let token = if args.dry_run {
        None
    } else {
        Some(resolve_token(args.token_stdin)?)
    };
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build HTTP client")?;
    let challenge_repo = args
        .challenge_repo
        .canonicalize()
        .with_context(|| format!("resolve challenge repo {}", args.challenge_repo.display()))?;
    let solution_root = challenge_repo.join("test-solutions");
    let allowlist = name_set_from_args(&args.challenges, args.allowlist_file.as_deref())?;
    let deferlist = build_deferlist(args.include_deferred, args.deferlist_file.as_deref())?;
    let target_filter = args.targets.iter().cloned().collect::<BTreeSet<_>>();
    let mut state = load_state(&args.state_file)?;
    let challenges = list_challenges(&client, &api_base_url).await?;
    let mut submitted_count = 0usize;
    let mut summary = RunSummary::default();

    'challenge_loop: for challenge_name in challenges {
        if !allowlist.is_empty() && !allowlist.contains(&challenge_name) {
            continue;
        }
        if deferlist.contains(&challenge_name) {
            println!("skip {challenge_name}: deferred by baseline audit");
            summary.increment_deferred()?;
            continue;
        }
        let solution_dir = solution_root.join(challenge_name.as_str());
        if !solution_dir.is_dir() {
            println!("skip {challenge_name}: no checked-in test solution");
            summary.increment_skipped()?;
            continue;
        }
        if !args.include_deferred
            && let Some(reason) = solution_defer_marker(&solution_dir)?
        {
            println!("skip {challenge_name}: deferred by solution metadata ({reason})");
            summary.increment_deferred()?;
            continue;
        }

        let detail = get_challenge(&client, &api_base_url, &challenge_name).await?;
        let selected_targets = select_targets(&detail, &target_filter)?;
        if selected_targets.is_empty() {
            println!("skip {challenge_name}: no selected targets");
            summary.increment_skipped()?;
            continue;
        }

        let package = package_solution_workspace(&solution_dir)
            .with_context(|| format!("package solution for {challenge_name}"))?;
        for target in selected_targets {
            if let Some(limit) = args.limit
                && submitted_count >= limit
            {
                break 'challenge_loop;
            }
            let key = BaselineKey::new(challenge_name.clone(), target.clone());
            if !args.resubmit
                && state
                    .get(&key)
                    .is_some_and(|record| record.status == "completed")
            {
                println!("skip {challenge_name}/{target}: already completed in state file");
                summary.increment_skipped()?;
                continue;
            }
            if args.dry_run {
                println!(
                    "dry-run submit {challenge_name}/{target}: {} files, {} uncompressed bytes, {} zip bytes",
                    package.file_count,
                    package.uncompressed_bytes,
                    package.bytes.len()
                );
                summary.increment_planned()?;
                submitted_count = checked_increment(submitted_count)?;
                continue;
            }

            let token = token
                .as_ref()
                .context("internal error: token must be present outside dry-run")?;
            let response = match create_submission(
                &client,
                &api_base_url,
                token,
                &challenge_name,
                &target,
                &package,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    let note = error.to_string();
                    println!("failed submit {challenge_name}/{target}: {note}");
                    let record = BaselineStateRecord {
                        challenge_name: challenge_name.clone(),
                        target: target.clone(),
                        solution_path: solution_dir.clone(),
                        submission_id: None,
                        status: "failed_to_submit".to_string(),
                        note,
                        recorded_at_unix_secs: now_unix_secs()?,
                    };
                    append_state(&args.state_file, &record)?;
                    state.insert(key, record);
                    summary.increment_failed()?;
                    submitted_count = checked_increment(submitted_count)?;
                    sleep(Duration::from_secs(args.delay_secs)).await;
                    continue;
                }
            };
            summary.increment_submitted()?;
            println!("submitted {challenge_name}/{target}: {}", response.id);
            append_state(
                &args.state_file,
                &BaselineStateRecord {
                    challenge_name: challenge_name.clone(),
                    target: target.clone(),
                    solution_path: solution_dir.clone(),
                    submission_id: Some(response.id.clone()),
                    status: response.status.as_str().to_string(),
                    note: response.note.clone(),
                    recorded_at_unix_secs: now_unix_secs()?,
                },
            )?;

            let final_response = match wait_submission(
                &client,
                &api_base_url,
                token,
                &response.id,
                Duration::from_secs(args.wait_timeout_secs),
                Duration::from_secs(args.poll_secs),
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    let note = error.to_string();
                    println!("failed wait {challenge_name}/{target}: {note}");
                    let record = BaselineStateRecord {
                        challenge_name: challenge_name.clone(),
                        target: target.clone(),
                        solution_path: solution_dir.clone(),
                        submission_id: Some(response.id.clone()),
                        status: "failed_to_wait".to_string(),
                        note,
                        recorded_at_unix_secs: now_unix_secs()?,
                    };
                    append_state(&args.state_file, &record)?;
                    state.insert(key, record);
                    summary.increment_failed()?;
                    submitted_count = checked_increment(submitted_count)?;
                    sleep(Duration::from_secs(args.delay_secs)).await;
                    continue;
                }
            };
            println!(
                "finished {challenge_name}/{target}: {} ({})",
                final_response.id, final_response.status
            );
            let final_record = BaselineStateRecord {
                challenge_name: challenge_name.clone(),
                target: target.clone(),
                solution_path: solution_dir.clone(),
                submission_id: Some(final_response.id.clone()),
                status: final_response.status.as_str().to_string(),
                note: final_response.note.clone(),
                recorded_at_unix_secs: now_unix_secs()?,
            };
            append_state(&args.state_file, &final_record)?;
            state.insert(key, final_record);
            if matches!(
                final_response.status,
                agentics_domain::models::evaluation::SolutionSubmissionStatus::Completed
            ) {
                summary.increment_completed()?;
            } else {
                summary.increment_failed()?;
            }
            submitted_count = checked_increment(submitted_count)?;
            sleep(Duration::from_secs(args.delay_secs)).await;
        }
    }

    summary.print();
    Ok(())
}

fn checked_increment(value: usize) -> Result<usize> {
    value
        .checked_add(1)
        .context("submission counter overflowed")
}

fn resolve_token(token_stdin: bool) -> Result<SecretString> {
    if token_stdin {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .context("read agent token from stdin")?;
        return secret_from_optional(input.trim(), "stdin token");
    }
    if let Ok(value) = std::env::var("AGENTICS_TOKEN") {
        return secret_from_optional(value.trim(), "AGENTICS_TOKEN");
    }
    let config_path = cli_config_path()?;
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read CLI config {}", config_path.display()))?;
    let config = toml::from_str::<CliConfig>(&raw)
        .with_context(|| format!("parse CLI config {}", config_path.display()))?;
    let token = config
        .token
        .context("no agent token in --token-stdin, AGENTICS_TOKEN, or CLI config")?;
    secret_from_optional(token.trim(), "CLI config token")
}

fn secret_from_optional(value: &str, source: &str) -> Result<SecretString> {
    if value.is_empty() {
        bail!("{source} is empty");
    }
    Ok(SecretString::from(value.to_string()))
}

fn cli_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("resolve user config directory")?;
    Ok(config_dir.join("agentics").join("config.toml"))
}

fn name_set_from_args(
    names: &[ChallengeName],
    file: Option<&Path>,
) -> Result<BTreeSet<ChallengeName>> {
    let mut set = names.iter().cloned().collect::<BTreeSet<_>>();
    if let Some(file) = file {
        set.extend(read_name_set_file(file)?);
    }
    Ok(set)
}

fn build_deferlist(include_deferred: bool, file: Option<&Path>) -> Result<BTreeSet<ChallengeName>> {
    let mut set = BTreeSet::new();
    if !include_deferred {
        for name in DEFAULT_DEFERRED_CHALLENGES {
            set.insert((*name).parse::<ChallengeName>()?);
        }
    }
    if let Some(file) = file {
        set.extend(read_name_set_file(file)?);
    }
    Ok(set)
}

fn read_name_set_file(path: &Path) -> Result<BTreeSet<ChallengeName>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read challenge name list {}", path.display()))?;
    let mut set = BTreeSet::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        set.insert(trimmed.parse::<ChallengeName>()?);
    }
    Ok(set)
}

fn solution_defer_marker(solution_dir: &Path) -> Result<Option<String>> {
    let manifest_path = solution_dir.join(REQUIRED_MANIFEST);
    if manifest_path.is_file() {
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?;
        let value = serde_json::from_str::<serde_json::Value>(&raw)
            .with_context(|| format!("parse {}", manifest_path.display()))?;
        if let Some(note) = value.get("note").and_then(serde_json::Value::as_str)
            && let Some(marker) = defer_marker(note)
        {
            return Ok(Some(format!(
                "{REQUIRED_MANIFEST} note contains `{marker}`"
            )));
        }
    }

    let readme_path = solution_dir.join("README.md");
    if readme_path.is_file() {
        let raw = fs::read_to_string(&readme_path)
            .with_context(|| format!("read {}", readme_path.display()))?;
        if let Some(marker) = defer_marker(&raw) {
            return Ok(Some(format!("README.md contains `{marker}`")));
        }
    }

    Ok(None)
}

fn defer_marker(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    [
        "cheap public",
        "public-only",
        "public only",
        "tiny public",
        "not as an official",
        "not intended to be competitive",
        "not a competitive private",
        "meant for public validation",
        "public validation and dev seeding",
        "smoke",
    ]
    .into_iter()
    .find(|marker| lower.contains(marker))
}

fn load_state(path: &Path) -> Result<BTreeMap<BaselineKey, BaselineStateRecord>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read baseline state {}", path.display()))?;
    let mut state = BTreeMap::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str::<BaselineStateRecord>(line).with_context(|| {
            format!(
                "parse baseline state line {} in {}",
                index.saturating_add(1),
                path.display()
            )
        })?;
        state.insert(
            BaselineKey::new(record.challenge_name.clone(), record.target.clone()),
            record,
        );
    }
    Ok(state)
}

fn append_state(path: &Path, record: &BaselineStateRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create state directory {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open baseline state {}", path.display()))?;
    serde_json::to_writer(&mut file, record).context("write baseline state record")?;
    file.write_all(b"\n").context("finish baseline state line")
}

fn now_unix_secs() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs())
}

async fn list_challenges(client: &Client, base_url: &Url) -> Result<Vec<ChallengeName>> {
    let mut offset = 0i64;
    let limit = 100i64;
    let mut names = Vec::new();
    loop {
        let path = format!("/api/public/challenges?limit={limit}&offset={offset}");
        let response = get_json::<ChallengeListResponse>(client, base_url, &path, None).await?;
        names.extend(response.items.into_iter().map(|item| item.challenge_name));
        if !response.has_more {
            break;
        }
        offset = offset
            .checked_add(limit)
            .context("challenge list offset overflowed")?;
    }
    names.sort();
    Ok(names)
}

async fn get_challenge(
    client: &Client,
    base_url: &Url,
    challenge_name: &ChallengeName,
) -> Result<ChallengeDetailResponse> {
    let path = format!("/api/public/challenges/{challenge_name}");
    get_json(client, base_url, &path, None).await
}

fn select_targets(
    detail: &ChallengeDetailResponse,
    target_filter: &BTreeSet<TargetName>,
) -> Result<Vec<TargetName>> {
    let declared = detail
        .spec
        .targets
        .iter()
        .map(|target| target.name.clone())
        .collect::<BTreeSet<_>>();
    for target in target_filter {
        if !declared.contains(target) {
            bail!(
                "challenge {} does not declare requested target {target}",
                detail.challenge_name
            );
        }
    }
    let mut targets = detail
        .spec
        .targets
        .iter()
        .filter(|target| target_filter.is_empty() || target_filter.contains(&target.name))
        .map(|target| target.name.clone())
        .collect::<Vec<_>>();
    targets.sort();
    Ok(targets)
}

async fn create_submission(
    client: &Client,
    base_url: &Url,
    token: &SecretString,
    challenge_name: &ChallengeName,
    target: &TargetName,
    package: &SolutionPackage,
) -> Result<CreateSolutionSubmissionResponse> {
    let request = CreateSolutionSubmissionRequest {
        challenge_name: challenge_name.clone(),
        target: target.clone(),
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation: DEFAULT_EXPLANATION.to_string(),
        parent_solution_submission_id: None,
        credit_text: DEFAULT_CREDIT_TEXT.to_string(),
    };
    post_json(
        client,
        base_url,
        "/api/agent/solution-submissions",
        token,
        &request,
    )
    .await
}

async fn wait_submission(
    client: &Client,
    base_url: &Url,
    token: &SecretString,
    submission_id: &SolutionSubmissionId,
    timeout: Duration,
    poll: Duration,
) -> Result<SolutionSubmissionResponse> {
    let started = Instant::now();
    loop {
        let path = format!("/api/agent/solution-submissions/{submission_id}");
        let response =
            get_json::<SolutionSubmissionResponse>(client, base_url, &path, Some(token)).await?;
        if matches!(
            response.status,
            agentics_domain::models::evaluation::SolutionSubmissionStatus::Completed
                | agentics_domain::models::evaluation::SolutionSubmissionStatus::Failed
        ) {
            return Ok(response);
        }
        if started.elapsed() >= timeout {
            bail!("timed out waiting for submission {submission_id}");
        }
        sleep(poll).await;
    }
}

async fn get_json<T>(
    client: &Client,
    base_url: &Url,
    path: &str,
    token: Option<&SecretString>,
) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut request = client.get(join_url(base_url, path)?);
    if let Some(token) = token {
        request = request.bearer_auth(token.expose_secret());
    }
    let response = request.send().await.context("send GET request")?;
    response_to_json(response).await
}

async fn post_json<T, B>(
    client: &Client,
    base_url: &Url,
    path: &str,
    token: &SecretString,
    body: &B,
) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    B: Serialize + Sync,
{
    let response = client
        .post(join_url(base_url, path)?)
        .bearer_auth(token.expose_secret())
        .json(body)
        .send()
        .await
        .context("send POST request")?;
    response_to_json(response).await
}

async fn response_to_json<T>(response: reqwest::Response) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await.context("read HTTP response body")?;
    if !status.is_success() {
        bail!("Agentics API returned {status}: {body}");
    }
    serde_json::from_str(&body).with_context(|| format!("parse Agentics API response: {body}"))
}

fn join_url(base: &Url, path: &str) -> Result<Url> {
    let mut base = base.clone();
    let path = path.trim_start_matches('/');
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path().trim_end_matches('/')));
    }
    base.join(path)
        .with_context(|| format!("join API path {path}"))
}

fn package_solution_workspace(workspace_dir: &Path) -> Result<SolutionPackage> {
    package_solution_workspace_with_limits(workspace_dir, PackageLimits::DEFAULT)
}

fn package_solution_workspace_with_limits(
    workspace_dir: &Path,
    limits: PackageLimits,
) -> Result<SolutionPackage> {
    let workspace_dir = workspace_dir
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", workspace_dir.display()))?;
    if !workspace_dir.is_dir() {
        bail!("workspace is not a directory: {}", workspace_dir.display());
    }

    let manifest_path = workspace_dir.join(REQUIRED_MANIFEST);
    if !fs::exists(&manifest_path)
        .with_context(|| format!("inspect {}", manifest_path.display()))?
    {
        bail!("{REQUIRED_MANIFEST} must exist at the workspace root");
    }
    if !manifest_path.is_file() {
        bail!("{REQUIRED_MANIFEST} must be a file");
    }
    let manifest_raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest = ZipProjectManifest::parse_json(&manifest_raw)?;

    let mut required_scripts = Vec::new();
    if let Some(setup) = &manifest.commands.setup {
        required_scripts.push(setup);
    }
    if let Some(build) = &manifest.commands.build {
        required_scripts.push(build);
    }
    required_scripts.push(&manifest.commands.run);
    for script in &required_scripts {
        let script_path = workspace_dir.join(script.as_path());
        if !fs::exists(&script_path)
            .with_context(|| format!("inspect {}", script_path.display()))?
        {
            bail!("{script} must exist before packaging");
        }
        if !script_path.is_file() {
            bail!("{script} must be a file");
        }
    }

    let collected = collect_package_files(&workspace_dir, limits)?;
    let files = collected.files;
    if !files
        .iter()
        .any(|file| file.archive_name == REQUIRED_MANIFEST)
    {
        bail!("{REQUIRED_MANIFEST} is excluded by .gitignore or package filters");
    }
    for script in required_scripts {
        if !files
            .iter()
            .any(|file| file.archive_name == script.as_str())
        {
            bail!("{script} is excluded by .gitignore or package filters");
        }
    }
    if files.is_empty() {
        bail!("workspace contains no packageable files");
    }

    let bytes = write_zip_archive(&files, limits)?;
    let zip_bytes = u64::try_from(bytes.len()).context("zip archive length exceeds u64")?;
    if zip_bytes > limits.max_zip_bytes {
        bail!(
            "solution archive must be at most {} bytes after compression",
            limits.max_zip_bytes
        );
    }
    validate_zip_project_archive_envelope(&bytes)?;
    Ok(SolutionPackage {
        bytes,
        file_count: files.len(),
        uncompressed_bytes: collected.uncompressed_bytes,
    })
}

fn collect_package_files(
    workspace_dir: &Path,
    limits: PackageLimits,
) -> Result<CollectedPackageFiles> {
    let mut builder = WalkBuilder::new(workspace_dir);
    builder
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .hidden(true)
        .parents(true)
        .require_git(false)
        .filter_entry(should_descend);

    let mut files = Vec::new();
    let mut uncompressed_bytes = 0u64;
    for entry in builder.build() {
        let entry = entry.with_context(|| format!("walk workspace {}", workspace_dir.display()))?;
        if entry.path() == workspace_dir || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(workspace_dir)
            .with_context(|| format!("compute relative path for {}", entry.path().display()))?;
        let archive_name = archive_name(relative)?;
        let metadata = entry
            .metadata()
            .with_context(|| format!("stat {}", entry.path().display()))?;
        if files.len() >= limits.max_file_count {
            bail!(
                "solution workspace must contain at most {} packageable files",
                limits.max_file_count
            );
        }
        uncompressed_bytes = uncompressed_bytes
            .checked_add(metadata.len())
            .context("solution workspace is too large")?;
        if uncompressed_bytes > limits.max_uncompressed_bytes {
            bail!(
                "solution workspace must contain at most {} bytes before compression",
                limits.max_uncompressed_bytes
            );
        }
        files.push(PackageFile {
            path: entry.path().to_path_buf(),
            archive_name,
            unix_permissions: unix_permissions(&metadata),
        });
    }

    files.sort_by(|a, b| a.archive_name.cmp(&b.archive_name));
    Ok(CollectedPackageFiles {
        files,
        uncompressed_bytes,
    })
}

fn should_descend(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };
    !matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".mypy_cache"
            | ".venv"
            | "dist"
            | "build"
            | ".next"
    )
}

fn write_zip_archive(files: &[PackageFile], limits: PackageLimits) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    for file in files {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(file.unix_permissions);
        archive
            .start_file(&file.archive_name, options)
            .with_context(|| format!("add {} to zip", file.archive_name))?;
        copy_file_to_archive(file, &mut archive)?;
        if current_archive_len(&archive)? > limits.max_zip_bytes {
            bail!(
                "solution archive must be at most {} bytes after compression",
                limits.max_zip_bytes
            );
        }
    }
    Ok(archive.finish()?.into_inner())
}

fn current_archive_len(archive: &zip::ZipWriter<Cursor<Vec<u8>>>) -> Result<u64> {
    let cursor = archive
        .get_ref()
        .context("zip writer closed before package finalization")?;
    u64::try_from(cursor.get_ref().len()).context("zip archive length exceeds u64")
}

fn copy_file_to_archive<W>(file: &PackageFile, archive: &mut zip::ZipWriter<W>) -> Result<()>
where
    W: Write + Seek,
{
    let mut input =
        File::open(&file.path).with_context(|| format!("open {}", file.path.display()))?;
    std::io::copy(&mut input, archive)
        .with_context(|| format!("write {} to zip", file.archive_name))
        .map(|_| ())
}

fn archive_name(path: &Path) -> Result<String> {
    Ok(NormalizedArchivePath::from_relative_path(path, "solution package path")?.to_string())
}

#[cfg(unix)]
fn unix_permissions(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn unix_permissions(_metadata: &std::fs::Metadata) -> u32 {
    0o644
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{build_deferlist, name_set_from_args, solution_defer_marker};

    #[test]
    fn default_deferlist_is_disabled_when_requested() {
        let deferlist = build_deferlist(true, None).expect("deferlist");

        assert!(deferlist.is_empty());
    }

    #[test]
    fn default_deferlist_keeps_public_smoke_and_allows_upgraded_baselines() {
        let deferlist = build_deferlist(false, None).expect("deferlist");

        assert!(deferlist.contains(&"world-map-frontier-cs-algorithmic-6".parse().expect("name")));
        assert!(
            !deferlist.contains(
                &"functional-cycle-reach-frontier-cs-algorithmic-252"
                    .parse()
                    .expect("name")
            )
        );
    }

    #[test]
    fn name_set_accepts_explicit_names() {
        let name = "hello-world-rs".parse().expect("challenge name");
        let set = name_set_from_args(&[name], None).expect("name set");

        assert!(set.contains(&"hello-world-rs".parse().expect("challenge name")));
    }

    #[test]
    fn solution_defer_marker_detects_smoke_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("agentics.solution.json"),
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "Public smoke solution"
            })
            .to_string(),
        )
        .expect("manifest");

        let marker = solution_defer_marker(temp.path()).expect("marker");

        assert!(marker.is_some());
    }
}
