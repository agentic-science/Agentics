use agentics_config::Config;
use agentics_contracts::challenge_creation;
use agentics_contracts::validation::github::GithubPullRequestRef;
use agentics_domain::models::challenge_creation::CreatorChallengeReviewRecordResponse;
use agentics_domain::models::ids::{ChallengeReviewAuditEventId, ChallengeReviewRecordId};
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::paths::RepoRelativePath;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};

use super::presentation::review_record_response;
use super::types::CreateChallengeReviewRecordServiceRequest;

/// Create a challenge review record bound to a public GitHub PR and manifest.
pub async fn create_challenge_review_record(
    pool: &sqlx::PgPool,
    config: &Config,
    request: CreateChallengeReviewRecordServiceRequest,
) -> Result<CreatorChallengeReviewRecordResponse> {
    let CreateChallengeReviewRecordServiceRequest { creator, body } = request;
    challenge_creation::validate_challenge_creation_manifest(&body.manifest)?;
    validate_challenge_review_record_path(&body.challenge_path, &body.manifest.challenge_name)?;
    GithubPullRequestRef::try_new(
        body.repo_url.clone(),
        body.pr_url.clone(),
        body.pr_number.clone(),
    )?;

    if creator.github_user_id != body.pr_author_github_user_id {
        return Err(ServiceError::BadRequest(format!(
            "PR author GitHub user id {} does not match authenticated creator GitHub user id {}",
            body.pr_author_github_user_id, creator.github_user_id
        )));
    }
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&body.manifest)?;
    let review_record_id = ChallengeReviewRecordId::generate();
    let repo_url = body.repo_url.clone();
    let pr_number = body.pr_number.clone();
    let commit_sha = body.commit_sha;
    let review_record = Repositories::new(pool)
        .challenge_review_records()
        .create(
            &persistence::CreateChallengeReviewRecordInput {
                review_record_id: review_record_id.clone(),
                creator_agent_id: creator.agent_id.clone(),
                max_active_review_records: i64::from(
                    config.quotas.max_active_challenge_review_records_per_agent,
                ),
                creator_github_user_id: creator.github_user_id,
                creator_github_login: creator.github_login.clone(),
                repo_url: body.repo_url,
                pr_number: body.pr_number,
                pr_url: body.pr_url,
                commit_sha: body.commit_sha,
                challenge_path: body.challenge_path,
                manifest_sha256,
                manifest: body.manifest,
            },
            &persistence::CreateChallengeReviewRecordAuditEventInput {
                event_id: ChallengeReviewAuditEventId::generate(),
                review_record_id,
                actor_agent_id: Some(creator.agent_id.clone()),
                actor_admin_username: None,
                action: "draft_created".to_string(),
                message: "challenge review record created from GitHub PR".to_string(),
                metadata: serde_json::json!({
                    "repo_url": repo_url,
                    "pr_number": pr_number,
                    "commit_sha": commit_sha
                }),
            },
        )
        .await
        .map_err(ServiceError::unique_violation_as_conflict)?;

    Ok(review_record_response(review_record).into())
}

/// Ensures a review_record path follows the canonical `challenges/{challenge_name}` repository layout.
fn validate_challenge_review_record_path(
    path: &RepoRelativePath,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let expected = format!("challenges/{challenge_name}");
    if path.as_str() != expected {
        return Err(ServiceError::BadRequest(format!(
            "challenge_path must be `{expected}`"
        )));
    }
    Ok(())
}
