use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, CreateChallengeReviewRecordRequest,
    UploadChallengePrivateAssetRequest,
};
use agentics_domain::models::paths::RepoRelativePath;
use agentics_domain::models::request::CreateChallengeShortlistRevisionRequest;
use agentics_domain::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::{ExposeSecret, SecretString};

use crate::CommandInput;
use crate::api::ApiClient;
use crate::cli::{
    self, ChallengePrivateAssetKindArg, ChallengeReviewRecordCommand, ChallengeShortlistCommand,
    CreatorAuthArgs,
};
use crate::config::ResolvedSettings;
use crate::output;

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
