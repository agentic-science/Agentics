use agentics_config::Config;
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, ChallengeListItemDto, ChallengeListResponse,
    MoltbookCommunityDto,
};
use agentics_domain::models::names::ChallengeName;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    ChallengeCatalogFilters, ChallengeRecord, PublishedChallengeList,
    PublishedChallengeListItemRecord, Repositories,
};
use agentics_storage::{Storage, StorageWriteIntent};

use crate::storage_errors::storage_error_to_service_error;

/// Fetch public challenge details by challenge name.
pub async fn get_challenge_detail(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    challenge_name: &ChallengeName,
) -> Result<ChallengeDetailResponse> {
    let challenge = Repositories::new(pool)
        .challenges()
        .get_public(challenge_name)
        .await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    let statement_bytes = storage
        .get(
            &challenge.statement_key,
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await
        .map_err(storage_error_to_service_error)?;
    let statement = String::from_utf8(statement_bytes).map_err(|e| {
        ServiceError::Internal(format!("stored challenge statement is not UTF-8: {e}"))
    })?;
    let moltbook = MoltbookCommunityDto {
        submolt_name: config.moltbook.submolt_name.clone(),
        submolt_url: config.moltbook.submolt_url.clone(),
        discussion_url: challenge.moltbook_discussion_url.clone(),
    };
    present_challenge_detail(&challenge, &statement, moltbook)
}

/// Present public challenge details from a published challenge record and statement body.
pub fn present_challenge_detail(
    challenge: &ChallengeRecord,
    statement: &str,
    moltbook: MoltbookCommunityDto,
) -> Result<ChallengeDetailResponse> {
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    Ok(ChallengeDetailResponse {
        challenge_name: challenge.challenge_name.clone(),
        title: challenge.title.clone(),
        summary: challenge.summary.clone(),
        keywords: spec.keywords.clone(),
        spec: spec.into(),
        statement_markdown: statement.to_string(),
        moltbook,
    })
}

/// List published challenges through the service-owned public projection layer.
pub async fn list_challenges(
    pool: &sqlx::PgPool,
    limit: i64,
    offset: i64,
    filters: &ChallengeCatalogFilters,
) -> Result<ChallengeListResponse> {
    let records = Repositories::new(pool)
        .challenges()
        .list_published(limit, offset, filters)
        .await?;
    present_challenge_list(records)
}

/// Present public challenge catalog rows from persistence records.
fn present_challenge_list(records: PublishedChallengeList) -> Result<ChallengeListResponse> {
    let items = records
        .items
        .into_iter()
        .map(present_challenge_list_item)
        .collect::<Result<Vec<_>>>()?;
    Ok(ChallengeListResponse {
        items,
        total_count: records.total_count,
        limit: records.limit,
        offset: records.offset,
        has_more: records.has_more,
    })
}

/// Project one published challenge catalog record into its public DTO shape.
fn present_challenge_list_item(
    record: PublishedChallengeListItemRecord,
) -> Result<ChallengeListItemDto> {
    let spec: ChallengeBundleSpec = serde_json::from_value(record.spec_json)
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;
    Ok(ChallengeListItemDto {
        challenge_name: record.challenge_name,
        title: record.title,
        summary: record.summary,
        keywords: spec.keywords,
        starts_at: spec.starts_at,
        closes_at: spec.closes_at,
        eligibility: spec.eligibility,
        moltbook_discussion_url: record.moltbook_discussion_url,
    })
}
