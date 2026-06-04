use std::path::Path;

use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, ChallengeReviewDecisionRequest, CreateChallengeReviewRecordRequest,
    UploadChallengePrivateAssetRequest, ValidateChallengeReviewRecordRequest,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::ChallengeReviewRecordId;
use agentics_domain::models::paths::RepoRelativePath;
use agentics_domain::models::request::{
    CreatePioneerCodeRequest, SetChallengeMoltbookDiscussionRequest,
};
use agentics_domain::models::urls::{GithubPullRequestUrl, GithubRepoRemote, MoltbookPostUrl};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::{ExposeSecret, SecretString};

use crate::api::ApiClient;
use crate::cli::{
    self, AdminAgentsCommand, AdminArgs, AdminAuthArgs, AdminChallengesCommand, AdminCommand,
    AdminMoltbookCommand, AdminPioneerCodeCommand, AdminReviewRecordCommand,
    AdminServiceHeartbeatsCommand, AdminSubmissionsCommand, ChallengePrivateAssetKindArg,
    ChallengeReviewRecordCommand, CreatorAuthArgs,
};
use crate::config::ResolvedSettings;
use crate::output;

/// Handles challenge review record for this module.
pub(crate) async fn challenge_review_record(
    command: ChallengeReviewRecordCommand,
    creator: &CreatorAuthArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
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
            let creator_api_token = resolve_creator_api_token(creator, settings)?;
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
            let creator_api_token = resolve_creator_api_token(creator, settings)?;
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
            let creator_api_token = resolve_creator_api_token(creator, settings)?;
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

/// Resolve the creator API token from a non-argv source.
pub(crate) fn resolve_creator_api_token(
    creator: &CreatorAuthArgs,
    settings: &ResolvedSettings,
) -> Result<SecretString> {
    let token = if creator.creator_token_stdin {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .context("failed to read creator API token from stdin")?;
        SecretString::from(input.trim_end_matches(['\r', '\n']).to_string())
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

/// Converts an admin repository path into the UTF-8 API wire contract.
fn admin_repository_path_to_wire(path: &Path) -> Result<String> {
    path.to_str().map(ToOwned::to_owned).with_context(|| {
        format!(
            "admin repository path `{}` is not valid UTF-8; pass a UTF-8 path",
            path.display()
        )
    })
}

/// Resolve the admin service token from a non-argv source.
fn resolve_admin_service_token(
    admin: &AdminAuthArgs,
    settings: &ResolvedSettings,
) -> Result<SecretString> {
    let token = if admin.admin_service_token_stdin {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .context("failed to read admin service token from stdin")?;
        SecretString::from(input.trim_end_matches(['\r', '\n']).to_string())
    } else {
        settings.admin_service_token.clone().unwrap_or_default()
    };
    if token.expose_secret().is_empty() {
        bail!(
            "set AGENTICS_ADMIN_SERVICE_TOKEN or pass --admin-service-token-stdin for admin commands"
        );
    }
    Ok(token)
}

/// Enumerates review record decision variants supported by this module.
enum ReviewRecordDecisionAction {
    Approve,
    Reject,
    Abandon,
}

impl From<ChallengePrivateAssetKindArg> for ChallengePrivateAssetKind {
    /// Handles from for this module.
    fn from(value: ChallengePrivateAssetKindArg) -> Self {
        match value {
            ChallengePrivateAssetKindArg::BenchmarkData => Self::PrivateBenchmarkData,
            ChallengePrivateAssetKindArg::EvaluatorPackage => Self::PrivateEvaluatorPackage,
            ChallengePrivateAssetKindArg::Seeds => Self::PrivateSeeds,
            ChallengePrivateAssetKindArg::ReferenceOutputs => Self::PrivateReferenceOutputs,
        }
    }
}

/// Handles top-level admin commands authenticated by service token.
pub(crate) async fn admin_command(
    args: AdminArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let admin_service_token = resolve_admin_service_token(&args.admin, settings)?;
    match args.command {
        AdminCommand::PioneerCode { command } => {
            admin_pioneer_code(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::Challenges { command } => {
            admin_challenges(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::Moltbook { command } => {
            admin_moltbook(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::Submissions { command } => {
            admin_submissions(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::ReviewRecord { command } => {
            admin_review_record(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::ServiceHeartbeats { command } => {
            admin_service_heartbeats(command, &client, &admin_service_token, output_format).await
        }
        AdminCommand::Capacity => {
            let response = client.get_admin_capacity(&admin_service_token).await?;
            output::render_json_or_text(
                &response,
                format!(
                    "quota_window_seconds: {}\nactive_agents: {}\nactive_validation_jobs: {}\nactive_official_jobs: {}\nmax_active_agents: {}\nmax_active_official_jobs: {}",
                    response.quota_window_seconds,
                    response.usage.active_agents,
                    response.usage.active_validation_jobs,
                    response.usage.active_official_jobs,
                    response.quotas.max_active_agents,
                    response.quotas.max_active_official_jobs
                ),
                output_format,
            )
        }
        AdminCommand::Agents { command } => match command {
            AdminAgentsCommand::Disable { agent_id } => {
                let response = client
                    .disable_agent_admin(&agent_id, &admin_service_token)
                    .await?;
                output::render_json_or_text(
                    &response,
                    format!("agent: {}\nstatus: {}", response.id, response.status),
                    output_format,
                )
            }
        },
    }
}

async fn admin_pioneer_code(
    command: AdminPioneerCodeCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminPioneerCodeCommand::List => {
            let response = client.list_pioneer_codes_admin(admin_service_token).await?;
            output::render_json_or_text(
                &response,
                format!("pioneer_codes: {}", response.items.len()),
                output_format,
            )
        }
        AdminPioneerCodeCommand::Show { id } => {
            let response = client
                .get_pioneer_code_admin(&id, admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "pioneer_code: {}\nstatus: {}\nuses: {}/{}",
                    response.code.code_display,
                    response.code.status,
                    response.code.use_count,
                    response.code.max_uses
                ),
                output_format,
            )
        }
        AdminPioneerCodeCommand::Create {
            label,
            note,
            max_uses,
            expires_at,
        } => {
            let response = client
                .create_pioneer_code_admin(
                    &CreatePioneerCodeRequest {
                        label,
                        note: (!note.is_empty()).then_some(note),
                        max_uses,
                        expires_at,
                    },
                    admin_service_token,
                )
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "pioneer_code: {}\nid: {}\nstatus: {}",
                    response.code.code_display, response.code.id, response.code.status
                ),
                output_format,
            )
        }
        AdminPioneerCodeCommand::Revoke { id } => {
            let response = client
                .revoke_pioneer_code_admin(&id, admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "pioneer_code: {}\nstatus: {}\nrevoked_humans: {}\nrevoked_agents: {}",
                    response.id,
                    response.status,
                    response.revoked_human_count,
                    response.revoked_agent_count
                ),
                output_format,
            )
        }
    }
}

async fn admin_challenges(
    command: AdminChallengesCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminChallengesCommand::List => {
            let response = client.list_admin_challenges(admin_service_token).await?;
            output::render_json_or_text(
                &response,
                format!("challenges: {}", response.items.len()),
                output_format,
            )
        }
    }
}

async fn admin_moltbook(
    command: AdminMoltbookCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminMoltbookCommand::Set {
            challenge_name,
            discussion_url,
        } => {
            let response = client
                .set_challenge_moltbook_discussion_admin(
                    &challenge_name,
                    &SetChallengeMoltbookDiscussionRequest {
                        discussion_url: MoltbookPostUrl::try_new(discussion_url)
                            .context("invalid Moltbook discussion URL")?,
                    },
                    admin_service_token,
                )
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "challenge: {}\nmoltbook_discussion: {}",
                    response.challenge_name,
                    response
                        .moltbook
                        .discussion_url
                        .as_ref()
                        .map(|url| url.as_str())
                        .unwrap_or("none")
                ),
                output_format,
            )
        }
        AdminMoltbookCommand::Clear { challenge_name } => {
            let response = client
                .clear_challenge_moltbook_discussion_admin(&challenge_name, admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "challenge: {}\nmoltbook_discussion: none",
                    response.challenge_name
                ),
                output_format,
            )
        }
    }
}

async fn admin_submissions(
    command: AdminSubmissionsCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminSubmissionsCommand::List => {
            let response = client
                .list_admin_solution_submissions(admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!("solution_submissions: {}", response.items.len()),
                output_format,
            )
        }
        AdminSubmissionsCommand::Rejudge { submission_id } => {
            let response = client
                .rejudge_admin(&submission_id, admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "job: {}\nsubmission: {}\nstatus: {}",
                    response.job_id, response.solution_submission_id, response.status
                ),
                output_format,
            )
        }
        AdminSubmissionsCommand::OfficialRun { submission_id } => {
            let response = client
                .official_run_admin(&submission_id, admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!(
                    "job: {}\nsubmission: {}\nstatus: {}",
                    response.job_id, response.solution_submission_id, response.status
                ),
                output_format,
            )
        }
    }
}

async fn admin_review_record(
    command: AdminReviewRecordCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminReviewRecordCommand::List => {
            let response = client
                .list_challenge_review_records_admin(admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!("review_records: {}", response.items.len()),
                output_format,
            )
        }
        AdminReviewRecordCommand::PrivateAssets { review_record_id } => {
            let response = client
                .list_challenge_review_record_private_assets_admin(
                    &review_record_id,
                    admin_service_token,
                )
                .await?;
            output::render_json_or_text(
                &response,
                format!("private_assets: {}", response.items.len()),
                output_format,
            )
        }
        AdminReviewRecordCommand::Validate {
            review_record_id,
            repository_path,
        } => {
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .validate_challenge_review_record_admin(
                    &review_record_id,
                    &ValidateChallengeReviewRecordRequest { repository_path },
                    admin_service_token,
                )
                .await?;
            output::render_challenge_review_record(&response, output_format)
        }
        AdminReviewRecordCommand::Approve {
            review_record_id,
            expected_validation_bundle_sha256,
            message,
        } => {
            admin_review_record_decision(
                client,
                output_format,
                admin_service_token,
                review_record_id,
                message,
                Some(expected_validation_bundle_sha256),
                ReviewRecordDecisionAction::Approve,
            )
            .await
        }
        AdminReviewRecordCommand::Reject {
            review_record_id,
            message,
        } => {
            admin_review_record_decision(
                client,
                output_format,
                admin_service_token,
                review_record_id,
                message,
                None,
                ReviewRecordDecisionAction::Reject,
            )
            .await
        }
        AdminReviewRecordCommand::Abandon {
            review_record_id,
            message,
        } => {
            admin_review_record_decision(
                client,
                output_format,
                admin_service_token,
                review_record_id,
                message,
                None,
                ReviewRecordDecisionAction::Abandon,
            )
            .await
        }
        AdminReviewRecordCommand::Publish {
            review_record_id,
            repository_path,
        } => {
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .publish_challenge_review_record_admin(
                    &review_record_id,
                    &ValidateChallengeReviewRecordRequest { repository_path },
                    admin_service_token,
                )
                .await?;
            output::render_challenge_review_record(&response, output_format)
        }
        AdminReviewRecordCommand::Cleanup => {
            let response = client
                .cleanup_challenge_review_records_admin(admin_service_token)
                .await?;
            output::render_challenge_review_record_cleanup(&response, output_format)
        }
    }
}

