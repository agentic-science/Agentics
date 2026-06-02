use sqlx::{PgPool, Row};

use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::urls::MoltbookPostUrl;
use agentics_error::{Result, ServiceError};

use super::helpers::{
    challenge_status_from_row, localized_text_from_row, optional_moltbook_post_url_from_row,
};
use super::records::{AdminChallengeListItemRecord, ChallengeMoltbookDiscussionRecord};
use crate::db::ids::challenge_name_from_row;

/// List all challenge shells for admin review.
pub async fn list_admin_challenges(pool: &PgPool) -> Result<Vec<AdminChallengeListItemRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT challenge_name, title, summary, status, spec_json, moltbook_discussion_url, created_at, updated_at
        FROM challenges
        ORDER BY updated_at DESC, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let spec_json: Option<serde_json::Value> = r.try_get("spec_json")?;
            Ok(AdminChallengeListItemRecord {
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                title: r.try_get("title")?,
                summary: localized_text_from_row(&r, "summary")?,
                status: challenge_status_from_row(&r, "status")?,
                spec_json,
                moltbook_discussion_url: optional_moltbook_post_url_from_row(
                    &r,
                    "moltbook_discussion_url",
                )?,
                created_at: r.try_get("created_at")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Attach a Moltbook discussion post to an active or archived challenge.
pub async fn set_challenge_moltbook_discussion(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    discussion_url: &MoltbookPostUrl,
) -> Result<ChallengeMoltbookDiscussionRecord> {
    update_challenge_moltbook_discussion(pool, challenge_name, Some(discussion_url)).await
}

/// Clear a Moltbook discussion post from an active or archived challenge.
pub async fn clear_challenge_moltbook_discussion(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<ChallengeMoltbookDiscussionRecord> {
    update_challenge_moltbook_discussion(pool, challenge_name, None).await
}

/// Guarded Moltbook discussion update shared by set and clear paths.
async fn update_challenge_moltbook_discussion(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    discussion_url: Option<&MoltbookPostUrl>,
) -> Result<ChallengeMoltbookDiscussionRecord> {
    let row = sqlx::query(
        r#"
        UPDATE challenges
        SET moltbook_discussion_url = $2,
            updated_at = NOW()
        WHERE challenge_name = $1
          AND status IN ('active', 'archived')
          AND spec_json IS NOT NULL
        RETURNING challenge_name, moltbook_discussion_url
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(discussion_url.map(MoltbookPostUrl::as_str))
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(ServiceError::NotFound)?;
    Ok(ChallengeMoltbookDiscussionRecord {
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        discussion_url: optional_moltbook_post_url_from_row(&row, "moltbook_discussion_url")?,
    })
}
