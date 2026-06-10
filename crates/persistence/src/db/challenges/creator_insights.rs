use std::cmp::Ordering;

use serde_json::Value;
use sqlx::{PgPool, Row};

use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::evaluation::{MetricValue, compare_metric_payloads_by_ranking};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_error::{Result, ServiceError};

use super::catalog::get_public_challenge;
use super::helpers::optional_solution_submission_status_from_row;
use super::records::{
    CreatorChallengeParticipantRecord, CreatorChallengeParticipantsRecord,
    CreatorChallengeStatsRecord,
};
use crate::db::ids::{agent_id_from_row, optional_solution_submission_id_from_row};
use crate::db::json::decode_optional_json;

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
        WITH challenge_scope AS (
            SELECT
                challenge_name,
                spec_json #>> '{metric_schema,ranking,primary_metric_name}' AS primary_metric_name
            FROM challenges
            WHERE challenge_name = $1
        ),
        filtered_submissions AS (
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
                MIN(pm.value) AS primary_metric_min,
                MAX(pm.value) AS primary_metric_max,
                AVG(pm.value) AS primary_metric_mean
            FROM leaderboard_entries le
            JOIN challenge_scope cs ON cs.challenge_name = le.challenge_name
            JOIN LATERAL (
                SELECT (metric->>'value')::DOUBLE PRECISION AS value
                FROM jsonb_array_elements(COALESCE(le.official_metrics_json, '[]'::jsonb)) AS metric
                WHERE metric->>'metric_name' = cs.primary_metric_name
                LIMIT 1
            ) pm ON TRUE
            WHERE le.challenge_name = $1
              AND ($2::TEXT IS NULL OR le.target = $2)
        )
        SELECT
            cs.primary_metric_name,
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
            ls.primary_metric_min,
            ls.primary_metric_max,
            ls.primary_metric_mean
        FROM challenge_scope cs
        CROSS JOIN submission_counts sc
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
        primary_metric_name: metric_name_from_row(&row, "primary_metric_name")?,
        primary_metric_min: row.try_get("primary_metric_min")?,
        primary_metric_max: row.try_get("primary_metric_max")?,
        primary_metric_mean: row.try_get("primary_metric_mean")?,
    })
}

fn metric_name_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<MetricName> {
    let value: String = row.try_get(column)?;
    MetricName::try_new(value)
        .map_err(|error| ServiceError::Internal(format!("stored invalid metric name: {error}")))
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
    let spec: ChallengeBundleSpec =
        serde_json::from_value(challenge.spec_json.clone()).map_err(|error| {
            ServiceError::Internal(format!("stored challenge spec is invalid: {error}"))
        })?;
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
        )
        SELECT
            a.id::text AS agent_id,
            a.display_name AS agent_display_name,
            c.solution_submission_count,
            le.best_solution_submission_id,
            le.aggregate_metrics_json,
            le.official_metrics_json,
            le.updated_at AS best_updated_at,
            l.latest_status,
            l.latest_solution_submission_at
        FROM counts c
        JOIN agents a ON a.id = c.agent_id
        LEFT JOIN leaderboard_entries le
          ON le.challenge_name = $1
         AND le.agent_id = c.agent_id
         AND ($2::TEXT IS NULL OR le.target = $2)
        LEFT JOIN latest l ON l.agent_id = c.agent_id
        ORDER BY a.display_name ASC
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(target_raw)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::new();
    for row in rows {
        let agent_id = agent_id_from_row(&row, "agent_id")?;
        let aggregate_metrics = decode_metric_payload(
            &row,
            "aggregate_metrics_json",
            "participant aggregate metrics",
        )?;
        let official_metrics = decode_metric_payload(
            &row,
            "official_metrics_json",
            "participant official metrics",
        )?;
        let candidate = CreatorChallengeParticipantRecord {
            agent_id: agent_id.clone(),
            agent_display_name: row.try_get("agent_display_name")?,
            solution_submission_count: row.try_get("solution_submission_count")?,
            best_solution_submission_id: optional_solution_submission_id_from_row(
                &row,
                "best_solution_submission_id",
            )?,
            best_primary_metric: MetricValue::find_by_name(
                official_metrics.as_deref().unwrap_or_default(),
                &spec.metric_schema.ranking.primary_metric_name,
            ),
            best_aggregate_metrics: aggregate_metrics,
            best_updated_at: row.try_get("best_updated_at")?,
            latest_status: optional_solution_submission_status_from_row(&row, "latest_status")?,
            latest_solution_submission_at: row.try_get("latest_solution_submission_at")?,
        };
        upsert_participant_candidate(&mut items, candidate, &spec);
    }
    items.sort_by(|a, b| compare_creator_participants(&spec, a, b));

    Ok(CreatorChallengeParticipantsRecord {
        challenge_name: challenge.challenge_name,
        target: target.cloned(),
        items,
    })
}

fn decode_metric_payload(
    row: &sqlx::postgres::PgRow,
    column: &str,
    label: &str,
) -> Result<Option<Vec<MetricValue>>> {
    decode_optional_json(row.try_get::<Option<Value>, _>(column)?, label)
}

fn upsert_participant_candidate(
    items: &mut Vec<CreatorChallengeParticipantRecord>,
    candidate: CreatorChallengeParticipantRecord,
    spec: &ChallengeBundleSpec,
) {
    let Some(existing) = items
        .iter_mut()
        .find(|item| item.agent_id == candidate.agent_id)
    else {
        items.push(candidate);
        return;
    };

    if compare_participant_best(spec, &candidate, existing) == Ordering::Less {
        *existing = candidate;
    }
}

fn compare_creator_participants(
    spec: &ChallengeBundleSpec,
    a: &CreatorChallengeParticipantRecord,
    b: &CreatorChallengeParticipantRecord,
) -> Ordering {
    compare_participant_best(spec, a, b)
        .then_with(|| {
            b.solution_submission_count
                .cmp(&a.solution_submission_count)
        })
        .then_with(|| a.agent_display_name.cmp(&b.agent_display_name))
        .then_with(|| a.agent_id.cmp(&b.agent_id))
}

fn compare_participant_best(
    spec: &ChallengeBundleSpec,
    a: &CreatorChallengeParticipantRecord,
    b: &CreatorChallengeParticipantRecord,
) -> Ordering {
    match (
        a.best_aggregate_metrics.as_deref(),
        b.best_aggregate_metrics.as_deref(),
    ) {
        (Some(a_metrics), Some(b_metrics)) => {
            compare_metric_payloads_by_ranking(&spec.metric_schema, a_metrics, b_metrics)
                .then_with(|| a.best_updated_at.cmp(&b.best_updated_at))
                .then_with(|| {
                    a.best_solution_submission_id
                        .cmp(&b.best_solution_submission_id)
                })
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
