use std::path::{Path, PathBuf};

use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::challenge_creation::AGENTICS_CHALLENGE_MANIFEST_FILE;
use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, CreateChallengeReviewRecordRequest,
    UploadChallengePrivateAssetRequest,
};
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::paths::RepoRelativePath;
use agentics_domain::models::request::CreateChallengeShortlistRevisionRequest;
use agentics_domain::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

use crate::CommandInput;
use crate::api::ApiClient;
use crate::cli::{
    self, ChallengePrivateAssetKindArg, ChallengeReviewRecordCommand, ChallengeShortlistCommand,
    CreatorAuthArgs,
};
use crate::config::ResolvedSettings;
use crate::output;

#[derive(Debug, Serialize)]
struct ChallengeCheckReport {
    checked_count: usize,
    passed_count: usize,
    failed_count: usize,
    results: Vec<ChallengeCheckResult>,
}

#[derive(Debug, Serialize)]
struct ChallengeCheckResult {
    path: String,
    challenge_name: Option<ChallengeName>,
    status: ChallengeCheckStatus,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChallengeCheckStatus {
    Passed,
    Failed,
}

/// Checks local challenge proposal directories with the Rust contract validator.
pub(crate) async fn challenge_creator_check(
    path: PathBuf,
    output_format: cli::OutputFormat,
) -> Result<String> {
    let roots = discover_challenge_proposal_roots(&path)?;
    let mut results = Vec::with_capacity(roots.len());

    for root in roots {
        let path = root.display().to_string();
        let validation = validate_challenge_proposal_root(&root).await;
        match validation {
            Ok(challenge_name) => results.push(ChallengeCheckResult {
                path,
                challenge_name: Some(challenge_name),
                status: ChallengeCheckStatus::Passed,
                error: None,
            }),
            Err(error) => results.push(ChallengeCheckResult {
                path,
                challenge_name: None,
                status: ChallengeCheckStatus::Failed,
                error: Some(format!("{error:#}")),
            }),
        }
    }

    let passed_count = results
        .iter()
        .filter(|result| matches!(result.status, ChallengeCheckStatus::Passed))
        .count();
    let checked_count = results.len();
    let failed_count = checked_count.saturating_sub(passed_count);
    let report = ChallengeCheckReport {
        checked_count,
        passed_count,
        failed_count,
        results,
    };
    let rendered = render_challenge_check_report(&report, output_format)?;
    if report.failed_count > 0 {
        bail!("{rendered}");
    }
    Ok(rendered)
}

fn discover_challenge_proposal_roots(path: &Path) -> Result<Vec<PathBuf>> {
    if path.join(AGENTICS_CHALLENGE_MANIFEST_FILE).is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let challenges_dir = path.join("challenges");
    if challenges_dir.is_dir() {
        return proposal_children(&challenges_dir)
            .with_context(|| format!("failed to inspect {}", challenges_dir.display()));
    }

    let children =
        proposal_children(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    if !children.is_empty() {
        return Ok(children);
    }

    bail!(
        "no challenge proposals found under {}; accepted layouts are a proposal directory containing {}, a repository root containing challenges/*/{}, or a collection directory with direct child proposal directories",
        path.display(),
        AGENTICS_CHALLENGE_MANIFEST_FILE,
        AGENTICS_CHALLENGE_MANIFEST_FILE
    );
}

fn proposal_children(path: &Path) -> Result<Vec<PathBuf>> {
    let mut children = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() && child.join(AGENTICS_CHALLENGE_MANIFEST_FILE).is_file() {
            children.push(child);
        }
    }
    children.sort();
    Ok(children)
}

async fn validate_challenge_proposal_root(root: &Path) -> Result<ChallengeName> {
    let manifest =
        agentics_contracts::challenge_creation::validate_challenge_creation_repository(root)
            .await
            .with_context(|| format!("failed to validate challenge proposal {}", root.display()))?;
    let directory_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("challenge proposal path must end in a UTF-8 directory name"))?;
    if directory_name != manifest.challenge_name.as_str() {
        bail!(
            "challenge directory name `{directory_name}` must match challenge_name `{}`",
            manifest.challenge_name
        );
    }
    Ok(manifest.challenge_name)
}

fn render_challenge_check_report(
    report: &ChallengeCheckReport,
    output_format: cli::OutputFormat,
) -> Result<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "checked: {}  passed: {}  failed: {}",
        report.checked_count, report.passed_count, report.failed_count
    ));
    for result in &report.results {
        let status = match result.status {
            ChallengeCheckStatus::Passed => "ok",
            ChallengeCheckStatus::Failed => "failed",
        };
        let name = result
            .challenge_name
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!("{status}\t{name}\t{}", result.path));
        if let Some(error) = &result.error {
            lines.push(format!("  {error}"));
        }
    }
    output::render_json_or_text(report, lines.join("\n"), output_format)
}

