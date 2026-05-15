//! Challenge shell and published challenge queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use super::ids::{challenge_id_from_row, optional_solution_submission_id_from_row};
use crate::error::{AppError, Result};
use crate::models::challenge::{
    AdminChallengeListItemDto, ChallengeBundleSpec, ChallengeListItemDto, PublishChallengeResponse,
};
use crate::models::ids::{ChallengeId, TargetName};
use crate::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse, ChallengeShortlistedAgentDto,
    CreatorChallengeParticipantDto, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse,
};

/// Published challenge joined with challenge metadata.
#[derive(Debug, Clone)]
pub struct ChallengeRecord {
    pub challenge_id: ChallengeId,
    pub title: String,
    pub summary: String,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec_json: Value,
}

/// Create or update an unpublished challenge shell.
pub async fn create_or_update_challenge(
    pool: &PgPool,
    id: &ChallengeId,
    title: &str,
    summary: &str,
) -> Result<crate::models::challenge::ChallengeAdminResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO challenges (id, title, summary, status)
        VALUES ($1, $2, $3, 'draft')
        ON CONFLICT (id) DO UPDATE
        SET title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING id, title, summary, status, created_at, updated_at
        "#,
    )
    .bind(id.as_str())
    .bind(title)
    .bind(summary)
    .fetch_one(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => AppError::Conflict,
        error => AppError::Database(error),
    })?;

    Ok(crate::models::challenge::ChallengeAdminResponse {
        id: challenge_id_from_row(&row, "id")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        status: row.try_get("status")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// List all challenge shells for admin review.
pub async fn list_admin_challenges(pool: &PgPool) -> Result<Vec<AdminChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT id, title, summary, status, spec_json, created_at, updated_at
        FROM challenges
        ORDER BY updated_at DESC, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let spec_json: Option<Value> = r.try_get("spec_json")?;
            let spec = spec_json
                .map(serde_json::from_value::<ChallengeBundleSpec>)
                .transpose()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(AdminChallengeListItemDto {
                id: challenge_id_from_row(&r, "id")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                status: r.try_get("status")?,
                targets: spec.as_ref().map(|spec| spec.targets.clone()),
                starts_at: spec.as_ref().and_then(|spec| spec.starts_at.clone()),
                closes_at: spec.as_ref().and_then(|spec| spec.closes_at.clone()),
                eligibility: spec.as_ref().map(|spec| spec.eligibility.clone()),
                visibility: spec.as_ref().map(|spec| spec.visibility.clone()),
                solution_publication: spec.as_ref().map(|spec| spec.solution_publication),
                private_benchmark_enabled: spec
                    .as_ref()
                    .map(|spec| spec.datasets.private_benchmark_enabled),
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Publish a validated bundle as the benchmark contract for a challenge id.
pub async fn publish_challenge(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    bundle_path: &str,
    statement_path: &str,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<PublishChallengeResponse> {
    let mut tx = pool.begin().await?;
    let response = publish_challenge_tx(
        &mut tx,
        challenge_id,
        bundle_path,
        statement_path,
        spec,
        title,
        summary,
    )
    .await?;
    tx.commit().await?;
    Ok(response)
}

pub async fn publish_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
    bundle_path: &str,
    statement_path: &str,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<PublishChallengeResponse> {
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenges (
            id, title, summary, bundle_path, statement_path, spec_json,
            starts_at, closes_at, eligibility_policy_json, validation_submission_limit,
            official_submission_limit, leaderboard_visibility, score_distribution_visibility,
            result_detail_visibility, solution_publication_policy, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'active')
        ON CONFLICT (id) DO UPDATE
        SET title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            bundle_path = EXCLUDED.bundle_path,
            statement_path = EXCLUDED.statement_path,
            spec_json = EXCLUDED.spec_json,
            starts_at = EXCLUDED.starts_at,
            closes_at = EXCLUDED.closes_at,
            eligibility_policy_json = EXCLUDED.eligibility_policy_json,
            validation_submission_limit = EXCLUDED.validation_submission_limit,
            official_submission_limit = EXCLUDED.official_submission_limit,
            leaderboard_visibility = EXCLUDED.leaderboard_visibility,
            score_distribution_visibility = EXCLUDED.score_distribution_visibility,
            result_detail_visibility = EXCLUDED.result_detail_visibility,
            solution_publication_policy = EXCLUDED.solution_publication_policy,
            status = 'active',
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING id AS challenge_id, title, bundle_path, statement_path
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(title)
    .bind(summary)
    .bind(bundle_path)
    .bind(statement_path)
    .bind(&spec_json)
    .bind(parse_optional_time(spec.starts_at.as_deref())?)
    .bind(parse_optional_time(spec.closes_at.as_deref())?)
    .bind(serde_json::to_value(&spec.eligibility).map_err(|e| AppError::Internal(e.to_string()))?)
    .bind(spec.validation_submission_limit)
    .bind(spec.official_submission_limit)
    .bind(to_json_string(spec.visibility.leaderboard)?)
    .bind(to_json_string(spec.visibility.score_distribution)?)
    .bind(to_json_string(spec.visibility.result_detail)?)
    .bind(to_json_string(spec.solution_publication)?)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => AppError::Conflict,
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => AppError::Conflict,
        error => AppError::Database(error),
    })?;

    Ok(PublishChallengeResponse {
        challenge_id: challenge_id_from_row(&row, "challenge_id")?,
        title: row.try_get("title")?,
        bundle_path: row.try_get("bundle_path")?,
        statement_path: row.try_get("statement_path")?,
    })
}

fn parse_optional_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("invalid challenge timestamp: {e}")))
        })
        .transpose()
}

