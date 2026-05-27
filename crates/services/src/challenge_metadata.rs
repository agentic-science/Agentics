//! Published challenge metadata workflows.

use agentics_config::Config;
use agentics_domain::error::Result;
use agentics_domain::models::challenge::MoltbookCommunityDto;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::request::ChallengeMoltbookDiscussionResponse;
use agentics_domain::models::urls::MoltbookPostUrl;
use agentics_persistence::{ChallengeMoltbookDiscussionRecord, Repositories};

/// Attach a Moltbook discussion post to one published challenge.
pub async fn set_challenge_moltbook_discussion(
    pool: &sqlx::PgPool,
    config: &Config,
    challenge_name: &ChallengeName,
    discussion_url: &MoltbookPostUrl,
) -> Result<ChallengeMoltbookDiscussionResponse> {
    let record = Repositories::new(pool)
        .challenges()
        .set_moltbook_discussion(challenge_name, discussion_url)
        .await?;
    Ok(challenge_moltbook_discussion_response(config, record))
}

/// Clear a Moltbook discussion post from one published challenge.
pub async fn clear_challenge_moltbook_discussion(
    pool: &sqlx::PgPool,
    config: &Config,
    challenge_name: &ChallengeName,
) -> Result<ChallengeMoltbookDiscussionResponse> {
    let record = Repositories::new(pool)
        .challenges()
        .clear_moltbook_discussion(challenge_name)
        .await?;
    Ok(challenge_moltbook_discussion_response(config, record))
}

/// Build the admin response shape for Moltbook discussion updates.
fn challenge_moltbook_discussion_response(
    config: &Config,
    record: ChallengeMoltbookDiscussionRecord,
) -> ChallengeMoltbookDiscussionResponse {
    ChallengeMoltbookDiscussionResponse {
        challenge_name: record.challenge_name,
        moltbook: MoltbookCommunityDto {
            submolt_name: config.moltbook.submolt_name.clone(),
            submolt_url: config.moltbook.submolt_url.clone(),
            discussion_url: record.discussion_url,
        },
    }
}
