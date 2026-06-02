use std::path::Path;

use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, ChallengeReviewDecisionRequest, ValidateChallengeReviewRecordRequest,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::ChallengeReviewRecordId;
use anyhow::{Context, Result, bail};
use secrecy::{ExposeSecret, SecretString};

use crate::api::ApiClient;
use crate::cli::{self, AdminAuthArgs, ChallengePrivateAssetKindArg, ChallengeReviewRecordCommand};
use crate::config::ResolvedSettings;
use crate::output;

/// Handles challenge review record for this module.
pub(crate) async fn challenge_review_record(
    command: ChallengeReviewRecordCommand,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeReviewRecordCommand::Create { .. } => {
            bail!(
                "creator review record creation requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeReviewRecordCommand::Status {
            review_record_id: _review_record_id,
        } => {
            bail!(
                "creator review record status requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeReviewRecordCommand::UploadPrivateAsset { .. } => {
            bail!(
                "creator private asset upload requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeReviewRecordCommand::Validate {
            review_record_id,
            repository_path,
            admin,
        } => {
            let admin_service_token = resolve_admin_service_token(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .validate_challenge_review_record_admin(
                    &review_record_id,
                    &ValidateChallengeReviewRecordRequest { repository_path },
                    &admin_service_token,
                )
                .await?;
            output::render_challenge_review_record(&response, output_format)
        }
        ChallengeReviewRecordCommand::Approve {
            review_record_id,
            expected_validation_bundle_sha256,
            message,
            admin,
        } => {
            review_record_decision(
                &client,
                output_format,
                ReviewRecordDecisionRequest {
                    admin,
                    review_record_id,
                    message,
                    expected_validation_bundle_sha256: Some(expected_validation_bundle_sha256),
                    action: ReviewRecordDecisionAction::Approve,
                },
                settings,
            )
            .await
        }
        ChallengeReviewRecordCommand::Reject {
            review_record_id,
            message,
            admin,
        } => {
            review_record_decision(
                &client,
                output_format,
                ReviewRecordDecisionRequest {
                    admin,
                    review_record_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: ReviewRecordDecisionAction::Reject,
                },
                settings,
            )
            .await
        }
        ChallengeReviewRecordCommand::Publish {
            review_record_id,
            repository_path,
            admin,
        } => {
            let admin_service_token = resolve_admin_service_token(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .publish_challenge_review_record_admin(
                    &review_record_id,
                    &ValidateChallengeReviewRecordRequest { repository_path },
                    &admin_service_token,
                )
                .await?;
            output::render_challenge_review_record(&response, output_format)
        }
        ChallengeReviewRecordCommand::Abandon {
            review_record_id,
            message,
            admin,
        } => {
            review_record_decision(
                &client,
                output_format,
                ReviewRecordDecisionRequest {
                    admin,
                    review_record_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: ReviewRecordDecisionAction::Abandon,
                },
                settings,
            )
            .await
        }
        ChallengeReviewRecordCommand::Cleanup { admin } => {
            let admin_service_token = resolve_admin_service_token(&admin, settings)?;
            let response = client
                .cleanup_challenge_review_records_admin(&admin_service_token)
                .await?;
            output::render_challenge_review_record_cleanup(&response, output_format)
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

/// Resolve the admin service token from an explicit source.
fn resolve_admin_service_token(
    admin: &AdminAuthArgs,
    settings: &ResolvedSettings,
) -> Result<SecretString> {
    if let Some(token) = &admin.admin_service_token {
        let token = token.trim();
        if token.is_empty() {
            bail!("--admin-service-token must not be empty");
        }
        return Ok(SecretString::from(token.to_string()));
    }
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
            "set AGENTICS_ADMIN_SERVICE_TOKEN or pass --admin-service-token/--admin-service-token-stdin for admin commands"
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

/// Carries review record decision inputs through the command handler.
struct ReviewRecordDecisionRequest {
    admin: AdminAuthArgs,
    review_record_id: ChallengeReviewRecordId,
    message: String,
    expected_validation_bundle_sha256: Option<Sha256Digest>,
    action: ReviewRecordDecisionAction,
}

/// Handles a review record decision.
async fn review_record_decision(
    client: &ApiClient,
    output_format: cli::OutputFormat,
    review: ReviewRecordDecisionRequest,
    settings: &ResolvedSettings,
) -> Result<String> {
    let request = ChallengeReviewDecisionRequest {
        message: review.message,
        expected_validation_bundle_sha256: review.expected_validation_bundle_sha256,
    };
    let admin_service_token = resolve_admin_service_token(&review.admin, settings)?;
    let response = match review.action {
        ReviewRecordDecisionAction::Approve => {
            client
                .approve_challenge_review_record_admin(
                    &review.review_record_id,
                    &request,
                    &admin_service_token,
                )
                .await?
        }
        ReviewRecordDecisionAction::Reject => {
            client
                .reject_challenge_review_record_admin(
                    &review.review_record_id,
                    &request,
                    &admin_service_token,
                )
                .await?
        }
        ReviewRecordDecisionAction::Abandon => {
            client
                .abandon_challenge_review_record_admin(
                    &review.review_record_id,
                    &request,
                    &admin_service_token,
                )
                .await?
        }
    };
    output::render_challenge_review_record(&response, output_format)
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
