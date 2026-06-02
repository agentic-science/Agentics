use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::ids::AgentId;
use agentics_domain::models::names::ChallengeName;
use agentics_error::{Result, ServiceError};

use super::catalog::get_public_challenge;
use super::helpers::{sha256_digest_from_row, storage_key_from_row};
use super::records::{
    ChallengeShortlistRecord, ChallengeShortlistRevisionRecord, ChallengeShortlistedAgentRecord,
    CreateChallengeShortlistRevisionInput,
};
use crate::db::ids::{agent_id_from_row, challenge_name_from_row, human_id_from_row};

/// Return whether a challenge has any effective shortlisted agents.
pub async fn challenge_has_shortlist(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_shortlisted_agents
            WHERE challenge_name = $1
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Return whether an agent is in a challenge's effective shortlist.
pub async fn agent_is_shortlisted(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_shortlisted_agents
            WHERE challenge_name = $1 AND agent_id = $2::uuid
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(agent_id.as_str())
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Persist a shortlist delta and append any new agent ids to the effective shortlist.
pub async fn create_challenge_shortlist_revision(
    pool: &PgPool,
    input: &CreateChallengeShortlistRevisionInput,
) -> Result<ChallengeShortlistRevisionRecord> {
    let mut tx = pool.begin().await?;

    lock_challenge_shortlist(&mut tx, &input.challenge_name).await?;
    ensure_shortlist_agents_exist(&mut tx, &input.agent_ids_to_add).await?;

    sqlx::query(
        r#"
        INSERT INTO challenge_shortlist_revisions (
            id, challenge_name, uploader_human_id, storage_key, sha256, requested_count, added_count
        )
        VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, 0)
        "#,
    )
    .bind(input.revision_id.as_str())
    .bind(input.challenge_name.as_str())
    .bind(input.uploader_human_id.as_str())
    .bind(input.storage_key.as_str())
    .bind(input.sha256.to_string())
    .bind(input.requested_count)
    .execute(&mut *tx)
    .await?;

    let mut added_count = 0i64;
    for agent_id in &input.agent_ids_to_add {
        let result = sqlx::query(
            r#"
            INSERT INTO challenge_shortlisted_agents (
                challenge_name, agent_id, added_by_human_id, source_revision_id
            )
            VALUES ($1, $2::uuid, $3::uuid, $4::uuid)
            ON CONFLICT (challenge_name, agent_id) DO NOTHING
            "#,
        )
        .bind(input.challenge_name.as_str())
        .bind(agent_id.as_str())
        .bind(input.uploader_human_id.as_str())
        .bind(input.revision_id.as_str())
        .execute(&mut *tx)
        .await?;
        added_count = added_count
            .checked_add(i64::try_from(result.rows_affected()).map_err(|_| {
                ServiceError::Internal("shortlist added row count overflow".to_string())
            })?)
            .ok_or_else(|| ServiceError::Internal("shortlist added count overflow".to_string()))?;
    }

    let row = sqlx::query(
        r#"
        UPDATE challenge_shortlist_revisions
        SET added_count = $2
        WHERE id = $1::uuid
        RETURNING
            id,
            challenge_name,
            uploader_human_id,
            storage_key,
            sha256,
            requested_count,
            added_count,
            created_at
        "#,
    )
    .bind(input.revision_id.as_str())
    .bind(added_count)
    .fetch_one(&mut *tx)
    .await?;

    let response = row_to_shortlist_revision_record(row)?;
    tx.commit().await?;
    Ok(response)
}

/// Handles lock challenge shortlist for this module.
async fn lock_challenge_shortlist(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<()> {
    sqlx::query("SELECT challenge_name FROM challenges WHERE challenge_name = $1 FOR UPDATE")
        .bind(challenge_name.as_str())
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

/// Ensures shortlist agents exist before continuing.
async fn ensure_shortlist_agents_exist(
    tx: &mut Transaction<'_, Postgres>,
    agent_ids: &[AgentId],
) -> Result<()> {
    for agent_id in agent_ids {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1::uuid)",
        )
        .bind(agent_id.as_str())
        .fetch_one(&mut **tx)
        .await?;
        if !exists {
            return Err(ServiceError::BadRequest(format!(
                "shortlisted agent `{agent_id}` does not exist"
            )));
        }
    }
    Ok(())
}

/// List the effective challenge shortlist.
pub async fn list_challenge_shortlist(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<ChallengeShortlistRecord> {
    let challenge = get_public_challenge(pool, challenge_name)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let rows = sqlx::query(
        r#"
        SELECT s.agent_id::text AS agent_id, a.display_name AS agent_display_name, s.added_by_human_id::text AS added_by_human_id, s.created_at
        FROM challenge_shortlisted_agents s
        JOIN agents a ON a.id = s.agent_id
        WHERE s.challenge_name = $1
        ORDER BY s.created_at ASC, s.agent_id ASC
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            Ok(ChallengeShortlistedAgentRecord {
                agent_id: agent_id_from_row(&row, "agent_id")?,
                agent_display_name: row.try_get("agent_display_name")?,
                added_by_human_id: human_id_from_row(&row, "added_by_human_id")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ChallengeShortlistRecord {
        challenge_name: challenge.challenge_name,
        items,
    })
}

/// Converts a database row into the shortlist revision record.
fn row_to_shortlist_revision_record(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeShortlistRevisionRecord> {
    Ok(ChallengeShortlistRevisionRecord {
        id: crate::db::ids::challenge_shortlist_revision_id_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        uploader_human_id: human_id_from_row(&row, "uploader_human_id")?,
        requested_count: row.try_get("requested_count")?,
        added_count: row.try_get("added_count")?,
        sha256: sha256_digest_from_row(&row, "sha256")?,
        storage_key: storage_key_from_row(&row, "storage_key")?,
        created_at: row.try_get("created_at")?,
    })
}
