//! Creator-owned challenge statistics, participants, and shortlist workflows.

use tracing::warn;

use agentics_config::Config;
use agentics_contracts::challenge_creation;
use agentics_domain::models::auth::{
    CreateCreatorApiTokenRequest, CreatorApiTokenCreatedResponse, CreatorApiTokenDto,
    CreatorApiTokenListResponse, RevokeCreatorApiTokenResponse,
};
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::ids::{
    AgentId, ChallengeShortlistRevisionId, CreatorApiTokenId, HumanId,
};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse, ChallengeShortlistedAgentDto,
    CreateChallengeShortlistRevisionRequest, CreatorChallengeParticipantDto,
    CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse,
};
use agentics_domain::storage::StorageKey;
use agentics_error::{ErrorDetail, Result, ServiceError};
use agentics_persistence::{
    ChallengeShortlistRecord, ChallengeShortlistRevisionRecord,
    CreateChallengeShortlistRevisionInput, CreateCreatorApiTokenInput, CreatorApiTokenRecord,
    CreatorChallengeParticipantsRecord, CreatorChallengeStatsRecord, Repositories,
};
use agentics_storage::{Storage, StorageWriteIntent};

use crate::storage_errors::storage_error_to_service_error;

/// Create a creator API token for CLI-owned challenge workflows.
pub async fn create_creator_api_token(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    body: CreateCreatorApiTokenRequest,
) -> Result<CreatorApiTokenCreatedResponse> {
    let label = body.label.trim();
    if label.is_empty() {
        return Err(ServiceError::validation_failed(
            "creator API token request is invalid",
            [ErrorDetail {
                field: Some("label".to_string()),
                message: "must not be empty".to_string(),
            }],
        ));
    }
    let expires_at = parse_optional_future_rfc3339(body.expires_at.as_deref(), "expires_at")?;
    let token = crate::auth::create_creator_api_token();
    let record = Repositories::new(pool)
        .sessions()
        .create_creator_api_token(&CreateCreatorApiTokenInput {
            id: CreatorApiTokenId::generate(),
            token_hash: crate::auth::hash_opaque_token(&token),
            label: label.to_string(),
            created_by_human_id: human_id.clone(),
            expires_at,
        })
        .await?;

    Ok(CreatorApiTokenCreatedResponse {
        token,
        token_record: creator_api_token_from_record(record),
    })
}

/// List creator API tokens owned by one creator.
pub async fn list_creator_api_tokens(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
) -> Result<CreatorApiTokenListResponse> {
    let items = Repositories::new(pool)
        .sessions()
        .list_creator_api_tokens(human_id)
        .await?
        .into_iter()
        .map(creator_api_token_from_record)
        .collect();
    Ok(CreatorApiTokenListResponse { items })
}

/// Revoke one creator API token owned by the current creator.
pub async fn revoke_creator_api_token(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    id: &CreatorApiTokenId,
) -> Result<RevokeCreatorApiTokenResponse> {
    let token = Repositories::new(pool)
        .sessions()
        .revoke_creator_api_token(human_id, id)
        .await?;
    Ok(RevokeCreatorApiTokenResponse {
        token_record: creator_api_token_from_record(token),
    })
}

/// Fetch owner-visible aggregate challenge statistics for shortlist decisions.
pub async fn get_creator_challenge_stats(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    challenge_name: &ChallengeName,
    requested_target: Option<&str>,
) -> Result<CreatorChallengeStatsResponse> {
    let (challenge_name, target) =
        resolve_creator_challenge_scope(pool, human_id, challenge_name, requested_target).await?;
    Repositories::new(pool)
        .challenges()
        .creator_stats(&challenge_name, target.as_ref())
        .await
        .map(creator_challenge_stats_from_record)
}

/// Fetch owner-visible participant rows for shortlist decisions.
pub async fn list_creator_challenge_participants(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    challenge_name: &ChallengeName,
    requested_target: Option<&str>,
) -> Result<CreatorChallengeParticipantsResponse> {
    let (challenge_name, target) =
        resolve_creator_challenge_scope(pool, human_id, challenge_name, requested_target).await?;
    Repositories::new(pool)
        .challenges()
        .creator_participants(&challenge_name, target.as_ref())
        .await
        .map(creator_challenge_participants_from_record)
}

