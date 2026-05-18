//! Challenge shell and published challenge queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use super::ids::{
    agent_id_from_row, challenge_name_from_row, challenge_shortlist_revision_id_from_row,
    optional_solution_submission_id_from_row,
};
use crate::error::{AppError, Result};
use crate::models::challenge::{
    AdminChallengeListItemDto, ChallengeBundleSpec, ChallengeLifecycleStatus, ChallengeListItemDto,
    PublishChallengeResponse,
};
use crate::models::evaluation::SolutionSubmissionStatus;
use crate::models::hashes::Sha256Digest;
use crate::models::ids::{AgentId, ChallengeShortlistRevisionId};
use crate::models::names::{ChallengeName, TargetName};
use crate::models::paths::{ManagedBundlePath, ManagedStatementPath};
use crate::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse, ChallengeShortlistedAgentDto,
    CreatorChallengeParticipantDto, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse,
};
use crate::storage::StorageKey;

/// Published challenge list plus the unbounded count for pagination previews.
#[derive(Debug, Clone)]
pub struct PublishedChallengeList {
    pub items: Vec<ChallengeListItemDto>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

/// Published challenge joined with challenge metadata.
#[derive(Debug, Clone)]
pub struct ChallengeRecord {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: String,
    pub bundle_path: ManagedBundlePath,
    pub statement_path: ManagedStatementPath,
    pub spec_json: Value,
}

/// Create or update an unpublished challenge shell.
pub async fn create_or_update_challenge(
    pool: &PgPool,
    name: &ChallengeName,
    title: &str,
    summary: &str,
) -> Result<crate::models::challenge::ChallengeAdminResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO challenges (name, title, summary, status)
        VALUES ($1, $2, $3, 'draft')
        ON CONFLICT (name) DO UPDATE
        SET title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING name, title, summary, status, created_at, updated_at
        "#,
    )
    .bind(name.as_str())
    .bind(title)
    .bind(summary)
    .fetch_one(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => AppError::Conflict,
        error => AppError::Database(error),
    })?;

    Ok(crate::models::challenge::ChallengeAdminResponse {
        name: challenge_name_from_row(&row, "name")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        status: challenge_status_from_row(&row, "status")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// List all challenge shells for admin review.
pub async fn list_admin_challenges(pool: &PgPool) -> Result<Vec<AdminChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT name, title, summary, status, spec_json, created_at, updated_at
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
                name: challenge_name_from_row(&r, "name")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                status: challenge_status_from_row(&r, "status")?,
                targets: spec.as_ref().map(|spec| spec.targets.clone()),
                starts_at: spec.as_ref().map(|spec| spec.starts_at.clone()),
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

/// Publish a validated bundle as the benchmark contract for a challenge name.
pub async fn publish_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    bundle_path: &ManagedBundlePath,
    statement_path: &ManagedStatementPath,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<PublishChallengeResponse> {
    let mut tx = pool.begin().await?;
    let response = publish_challenge_tx(
        &mut tx,
        challenge_name,
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

/// Handles publish challenge tx for this module.
pub async fn publish_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    bundle_path: &ManagedBundlePath,
    statement_path: &ManagedStatementPath,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<PublishChallengeResponse> {
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenges (
            name, title, summary, bundle_path, statement_path, spec_json,
            starts_at, closes_at, eligibility_policy_json, validation_submission_limit,
            official_submission_limit, leaderboard_visibility, score_distribution_visibility,
            result_detail_visibility, solution_publication_policy, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'active')
        ON CONFLICT (name) DO UPDATE
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
        RETURNING name AS challenge_name, title, bundle_path, statement_path
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(title)
    .bind(summary)
    .bind(bundle_path.as_str()?)
    .bind(statement_path.as_str()?)
    .bind(&spec_json)
    .bind(parse_required_time(&spec.starts_at)?)
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
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        title: row.try_get("title")?,
        bundle_path: managed_bundle_path_from_row(&row, "bundle_path")?,
        statement_path: managed_statement_path_from_row(&row, "statement_path")?,
    })
}

/// Parses required time from an external boundary string.
fn parse_required_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("invalid challenge timestamp: {e}")))
}

/// Parses optional time from an external boundary string.
fn parse_optional_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("invalid challenge timestamp: {e}")))
        })
        .transpose()
}

/// Converts this value to json string.
fn to_json_string<T: serde::Serialize>(value: T) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|e| AppError::Internal(e.to_string()))?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Internal("challenge enum did not serialize to string".to_string()))
}

