use std::path::Path;

use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, ReviewChallengeDraftRequest, ValidateChallengeDraftRequest,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::ChallengeDraftId;
use anyhow::{Context, Result, bail};
use secrecy::{ExposeSecret, SecretString};

use crate::api::ApiClient;
use crate::cli::{self, AdminAuthArgs, ChallengeDraftCommand, ChallengePrivateAssetKindArg};
use crate::config::ResolvedSettings;
use crate::output;

/// Handles challenge draft for this module.
pub(crate) async fn challenge_draft(
    command: ChallengeDraftCommand,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeDraftCommand::Create { .. } => {
            bail!(
                "creator draft creation requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::Status {
            draft_id: _draft_id,
        } => {
            bail!(
                "creator draft status requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::UploadPrivateAsset { .. } => {
            bail!(
                "creator private asset upload requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::Validate {
            draft_id,
            repository_path,
            admin,
        } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .validate_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest { repository_path },
                    &admin.admin_username,
                    &admin_password,
                )
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Approve {
            draft_id,
            expected_validation_bundle_sha256,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: Some(expected_validation_bundle_sha256),
                    action: DraftReviewAction::Approve,
                },
                settings,
            )
            .await
        }
        ChallengeDraftCommand::Reject {
            draft_id,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: DraftReviewAction::Reject,
                },
                settings,
            )
            .await
        }
        ChallengeDraftCommand::Publish {
            draft_id,
            repository_path,
            admin,
        } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .publish_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest { repository_path },
                    &admin.admin_username,
                    &admin_password,
                )
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Abandon {
            draft_id,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: DraftReviewAction::Abandon,
                },
                settings,
            )
            .await
        }
        ChallengeDraftCommand::Cleanup { admin } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let response = client
                .cleanup_challenge_drafts_admin(&admin.admin_username, &admin_password)
                .await?;
            output::render_challenge_draft_cleanup(&response, output_format)
        }
    }
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

/// Resolve the admin password from a non-argv source.
fn resolve_admin_password(
    admin: &AdminAuthArgs,
    settings: &ResolvedSettings,
) -> Result<SecretString> {
    let password = if admin.admin_password_stdin {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .context("failed to read admin password from stdin")?;
        SecretString::from(input.trim_end_matches(['\r', '\n']).to_string())
    } else {
        settings.admin_password.clone().unwrap_or_default()
    };
    if password.expose_secret().is_empty() {
        bail!("set AGENTICS_ADMIN_PASSWORD or pass --admin-password-stdin for admin commands");
    }
    Ok(password)
}

/// Enumerates draft review action variants supported by this module.
enum DraftReviewAction {
    Approve,
    Reject,
    Abandon,
}

/// Carries draft review inputs through the command handler.
struct DraftReviewRequest {
    admin: AdminAuthArgs,
    draft_id: ChallengeDraftId,
    message: String,
    expected_validation_bundle_sha256: Option<Sha256Digest>,
    action: DraftReviewAction,
}

/// Handles review draft for this module.
async fn review_draft(
    client: &ApiClient,
    output_format: cli::OutputFormat,
    review: DraftReviewRequest,
    settings: &ResolvedSettings,
) -> Result<String> {
    let request = ReviewChallengeDraftRequest {
        message: review.message,
        expected_validation_bundle_sha256: review.expected_validation_bundle_sha256,
    };
    let admin_password = resolve_admin_password(&review.admin, settings)?;
    let response = match review.action {
        DraftReviewAction::Approve => {
            client
                .approve_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
        DraftReviewAction::Reject => {
            client
                .reject_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
        DraftReviewAction::Abandon => {
            client
                .abandon_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
    };
    output::render_challenge_draft(&response, output_format)
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
