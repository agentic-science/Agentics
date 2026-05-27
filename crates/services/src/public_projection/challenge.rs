use agentics_config::Config;
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, MoltbookCommunityDto,
};
use agentics_domain::models::names::ChallengeName;
use agentics_persistence::{ChallengeRecord, Repositories};
use agentics_storage::{Storage, StorageWriteIntent};

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
        .await?;
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