/// Archive a challenge shell while preserving private assets and historical submissions.
pub async fn archive_challenge(pool: &PgPool, challenge_name: &ChallengeName) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE name = $1
        "#,
    )
    .bind(challenge_name.as_str())
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
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    add_challenge_owner_tx(&mut tx, challenge_name, agent_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Handles add challenge owner tx for this module.
pub async fn add_challenge_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO challenge_owners (challenge_name, agent_id)
        VALUES ($1, $2::uuid)
        ON CONFLICT (challenge_name, agent_id) DO NOTHING
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(agent_id.as_str())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Check whether an agent is an owner of a challenge.
pub async fn agent_owns_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
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

/// Input for one shortlist delta revision.
#[derive(Debug, Clone)]
pub struct CreateChallengeShortlistRevisionInput {
    pub revision_id: ChallengeShortlistRevisionId,
    pub challenge_name: ChallengeName,
    pub uploader_agent_id: AgentId,
    pub storage_key: StorageKey,
    pub sha256: Sha256Digest,
    pub requested_count: i64,
    pub agent_ids_to_add: Vec<AgentId>,
}

/// Persist a shortlist delta and append any new agent ids to the effective shortlist.
pub async fn create_challenge_shortlist_revision(
    pool: &PgPool,
    input: &CreateChallengeShortlistRevisionInput,
) -> Result<ChallengeShortlistRevisionResponse> {
    let mut tx = pool.begin().await?;

    lock_challenge_shortlist(&mut tx, &input.challenge_name).await?;
    ensure_shortlist_agents_exist(&mut tx, &input.agent_ids_to_add).await?;

    sqlx::query(
        r#"
        INSERT INTO challenge_shortlist_revisions (
            id, challenge_name, uploader_agent_id, storage_key, sha256, requested_count, added_count
        )
        VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, 0)
        "#,
    )
    .bind(input.revision_id.as_str())
    .bind(input.challenge_name.as_str())
    .bind(input.uploader_agent_id.as_str())
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
                challenge_name, agent_id, added_by_agent_id, source_revision_id
            )
            VALUES ($1, $2::uuid, $3::uuid, $4::uuid)
            ON CONFLICT (challenge_name, agent_id) DO NOTHING
            "#,
        )
        .bind(input.challenge_name.as_str())
        .bind(agent_id.as_str())
        .bind(input.uploader_agent_id.as_str())
        .bind(input.revision_id.as_str())
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
        WHERE id = $1::uuid
        RETURNING id, challenge_name, uploader_agent_id, storage_key, sha256, requested_count, added_count, created_at
        "#,
    )
    .bind(input.revision_id.as_str())
    .bind(added_count)
    .fetch_one(&mut *tx)
    .await?;

    let response = row_to_shortlist_revision_response(row)?;
    tx.commit().await?;
    Ok(response)
}