async fn admin_review_record_decision(
    client: &ApiClient,
    output_format: cli::OutputFormat,
    admin_service_token: &SecretString,
    review_record_id: ChallengeReviewRecordId,
    message: String,
    expected_validation_bundle_sha256: Option<Sha256Digest>,
    action: ReviewRecordDecisionAction,
) -> Result<String> {
    let request = ChallengeReviewDecisionRequest {
        message,
        expected_validation_bundle_sha256,
    };
    let response = match action {
        ReviewRecordDecisionAction::Approve => {
            client
                .approve_challenge_review_record_admin(
                    &review_record_id,
                    &request,
                    admin_service_token,
                )
                .await?
        }
        ReviewRecordDecisionAction::Reject => {
            client
                .reject_challenge_review_record_admin(
                    &review_record_id,
                    &request,
                    admin_service_token,
                )
                .await?
        }
        ReviewRecordDecisionAction::Abandon => {
            client
                .abandon_challenge_review_record_admin(
                    &review_record_id,
                    &request,
                    admin_service_token,
                )
                .await?
        }
    };
    output::render_challenge_review_record(&response, output_format)
}

async fn admin_service_heartbeats(
    command: AdminServiceHeartbeatsCommand,
    client: &ApiClient,
    admin_service_token: &SecretString,
    output_format: cli::OutputFormat,
) -> Result<String> {
    match command {
        AdminServiceHeartbeatsCommand::List => {
            let response = client
                .list_admin_service_heartbeats(admin_service_token)
                .await?;
            output::render_json_or_text(
                &response,
                format!("service_heartbeats: {}", response.items.len()),
                output_format,
            )
        }
    }
}