/// Handles creator-owned challenge review record commands.
pub(crate) async fn challenge_review_record(
    command: ChallengeReviewRecordCommand,
    creator: &CreatorAuthArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeReviewRecordCommand::Create {
            repo_url,
            pr_number,
            pr_url,
            commit_sha,
            repo_dir,
            challenge_path,
            pr_author_github_user_id,
        } => {
            let creator_api_token = resolve_creator_api_token(creator, settings, input)?;
            let challenge_path =
                RepoRelativePath::try_new(challenge_path).context("invalid challenge_path")?;
            let challenge_root = repo_dir.join(challenge_path.as_str());
            let manifest =
                agentics_contracts::challenge_creation::read_challenge_creation_manifest(
                    &challenge_root,
                )
                .await
                .with_context(|| {
                    format!(
                        "failed to read agentics.challenge.json under {}",
                        challenge_root.display()
                    )
                })?;
            let response = client
                .create_challenge_review_record_creator(
                    &CreateChallengeReviewRecordRequest {
                        repo_url: GithubRepoRemote::try_new(repo_url)
                            .context("invalid repo_url")?,
                        pr_number,
                        pr_url: GithubPullRequestUrl::try_new(pr_url).context("invalid pr_url")?,
                        commit_sha,
                        challenge_path,
                        pr_author_github_user_id: GithubUserId::try_new(pr_author_github_user_id)
                            .context("invalid pr_author_github_user_id")?,
                        manifest,
                    },
                    &creator_api_token,
                )
                .await?;
            output::render_creator_challenge_review_record(&response, output_format)
        }
        ChallengeReviewRecordCommand::Status { review_record_id } => {
            let creator_api_token = resolve_creator_api_token(creator, settings, input)?;
            let response = client
                .get_challenge_review_record_creator(&review_record_id, &creator_api_token)
                .await?;
            output::render_creator_challenge_review_record(&response, output_format)
        }
        ChallengeReviewRecordCommand::UploadPrivateAsset {
            review_record_id,
            asset_name,
            kind,
            file,
            required,
        } => {
            let creator_api_token = resolve_creator_api_token(creator, settings, input)?;
            let bytes = std::fs::read(&file)
                .with_context(|| format!("failed to read private asset {}", file.display()))?;
            let response = client
                .upload_challenge_private_asset_creator(
                    &review_record_id,
                    &UploadChallengePrivateAssetRequest {
                        asset_name,
                        kind: kind.into(),
                        required,
                        asset_base64: STANDARD.encode(bytes),
                    },
                    &creator_api_token,
                )
                .await?;
            output::render_challenge_private_asset(&response, output_format)
        }
    }
}

/// Resolves the creator API token from a non-argv source.
pub(crate) fn resolve_creator_api_token(
    creator: &CreatorAuthArgs,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<SecretString> {
    let token = if creator.creator_token_stdin {
        SecretString::from(
            input
                .read_to_string("creator API token")?
                .trim_end_matches(['\r', '\n'])
                .to_string(),
        )
    } else {
        settings.creator_api_token.clone().unwrap_or_default()
    };
    if token.expose_secret().is_empty() {
        bail!(
            "set AGENTICS_CREATOR_API_TOKEN, persist creator-api-token with `agentics config set creator-api-token --stdin`, or pass --creator-token-stdin for creator commands"
        );
    }
    Ok(token)
}

/// Handles owner-visible challenge stats commands.
pub(crate) async fn creator_stats(
    challenge_name: agentics_domain::models::names::ChallengeName,
    target: Option<agentics_domain::models::names::TargetName>,
    creator: &CreatorAuthArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let token = resolve_creator_api_token(creator, settings, input)?;
    let response = client
        .get_creator_challenge_stats(&challenge_name, target.as_ref(), &token)
        .await?;
    output::render_creator_challenge_stats(&response, output_format)
}

/// Handles owner-visible challenge participant commands.
pub(crate) async fn creator_participants(
    challenge_name: agentics_domain::models::names::ChallengeName,
    target: Option<agentics_domain::models::names::TargetName>,
    creator: &CreatorAuthArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let token = resolve_creator_api_token(creator, settings, input)?;
    let response = client
        .list_creator_challenge_participants(&challenge_name, target.as_ref(), &token)
        .await?;
    output::render_creator_challenge_participants(&response, output_format)
}

/// Handles owner-managed challenge shortlist commands.
pub(crate) async fn challenge_shortlist(
    command: ChallengeShortlistCommand,
    creator: &CreatorAuthArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let token = resolve_creator_api_token(creator, settings, input)?;
    match command {
        ChallengeShortlistCommand::Show { challenge_name } => {
            let response = client
                .get_challenge_shortlist_creator(&challenge_name, &token)
                .await?;
            output::render_challenge_shortlist(&response, output_format)
        }
        ChallengeShortlistCommand::Upload {
            challenge_name,
            file,
        } => {
            let raw = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read shortlist JSON {}", file.display()))?;
            let request: CreateChallengeShortlistRevisionRequest =
                serde_json::from_str(&raw).context("shortlist file must be valid JSON")?;
            let response = client
                .create_challenge_shortlist_revision_creator(&challenge_name, &request, &token)
                .await?;
            output::render_challenge_shortlist_revision(&response, output_format)
        }
    }
}

impl From<ChallengePrivateAssetKindArg> for ChallengePrivateAssetKind {
    /// Converts CLI private asset kind into the API contract enum.
    fn from(value: ChallengePrivateAssetKindArg) -> Self {
        match value {
            ChallengePrivateAssetKindArg::BenchmarkData => Self::PrivateBenchmarkData,
            ChallengePrivateAssetKindArg::EvaluatorPackage => Self::PrivateEvaluatorPackage,
            ChallengePrivateAssetKindArg::Seeds => Self::PrivateSeeds,
            ChallengePrivateAssetKindArg::ReferenceOutputs => Self::PrivateReferenceOutputs,
        }
    }
}