/// Handles lock challenge shortlist for this module.
async fn lock_challenge_shortlist(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<()> {
    sqlx::query("SELECT name FROM challenges WHERE name = $1 FOR UPDATE")
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
    challenge_name: &ChallengeName,
) -> Result<ChallengeShortlistResponse> {
    let rows = sqlx::query(
        r#"
        SELECT s.agent_id::text AS agent_id, a.display_name AS agent_display_name, s.added_by_agent_id::text AS added_by_agent_id, s.created_at
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
            Ok(ChallengeShortlistedAgentDto {
                agent_id: agent_id_from_row(&row, "agent_id")?,
                agent_display_name: row.try_get("agent_display_name")?,
                added_by_agent_id: agent_id_from_row(&row, "added_by_agent_id")?,
                created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ChallengeShortlistResponse {
        challenge_name: challenge_name.clone(),
        items,
    })
}

/// Challenge-owner statistics for one challenge and optional target.
pub async fn get_creator_challenge_stats(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: Option<&TargetName>,
) -> Result<CreatorChallengeStatsResponse> {
    let target_raw = target.map(TargetName::as_str);
    let row = sqlx::query(
        r#"
        WITH filtered_submissions AS (
            SELECT id, agent_id, status, visible_after_eval, created_at
            FROM solution_submissions
            WHERE challenge_name = $1
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
            WHERE challenge_name = $1
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
    .bind(challenge_name.as_str())
    .bind(target_raw)
    .fetch_one(pool)
    .await?;

    Ok(CreatorChallengeStatsResponse {
        challenge_name: challenge_name.clone(),
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
    challenge_name: &ChallengeName,
    target: Option<&TargetName>,
) -> Result<CreatorChallengeParticipantsResponse> {
    let target_raw = target.map(TargetName::as_str);
    let rows = sqlx::query(
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (s.agent_id)
                s.agent_id, s.status AS latest_status, s.created_at AS latest_solution_submission_at
            FROM solution_submissions s
            WHERE s.challenge_name = $1
              AND ($2::TEXT IS NULL OR s.target = $2)
            ORDER BY s.agent_id, s.created_at DESC
        ),
        counts AS (
            SELECT s.agent_id, COUNT(*)::BIGINT AS solution_submission_count
            FROM solution_submissions s
            WHERE s.challenge_name = $1
              AND ($2::TEXT IS NULL OR s.target = $2)
            GROUP BY s.agent_id
        ),
        best AS (
            SELECT DISTINCT ON (le.agent_id)
                le.agent_id, le.best_solution_submission_id, le.best_rank_score
            FROM leaderboard_entries le
            WHERE le.challenge_name = $1
              AND ($2::TEXT IS NULL OR le.target = $2)
            ORDER BY le.agent_id, le.best_rank_score DESC, le.updated_at ASC
        )
        SELECT
            a.id::text AS agent_id,
            a.display_name AS agent_display_name,
            c.solution_submission_count,
            b.best_solution_submission_id,
            b.best_rank_score,
            l.latest_status,
            l.latest_solution_submission_at
        FROM counts c
        JOIN agents a ON a.id = c.agent_id
        LEFT JOIN best b ON b.agent_id = c.agent_id
        LEFT JOIN latest l ON l.agent_id = c.agent_id
        ORDER BY b.best_rank_score DESC NULLS LAST, c.solution_submission_count DESC, a.display_name ASC
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(target_raw)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            Ok(CreatorChallengeParticipantDto {
                agent_id: agent_id_from_row(&row, "agent_id")?,
                agent_display_name: row.try_get("agent_display_name")?,
                solution_submission_count: row.try_get("solution_submission_count")?,
                best_solution_submission_id: optional_solution_submission_id_from_row(
                    &row,
                    "best_solution_submission_id",
                )?,
                best_rank_score: row.try_get("best_rank_score")?,
                latest_status: optional_solution_submission_status_from_row(&row, "latest_status")?,
                latest_solution_submission_at: optional_datetime_rfc3339(
                    &row,
                    "latest_solution_submission_at",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CreatorChallengeParticipantsResponse {
        challenge_name: challenge_name.clone(),
        target: target.cloned(),
        items,
    })
}

/// List active challenges with their published benchmark contract.
pub async fn list_published_challenges(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<PublishedChallengeList> {
    let total_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
        "#,
    )
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT name, title, summary, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
        ORDER BY created_at DESC, name ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| {
            let spec: ChallengeBundleSpec = serde_json::from_value(r.try_get("spec_json")?)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(ChallengeListItemDto {
                name: challenge_name_from_row(&r, "name")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                starts_at: spec.starts_at,
                closes_at: spec.closes_at,
                eligibility: spec.eligibility,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let returned_count = i64::try_from(items.len())
        .map_err(|_| AppError::Internal("challenge list length overflow".to_string()))?;
    let consumed = offset
        .checked_add(returned_count)
        .ok_or_else(|| AppError::Internal("challenge list offset overflow".to_string()))?;
    Ok(PublishedChallengeList {
        items,
        total_count,
        limit,
        offset,
        has_more: consumed < total_count,
    })
}

/// Fetch one active challenge by name.
pub async fn get_published_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT name AS challenge_name, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND name = $1
        LIMIT 1
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Fetch one public challenge detail by name, including archived records
/// that are hidden from default browsing.
pub async fn get_public_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT name AS challenge_name, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status IN ('active', 'archived')
          AND spec_json IS NOT NULL
          AND name = $1
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
        summary: r.try_get("summary")?,
        bundle_path: managed_bundle_path_from_row(&r, "bundle_path")?,
        statement_path: managed_statement_path_from_row(&r, "statement_path")?,
        spec_json: r.try_get("spec_json")?,
    })
}

/// Reads managed bundle path from a database row and validates its domain shape.
fn managed_bundle_path_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ManagedBundlePath> {
    let value: String = row.try_get(column)?;
    ManagedBundlePath::from_existing_dir(value)
        .map_err(|e| AppError::Internal(format!("stored invalid {column}: {e}")))
}

/// Reads managed statement path from a database row and validates its domain shape.
fn managed_statement_path_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ManagedStatementPath> {
    let value: String = row.try_get(column)?;
    ManagedStatementPath::from_existing_file(value)
        .map_err(|e| AppError::Internal(format!("stored invalid {column}: {e}")))
}

/// Converts a database row into the shortlist revision response model.
fn row_to_shortlist_revision_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeShortlistRevisionResponse> {
    Ok(ChallengeShortlistRevisionResponse {
        id: challenge_shortlist_revision_id_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        uploader_agent_id: agent_id_from_row(&row, "uploader_agent_id")?,
        requested_count: row.try_get("requested_count")?,
        added_count: row.try_get("added_count")?,
        sha256: sha256_digest_from_row(&row, "sha256")?,
        storage_key: storage_key_from_row(&row, "storage_key")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

/// Reads storage key from a database row and validates its domain shape.
fn storage_key_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads a challenge lifecycle status and validates its stored value.
fn challenge_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeLifecycleStatus> {
    let value: String = row.try_get(column)?;
    ChallengeLifecycleStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unexpected challenge status `{value}`")))
}

/// Reads an optional solution-submission status for creator participant rows.
fn optional_solution_submission_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionStatus>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            SolutionSubmissionStatus::from_storage_value(&value).ok_or_else(|| {
                AppError::Internal(format!("unexpected solution submission status `{value}`"))
            })
        })
        .transpose()
}

/// Reads sha256 digest from a database row and validates its domain shape.
fn sha256_digest_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<Sha256Digest> {
    let value: String = row.try_get(column)?;
    Sha256Digest::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Handles optional datetime rfc3339 for this module.
fn optional_datetime_rfc3339(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>> {
    Ok(row
        .try_get::<Option<DateTime<Utc>>, _>(column)?
        .map(|value| value.to_rfc3339()))
}