/// Append a delta-only owner-managed shortlist revision.
pub async fn create_challenge_shortlist_revision(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    human_id: &HumanId,
    challenge_name: &ChallengeName,
    body: CreateChallengeShortlistRevisionRequest,
) -> Result<ChallengeShortlistRevisionResponse> {
    let (challenge_name, _) =
        resolve_creator_challenge_scope(pool, human_id, challenge_name, None).await?;
    let requested_count = i64::try_from(body.agent_ids_to_add.len())
        .map_err(|_| ServiceError::BadRequest("shortlist payload is too large".to_string()))?;
    let raw_json = serde_json::to_vec(&body)
        .map_err(|e| ServiceError::Internal(format!("failed to encode shortlist revision: {e}")))?;
    let agent_ids_to_add = normalize_shortlist_agent_ids(&body.agent_ids_to_add)?;

    let revision_id = ChallengeShortlistRevisionId::generate();
    let sha256 = challenge_creation::sha256_digest(&raw_json);
    let storage_key = StorageKey::try_new(format!(
        "challenge-shortlists/{challenge_name}/{revision_id}.json"
    ))?;
    let stored_key = storage
        .put(
            &storage_key,
            &raw_json,
            StorageWriteIntent::new(
                "challenge shortlist JSON",
                config.storage.max_json_artifact_bytes,
            ),
        )
        .await
        .map_err(storage_error_to_service_error)?;

    let response = Repositories::new(pool)
        .challenges()
        .create_shortlist_revision(&CreateChallengeShortlistRevisionInput {
            revision_id,
            challenge_name,
            uploader_human_id: human_id.clone(),
            storage_key: stored_key.clone(),
            sha256,
            requested_count,
            agent_ids_to_add,
        })
        .await;

    match response {
        Ok(response) => Ok(shortlist_revision_from_record(response)),
        Err(error) => {
            cleanup_storage_key(storage, &stored_key).await;
            Err(error)
        }
    }
}

/// Fetch the effective owner-managed shortlist union.
pub async fn get_challenge_shortlist(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    challenge_name: &ChallengeName,
) -> Result<ChallengeShortlistResponse> {
    let (challenge_name, _) =
        resolve_creator_challenge_scope(pool, human_id, challenge_name, None).await?;
    Repositories::new(pool)
        .challenges()
        .list_shortlist(&challenge_name)
        .await
        .map(challenge_shortlist_from_record)
}

fn creator_challenge_stats_from_record(
    record: CreatorChallengeStatsRecord,
) -> CreatorChallengeStatsResponse {
    CreatorChallengeStatsResponse {
        challenge_name: record.challenge_name,
        target: record.target,
        agent_count: record.agent_count,
        solution_submission_count: record.solution_submission_count,
        completed_solution_submission_count: record.completed_solution_submission_count,
        failed_solution_submission_count: record.failed_solution_submission_count,
        queued_or_running_solution_submission_count: record
            .queued_or_running_solution_submission_count,
        visible_solution_submission_count: record.visible_solution_submission_count,
        validation_run_count: record.validation_run_count,
        official_run_count: record.official_run_count,
        latest_solution_submission_at: record
            .latest_solution_submission_at
            .map(|value| value.to_rfc3339()),
        latest_completed_evaluation_at: record
            .latest_completed_evaluation_at
            .map(|value| value.to_rfc3339()),
        primary_metric_name: record.primary_metric_name,
        primary_metric_min: record.primary_metric_min,
        primary_metric_max: record.primary_metric_max,
        primary_metric_mean: record.primary_metric_mean,
    }
}

fn creator_api_token_from_record(record: CreatorApiTokenRecord) -> CreatorApiTokenDto {
    CreatorApiTokenDto {
        id: record.id,
        label: record.label,
        status: record.status,
        created_by_human_id: record.created_by_human_id,
        created_at: record.created_at.to_rfc3339(),
        last_used_at: record.last_used_at.map(|value| value.to_rfc3339()),
        expires_at: record.expires_at.map(|value| value.to_rfc3339()),
        revoked_at: record.revoked_at.map(|value| value.to_rfc3339()),
    }
}

