use sqlx::{PgPool, Row};

use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_error::{Result, ServiceError};

use super::catalog::get_public_challenge;
use super::helpers::optional_solution_submission_status_from_row;
use super::records::{
    CreatorChallengeParticipantRecord, CreatorChallengeParticipantsRecord,
    CreatorChallengeStatsRecord,
};
use crate::db::ids::{agent_id_from_row, optional_solution_submission_id_from_row};

/// Challenge-owner statistics for one challenge and optional target.
pub async fn get_creator_challenge_stats(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: Option<&TargetName>,
) -> Result<CreatorChallengeStatsRecord> {
    let challenge = get_public_challenge(pool, challenge_name)
        .await?
        .ok_or(ServiceError::NotFound)?;
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

    Ok(CreatorChallengeStatsRecord {
        challenge_name: challenge.challenge_name,
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
        latest_solution_submission_at: row.try_get("latest_solution_submission_at")?,
        latest_completed_evaluation_at: row.try_get("latest_completed_evaluation_at")?,
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
) -> Result<CreatorChallengeParticipantsRecord> {
    let challenge = get_public_challenge(pool, challenge_name)
        .await?
        .ok_or(ServiceError::NotFound)?;
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
            Ok(CreatorChallengeParticipantRecord {
                agent_id: agent_id_from_row(&row, "agent_id")?,
                agent_display_name: row.try_get("agent_display_name")?,
                solution_submission_count: row.try_get("solution_submission_count")?,
                best_solution_submission_id: optional_solution_submission_id_from_row(
                    &row,
                    "best_solution_submission_id",
                )?,
                best_rank_score: row.try_get("best_rank_score")?,
                latest_status: optional_solution_submission_status_from_row(&row, "latest_status")?,
                latest_solution_submission_at: row.try_get("latest_solution_submission_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CreatorChallengeParticipantsRecord {
        challenge_name: challenge.challenge_name,
        target: target.cloned(),
        items,
    })
}
