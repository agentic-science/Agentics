use sqlx::{PgPool, Row};

use agentics_domain::models::names::ChallengeName;
use agentics_error::{Result, ServiceError};

use super::helpers::{
    localized_text_from_row, optional_moltbook_post_url_from_row, storage_key_from_row,
};
use super::records::{
    ChallengeCatalogFilters, ChallengeRecord, PublishedChallengeList,
    PublishedChallengeListItemRecord,
};
use crate::db::ids::challenge_name_from_row;

/// List active challenges with their published benchmark contract.
pub async fn list_published_challenges(
    pool: &PgPool,
    limit: i64,
    offset: i64,
    filters: &ChallengeCatalogFilters,
) -> Result<PublishedChallengeList> {
    let search = filters.search.as_deref();
    let keywords = filters
        .keywords
        .iter()
        .map(|keyword| keyword.as_str().to_string())
        .collect::<Vec<_>>();
    let total_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND (
            $1::text IS NULL
            OR POSITION(LOWER($1) IN LOWER(challenge_name)) > 0
            OR POSITION(LOWER($1) IN LOWER(title)) > 0
            OR POSITION(LOWER($1) IN LOWER(COALESCE(summary->>'en', ''))) > 0
            OR POSITION(LOWER($1) IN LOWER(COALESCE(summary->>'zh', ''))) > 0
            OR EXISTS (
              SELECT 1
              FROM jsonb_array_elements_text(COALESCE(spec_json->'keywords', '[]'::jsonb)) AS stored(keyword)
              WHERE POSITION(LOWER($1) IN LOWER(stored.keyword)) > 0
            )
          )
          AND (
            cardinality($2::text[]) = 0
            OR NOT EXISTS (
              SELECT 1
              FROM unnest($2::text[]) AS requested(keyword)
              WHERE NOT EXISTS (
                SELECT 1
                FROM jsonb_array_elements_text(COALESCE(spec_json->'keywords', '[]'::jsonb)) AS stored(keyword)
                WHERE LOWER(stored.keyword) = LOWER(requested.keyword)
              )
            )
          )
        "#,
    )
    .bind(search)
    .bind(&keywords)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT challenge_name, title, summary, spec_json, moltbook_discussion_url
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND (
            $1::text IS NULL
            OR POSITION(LOWER($1) IN LOWER(challenge_name)) > 0
            OR POSITION(LOWER($1) IN LOWER(title)) > 0
            OR POSITION(LOWER($1) IN LOWER(COALESCE(summary->>'en', ''))) > 0
            OR POSITION(LOWER($1) IN LOWER(COALESCE(summary->>'zh', ''))) > 0
            OR EXISTS (
              SELECT 1
              FROM jsonb_array_elements_text(COALESCE(spec_json->'keywords', '[]'::jsonb)) AS stored(keyword)
              WHERE POSITION(LOWER($1) IN LOWER(stored.keyword)) > 0
            )
          )
          AND (
            cardinality($2::text[]) = 0
            OR NOT EXISTS (
              SELECT 1
              FROM unnest($2::text[]) AS requested(keyword)
              WHERE NOT EXISTS (
                SELECT 1
                FROM jsonb_array_elements_text(COALESCE(spec_json->'keywords', '[]'::jsonb)) AS stored(keyword)
                WHERE LOWER(stored.keyword) = LOWER(requested.keyword)
              )
            )
          )
        ORDER BY created_at DESC, challenge_name ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(search)
    .bind(&keywords)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| {
            Ok(PublishedChallengeListItemRecord {
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                title: r.try_get("title")?,
                summary: localized_text_from_row(&r, "summary")?,
                spec_json: r.try_get("spec_json")?,
                moltbook_discussion_url: optional_moltbook_post_url_from_row(
                    &r,
                    "moltbook_discussion_url",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let returned_count = i64::try_from(items.len())
        .map_err(|_| ServiceError::Internal("challenge list length overflow".to_string()))?;
    let consumed = offset
        .checked_add(returned_count)
        .ok_or_else(|| ServiceError::Internal("challenge list offset overflow".to_string()))?;
    Ok(PublishedChallengeList {
        items,
        total_count,
        limit,
        offset,
        has_more: consumed < total_count,
    })
}

/// Fetch one active challenge by challenge name.
pub async fn get_published_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, moltbook_discussion_url
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND challenge_name = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Fetch one active challenge by unique challenge name.
pub async fn get_published_challenge_by_name(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, moltbook_discussion_url
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND challenge_name = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Fetch one public challenge detail by challenge name, including archived records
/// that are hidden from default browsing.
pub async fn get_public_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, moltbook_discussion_url
        FROM challenges
        WHERE status IN ('active', 'archived')
          AND spec_json IS NOT NULL
          AND challenge_name = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Converts a database row into the challenge record model.
fn row_to_challenge_record(r: sqlx::postgres::PgRow) -> Result<ChallengeRecord> {
    Ok(ChallengeRecord {
        challenge_name: challenge_name_from_row(&r, "challenge_name")?,
        title: r.try_get("title")?,
        summary: localized_text_from_row(&r, "summary")?,
        bundle_key: storage_key_from_row(&r, "bundle_key")?,
        public_bundle_key: storage_key_from_row(&r, "public_bundle_key")?,
        statement_key: storage_key_from_row(&r, "statement_key")?,
        spec_json: r.try_get("spec_json")?,
        moltbook_discussion_url: optional_moltbook_post_url_from_row(
            &r,
            "moltbook_discussion_url",
        )?,
    })
}