fn creator_challenge_participants_from_record(
    record: CreatorChallengeParticipantsRecord,
) -> CreatorChallengeParticipantsResponse {
    CreatorChallengeParticipantsResponse {
        challenge_name: record.challenge_name,
        target: record.target,
        items: record
            .items
            .into_iter()
            .map(|item| CreatorChallengeParticipantDto {
                agent_id: item.agent_id,
                agent_display_name: item.agent_display_name,
                solution_submission_count: item.solution_submission_count,
                best_solution_submission_id: item.best_solution_submission_id,
                best_primary_metric: item.best_primary_metric,
                latest_status: item.latest_status,
                latest_solution_submission_at: item
                    .latest_solution_submission_at
                    .map(|value| value.to_rfc3339()),
            })
            .collect(),
    }
}

fn shortlist_revision_from_record(
    record: ChallengeShortlistRevisionRecord,
) -> ChallengeShortlistRevisionResponse {
    ChallengeShortlistRevisionResponse {
        id: record.id,
        challenge_name: record.challenge_name,
        uploader_human_id: record.uploader_human_id,
        requested_count: record.requested_count,
        added_count: record.added_count,
        sha256: record.sha256,
        storage_key: record.storage_key,
        created_at: record.created_at.to_rfc3339(),
    }
}

fn challenge_shortlist_from_record(record: ChallengeShortlistRecord) -> ChallengeShortlistResponse {
    ChallengeShortlistResponse {
        challenge_name: record.challenge_name,
        items: record
            .items
            .into_iter()
            .map(|item| ChallengeShortlistedAgentDto {
                agent_id: item.agent_id,
                agent_display_name: item.agent_display_name,
                added_by_human_id: item.added_by_human_id,
                created_at: item.created_at.to_rfc3339(),
            })
            .collect(),
    }
}

/// Resolve and authorize a creator-owned challenge plus optional target filter.
async fn resolve_creator_challenge_scope(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    challenge_name: &ChallengeName,
    requested_target: Option<&str>,
) -> Result<(ChallengeName, Option<TargetName>)> {
    let repos = Repositories::new(pool);
    let challenge = repos.challenges().get_published(challenge_name).await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    if !repos
        .challenges()
        .human_owns(&challenge.challenge_name, human_id)
        .await?
    {
        return Err(ServiceError::Forbidden(
            "human is not an owner of this challenge".to_string(),
        ));
    }

    let target = resolve_target_from_spec(&challenge.spec_json, requested_target)?;
    Ok((challenge.challenge_name, target))
}

/// Validate an optional target query against a published challenge spec.
fn resolve_target_from_spec(
    spec_json: &serde_json::Value,
    requested_target: Option<&str>,
) -> Result<Option<TargetName>> {
    let Some(target) = requested_target else {
        return Ok(None);
    };

    let spec: ChallengeBundleSpec = serde_json::from_value(spec_json.clone())
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    let target = target
        .parse::<TargetName>()
        .map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    if spec.target(&target).is_some() {
        return Ok(Some(target));
    }
    Err(ServiceError::BadRequest(format!(
        "challenge does not support target `{target}`"
    )))
}

/// Deduplicate a shortlist delta and reject empty uploads before persistence.
fn normalize_shortlist_agent_ids(agent_ids: &[AgentId]) -> Result<Vec<AgentId>> {
    let mut unique = std::collections::BTreeSet::new();
    for agent_id in agent_ids {
        unique.insert(agent_id.clone());
    }
    if unique.is_empty() {
        return Err(ServiceError::BadRequest(
            "agent_ids_to_add must contain at least one agent id".to_string(),
        ));
    }
    Ok(unique.into_iter().collect())
}

/// Removes a staged shortlist object after shortlist persistence fails.
async fn cleanup_storage_key(storage: &dyn Storage, storage_key: &StorageKey) {
    if let Err(error) = storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up staged shortlist storage object"
        );
    }
}

fn parse_optional_future_rfc3339(
    raw: Option<&str>,
    field: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|error| validation_error(field, format!("must be RFC3339: {error}")))?;
    if parsed <= chrono::Utc::now() {
        return Err(validation_error(field, "must be in the future"));
    }
    Ok(Some(parsed))
}

fn validation_error(field: &str, message: impl Into<String>) -> ServiceError {
    ServiceError::validation_failed(
        "creator API token request is invalid",
        [ErrorDetail {
            field: Some(field.to_string()),
            message: message.into(),
        }],
    )
}