fn to_json_string<T: serde::Serialize>(value: T) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|e| AppError::Internal(e.to_string()))?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Internal("challenge enum did not serialize to string".to_string()))
}

/// Archive a challenge shell while preserving private assets and historical submissions.
pub async fn archive_challenge(pool: &PgPool, challenge_id: &ChallengeId) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(challenge_id.as_str())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Grant challenge-owner permissions to an agent.
pub async fn add_challenge_owner(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    agent_id: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    add_challenge_owner_tx(&mut tx, challenge_id, agent_id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn add_challenge_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
    agent_id: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO challenge_owners (challenge_id, agent_id)
        VALUES ($1, $2)
        ON CONFLICT (challenge_id, agent_id) DO NOTHING
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(agent_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Check whether an agent is an owner of a challenge.
pub async fn agent_owns_challenge(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    agent_id: &str,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
            WHERE challenge_id = $1 AND agent_id = $2
        )
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(agent_id)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Return whether a challenge has any effective shortlisted agents.
pub async fn challenge_has_shortlist(pool: &PgPool, challenge_id: &ChallengeId) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_shortlisted_agents
            WHERE challenge_id = $1
        )
        "#,
    )
    .bind(challenge_id.as_str())
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Return whether an agent is in a challenge's effective shortlist.
pub async fn agent_is_shortlisted(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    agent_id: &str,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_shortlisted_agents
            WHERE challenge_id = $1 AND agent_id = $2
        )
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(agent_id)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Input for one shortlist delta revision.
#[derive(Debug, Clone)]
pub struct CreateChallengeShortlistRevisionInput {
    pub revision_id: String,
    pub challenge_id: ChallengeId,
    pub uploader_agent_id: String,
    pub storage_uri: String,
    pub sha256: String,
    pub requested_count: i64,
    pub agent_ids_to_add: Vec<String>,
}

/// Persist a shortlist delta and append any new agent ids to the effective shortlist.
pub async fn create_challenge_shortlist_revision(
    pool: &PgPool,
    input: &CreateChallengeShortlistRevisionInput,
) -> Result<ChallengeShortlistRevisionResponse> {
    let mut tx = pool.begin().await?;

    lock_challenge_shortlist(&mut tx, &input.challenge_id).await?;
    ensure_shortlist_agents_exist(&mut tx, &input.agent_ids_to_add).await?;

    sqlx::query(
        r#"
        INSERT INTO challenge_shortlist_revisions (
            id, challenge_id, uploader_agent_id, storage_uri, sha256, requested_count, added_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, 0)
        "#,
    )
    .bind(&input.revision_id)
    .bind(input.challenge_id.as_str())
    .bind(&input.uploader_agent_id)
    .bind(&input.storage_uri)
    .bind(&input.sha256)
    .bind(input.requested_count)
    .execute(&mut *tx)
    .await?;

    let mut added_count = 0i64;
    for agent_id in &input.agent_ids_to_add {
        let result = sqlx::query(
            r#"
            INSERT INTO challenge_shortlisted_agents (
                challenge_id, agent_id, added_by_agent_id, source_revision_id
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (challenge_id, agent_id) DO NOTHING
            "#,
        )
        .bind(input.challenge_id.as_str())
        .bind(agent_id)
        .bind(&input.uploader_agent_id)
        .bind(&input.revision_id)
        .execute(&mut *tx)
        .await?;
        added_count = added_count
            .checked_add(i64::try_from(result.rows_affected()).map_err(|_| {
                AppError::Internal("shortlist added row count overflow".to_string())
            })?)
            .ok_or_else(|| AppError::Internal("shortlist added count overflow".to_string()))?;
    }

    let row = sqlx::query(
        r#"
        UPDATE challenge_shortlist_revisions
        SET added_count = $2
        WHERE id = $1
        RETURNING id, challenge_id, uploader_agent_id, storage_uri, sha256, requested_count, added_count, created_at
        "#,
    )
    .bind(&input.revision_id)
    .bind(added_count)
    .fetch_one(&mut *tx)
    .await?;

    let response = row_to_shortlist_revision_response(row)?;
    tx.commit().await?;
    Ok(response)
}

async fn lock_challenge_shortlist(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
) -> Result<()> {
    sqlx::query("SELECT id FROM challenges WHERE id = $1 FOR UPDATE")
        .bind(challenge_id.as_str())
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

async fn ensure_shortlist_agents_exist(
    tx: &mut Transaction<'_, Postgres>,
    agent_ids: &[String],
) -> Result<()> {
    for agent_id in agent_ids {
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1)")
                .bind(agent_id)
                .fetch_one(&mut **tx)
                .await?;
        if !exists {
            return Err(AppError::BadRequest(format!(
                "shortlisted agent `{agent_id}` does not exist"
            )));
        }
    }
    Ok(())
}

/// List the effective challenge shortlist.
pub async fn list_challenge_shortlist(
    pool: &PgPool,
    challenge_id: &ChallengeId,
) -> Result<ChallengeShortlistResponse> {
    let rows = sqlx::query(
        r#"
        SELECT s.agent_id, a.name AS agent_name, s.added_by_agent_id, s.created_at
        FROM challenge_shortlisted_agents s
        JOIN agents a ON a.id = s.agent_id
        WHERE s.challenge_id = $1
        ORDER BY s.created_at ASC, s.agent_id ASC
        "#,
    )
    .bind(challenge_id.as_str())
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            Ok(ChallengeShortlistedAgentDto {
                agent_id: row.try_get("agent_id")?,
                agent_name: row.try_get("agent_name")?,
                added_by_agent_id: row.try_get("added_by_agent_id")?,
                created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ChallengeShortlistResponse {
        challenge_id: challenge_id.clone(),
        items,
    })
}

/// Challenge-owner statistics for one challenge and optional target.
pub async fn get_creator_challenge_stats(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: Option<&TargetName>,
) -> Result<CreatorChallengeStatsResponse> {
    let target_raw = target.map(TargetName::as_str);
    let row = sqlx::query(
        r#"
        WITH filtered_submissions AS (
            SELECT id, agent_id, status, visible_after_eval, created_at
            FROM solution_submissions
            WHERE challenge_id = $1
              AND ($2::TEXT IS NULL OR target = $2)
        ),
        submission_counts AS (
            SELECT
                COUNT(DISTINCT agent_id)::BIGINT AS agent_count,
                COUNT(*)::BIGINT AS solution_submission_count,
                COUNT(*) FILTER (WHERE status = 'completed')::BIGINT AS completed_solution_submission_count,
                COUNT(*) FILTER (WHERE status = 'failed')::BIGINT AS failed_solution_submission_count,
                COUNT(*) FILTER (WHERE status IN ('pending', 'queued', 'running'))::BIGINT AS queued_or_running_solution_submission_count,
                COUNT(*) FILTER (WHERE visible_after_eval)::BIGINT AS visible_solution_submission_count,
                MAX(created_at) AS latest_solution_submission_at
            FROM filtered_submissions
        ),
        job_counts AS (
            SELECT
                COUNT(*) FILTER (WHERE j.eval_type = 'validation')::BIGINT AS validation_run_count,
                COUNT(*) FILTER (WHERE j.eval_type = 'official')::BIGINT AS official_run_count
            FROM evaluation_jobs j
            JOIN filtered_submissions s ON s.id = j.solution_submission_id
        ),
        latest_completed_evaluation AS (
            SELECT MAX(e.finished_at) AS latest_completed_evaluation_at
            FROM evaluations e
            JOIN filtered_submissions s ON s.id = e.solution_submission_id
            WHERE e.status = 'completed'
        ),
        leaderboard_summary AS (
            SELECT
                MIN(best_rank_score) AS best_rank_score_min,
                MAX(best_rank_score) AS best_rank_score_max,
                AVG(best_rank_score) AS best_rank_score_mean
            FROM leaderboard_entries
            WHERE challenge_id = $1
              AND ($2::TEXT IS NULL OR target = $2)
        )
        SELECT
            sc.agent_count,
            sc.solution_submission_count,
            sc.completed_solution_submission_count,
            sc.failed_solution_submission_count,
            sc.queued_or_running_solution_submission_count,
            sc.visible_solution_submission_count,
            jc.validation_run_count,
            jc.official_run_count,
            sc.latest_solution_submission_at,
            lce.latest_completed_evaluation_at,
            ls.best_rank_score_min,
            ls.best_rank_score_max,
            ls.best_rank_score_mean
        FROM submission_counts sc
        CROSS JOIN job_counts jc
        CROSS JOIN latest_completed_evaluation lce
        CROSS JOIN leaderboard_summary ls
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(target_raw)
    .fetch_one(pool)
    .await?;

    Ok(CreatorChallengeStatsResponse {
        challenge_id: challenge_id.clone(),
        target: target.cloned(),
        agent_count: row.try_get("agent_count")?,
        solution_submission_count: row.try_get("solution_submission_count")?,
        completed_solution_submission_count: row.try_get("completed_solution_submission_count")?,
        failed_solution_submission_count: row.try_get("failed_solution_submission_count")?,
        queued_or_running_solution_submission_count: row
            .try_get("queued_or_running_solution_submission_count")?,
        visible_solution_submission_count: row.try_get("visible_solution_submission_count")?,
        validation_run_count: row.try_get("validation_run_count")?,
        official_run_count: row.try_get("official_run_count")?,
        latest_solution_submission_at: optional_datetime_rfc3339(
            &row,
            "latest_solution_submission_at",
        )?,
        latest_completed_evaluation_at: optional_datetime_rfc3339(
            &row,
            "latest_completed_evaluation_at",
        )?,
        best_rank_score_min: row.try_get("best_rank_score_min")?,
        best_rank_score_max: row.try_get("best_rank_score_max")?,
        best_rank_score_mean: row.try_get("best_rank_score_mean")?,
    })
}

/// Challenge-owner participant rows for one challenge and optional target.
pub async fn list_creator_challenge_participants(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: Option<&TargetName>,
) -> Result<CreatorChallengeParticipantsResponse> {
    let target_raw = target.map(TargetName::as_str);
    let rows = sqlx::query(
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (s.agent_id)
                s.agent_id, s.status AS latest_status, s.created_at AS latest_solution_submission_at
            FROM solution_submissions s
            WHERE s.challenge_id = $1
              AND ($2::TEXT IS NULL OR s.target = $2)
            ORDER BY s.agent_id, s.created_at DESC
        ),
        counts AS (
            SELECT s.agent_id, COUNT(*)::BIGINT AS solution_submission_count
            FROM solution_submissions s
            WHERE s.challenge_id = $1
              AND ($2::TEXT IS NULL OR s.target = $2)
            GROUP BY s.agent_id
        ),
        best AS (
            SELECT DISTINCT ON (le.agent_id)
                le.agent_id, le.best_solution_submission_id, le.best_rank_score
            FROM leaderboard_entries le
            WHERE le.challenge_id = $1
              AND ($2::TEXT IS NULL OR le.target = $2)
            ORDER BY le.agent_id, le.best_rank_score DESC, le.updated_at ASC
        )
        SELECT
            a.id AS agent_id,
            a.name AS agent_name,
            c.solution_submission_count,
            b.best_solution_submission_id,
            b.best_rank_score,
            l.latest_status,
            l.latest_solution_submission_at
        FROM counts c
        JOIN agents a ON a.id = c.agent_id
        LEFT JOIN best b ON b.agent_id = c.agent_id
        LEFT JOIN latest l ON l.agent_id = c.agent_id
        ORDER BY b.best_rank_score DESC NULLS LAST, c.solution_submission_count DESC, a.name ASC
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(target_raw)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            Ok(CreatorChallengeParticipantDto {
                agent_id: row.try_get("agent_id")?,
                agent_name: row.try_get("agent_name")?,
                solution_submission_count: row.try_get("solution_submission_count")?,
                best_solution_submission_id: optional_solution_submission_id_from_row(
                    &row,
                    "best_solution_submission_id",
                )?,
                best_rank_score: row.try_get("best_rank_score")?,
                latest_status: row.try_get("latest_status")?,
                latest_solution_submission_at: optional_datetime_rfc3339(
                    &row,
                    "latest_solution_submission_at",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CreatorChallengeParticipantsResponse {
        challenge_id: challenge_id.clone(),
        target: target.cloned(),
        items,
    })
}

/// List active challenges with their published benchmark contract.
pub async fn list_published_challenges(pool: &PgPool) -> Result<Vec<ChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT id, title, summary, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let spec: ChallengeBundleSpec = serde_json::from_value(r.try_get("spec_json")?)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(ChallengeListItemDto {
                id: challenge_id_from_row(&r, "id")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                starts_at: spec.starts_at,
                closes_at: spec.closes_at,
                eligibility: spec.eligibility,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Fetch one active challenge by id.
pub async fn get_published_challenge(
    pool: &PgPool,
    challenge_id: &ChallengeId,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id AS challenge_id, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND id = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_id.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Fetch one public challenge detail by id, including archived records
/// that are hidden from default browsing.
pub async fn get_public_challenge(
    pool: &PgPool,
    challenge_id: &ChallengeId,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id AS challenge_id, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status IN ('active', 'archived')
          AND spec_json IS NOT NULL
          AND id = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_id.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

fn row_to_challenge_record(r: sqlx::postgres::PgRow) -> Result<ChallengeRecord> {
    Ok(ChallengeRecord {
        challenge_id: challenge_id_from_row(&r, "challenge_id")?,
        title: r.try_get("title")?,
        summary: r.try_get("summary")?,
        bundle_path: r.try_get("bundle_path")?,
        statement_path: r.try_get("statement_path")?,
        spec_json: r.try_get("spec_json")?,
    })
}

fn row_to_shortlist_revision_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeShortlistRevisionResponse> {
    Ok(ChallengeShortlistRevisionResponse {
        id: row.try_get("id")?,
        challenge_id: challenge_id_from_row(&row, "challenge_id")?,
        uploader_agent_id: row.try_get("uploader_agent_id")?,
        requested_count: row.try_get("requested_count")?,
        added_count: row.try_get("added_count")?,
        sha256: row.try_get("sha256")?,
        storage_uri: row.try_get("storage_uri")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn optional_datetime_rfc3339(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>> {
    Ok(row
        .try_get::<Option<DateTime<Utc>>, _>(column)?
        .map(|value| value.to_rfc3339()))
}
