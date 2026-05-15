use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{AppError, Result};
use crate::leaderboard::should_replace_leaderboard_entry;
use crate::models::challenge::{ChallengeBundleSpec, MetricDirection};
use crate::models::evaluation::{MetricValue, PublicCaseResult};
use crate::models::ids::{ChallengeId, SolutionSubmissionId, TargetName};
use crate::models::request::LeaderboardEntryDto;

use super::challenges::get_published_challenge;
use super::ids::{solution_submission_id_from_row, target_from_row};
use super::json::decode_optional_json;

/// Hide a solution submission and repair or remove the affected leaderboard entry.
pub async fn hide_solution_submission(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    let row: Option<(String, String, String)> = sqlx::query_as(
        "UPDATE solution_submissions SET visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1 RETURNING challenge_id, target, agent_id"
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let Some((challenge_id, target, agent_id)) = row else {
        return Err(AppError::NotFound);
    };

    let leaderboard_entry: Option<(String,)> = sqlx::query_as(
        "SELECT best_solution_submission_id FROM leaderboard_entries WHERE challenge_id = $1 AND target = $2 AND agent_id = $3 LIMIT 1"
    )
    .bind(&challenge_id)
    .bind(&target)
    .bind(&agent_id)
    .fetch_optional(&mut *tx)
    .await?;

    if leaderboard_entry
        .map(|e| e.0 == solution_submission_id.as_str())
        .unwrap_or(false)
    {
        let replacement: Option<(String, f64, Value, Value, Option<f64>, Value)> = sqlx::query_as(
            r#"
            SELECT
                s.id,
                COALESCE(
                    ve.rank_score,
                    oe.rank_score,
                    (ve.validation_summary_json->>'score')::double precision,
                    (oe.official_summary_json->>'score')::double precision
                ) AS ranking_score,
                COALESCE(ve.public_results_json, oe.public_results_json, '[]'::jsonb) AS public_results,
                COALESCE(ve.aggregate_metrics_json, oe.aggregate_metrics_json, '[]'::jsonb) AS aggregate_metrics,
                COALESCE(oe.rank_score, (oe.official_summary_json->>'score')::double precision) AS official_score,
                COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
            FROM solution_submissions s
            LEFT JOIN LATERAL (
                SELECT rank_score, aggregate_metrics_json, validation_summary_json, public_results_json
                FROM evaluations
                WHERE solution_submission_id = s.id AND eval_type = 'validation' AND status = 'completed' AND target = s.target
                ORDER BY created_at DESC LIMIT 1
            ) ve ON TRUE
            LEFT JOIN LATERAL (
                SELECT rank_score, aggregate_metrics_json, official_summary_json, public_results_json
                FROM evaluations
                WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed' AND target = s.target
                ORDER BY created_at DESC LIMIT 1
            ) oe ON TRUE
            WHERE s.challenge_id = $1 AND s.agent_id = $2 AND s.id <> $3
              AND s.target = $4
              AND s.visible_after_eval = TRUE AND s.status = 'completed'
              AND COALESCE(
                    ve.rank_score,
                    oe.rank_score,
                    (ve.validation_summary_json->>'score')::double precision,
                    (oe.official_summary_json->>'score')::double precision
                  ) IS NOT NULL
            ORDER BY ranking_score DESC, s.created_at ASC
            LIMIT 1
            "#
        )
        .bind(&challenge_id)
        .bind(&agent_id)
        .bind(solution_submission_id.as_str())
        .bind(&target)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((
            best_id,
            best_score,
            public_results,
            aggregate_metrics,
            official_score,
            official_metrics,
        )) = replacement
        {
            sqlx::query(
                r#"
                INSERT INTO leaderboard_entries (
                    challenge_id, target, agent_id, best_solution_submission_id, best_rank_score,
                    public_results_json, aggregate_metrics_json, official_score,
                    official_metrics_json, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
                ON CONFLICT (challenge_id, target, agent_id) DO UPDATE
                SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
                    best_rank_score = EXCLUDED.best_rank_score,
                    public_results_json = EXCLUDED.public_results_json,
                    aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
                    official_score = EXCLUDED.official_score,
                    official_metrics_json = EXCLUDED.official_metrics_json,
                    updated_at = NOW()
                "#,
            )
            .bind(&challenge_id)
            .bind(&target)
            .bind(&agent_id)
            .bind(&best_id)
            .bind(best_score)
            .bind(&public_results)
            .bind(&aggregate_metrics)
            .bind(official_score)
            .bind(&official_metrics)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM leaderboard_entries WHERE challenge_id = $1 AND target = $2 AND agent_id = $3",
            )
            .bind(&challenge_id)
            .bind(&target)
            .bind(&agent_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// List leaderboard entries for a challenge id.
pub async fn list_leaderboard_entries(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<LeaderboardEntryDto>> {
    let requested_limit = limit.max(1);
    let fetch_limit = requested_limit.saturating_mul(5).clamp(1, 10_000);
    let spec = get_published_challenge(pool, challenge_id)
        .await?
        .and_then(|challenge| {
            serde_json::from_value::<ChallengeBundleSpec>(challenge.spec_json).ok()
        });
    let rows = sqlx::query(
        r#"
        SELECT
            le.target, le.agent_id, a.name AS agent_name, le.best_solution_submission_id,
            le.best_rank_score, le.aggregate_metrics_json, le.official_score,
            le.official_metrics_json, le.updated_at
        FROM leaderboard_entries le
        JOIN agents a ON a.id = le.agent_id
        JOIN challenges p ON p.id = le.challenge_id
        WHERE p.id = $1
          AND le.target = $2
        ORDER BY le.best_rank_score DESC, le.updated_at ASC
        LIMIT $3
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(target.as_str())
    .bind(fetch_limit)
    .fetch_all(pool)
    .await?;

    let mut entries = rows
        .into_iter()
        .map(|r| {
            let aggregate_metrics = decode_optional_json(
                r.try_get::<Option<Value>, _>("aggregate_metrics_json")?,
                "leaderboard aggregate metrics",
            )?
            .unwrap_or_default();
            let official_metrics = decode_optional_json(
                r.try_get::<Option<Value>, _>("official_metrics_json")?,
                "leaderboard official metrics",
            )?
            .unwrap_or_default();
            let best_rank_score: f64 = r.try_get("best_rank_score")?;

            Ok(LeaderboardEntryDto {
                target: target_from_row(&r, "target")?,
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                best_solution_submission_id: solution_submission_id_from_row(
                    &r,
                    "best_solution_submission_id",
                )?,
                best_rank_score,
                rank_score: best_rank_score,
                aggregate_metrics,
                official_metrics,
                official_score: r.try_get::<Option<f64>, _>("official_score")?,
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(spec) = spec {
        entries.sort_by(|a, b| compare_leaderboard_entries(&spec, a, b));
    }
    entries.truncate(usize::try_from(requested_limit).unwrap_or(usize::MAX));

    Ok(entries)
}

pub(super) async fn upsert_leaderboard_entry_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    target: &TargetName,
    rank_score: f64,
    public_results: &[PublicCaseResult],
    aggregate_metrics: &[MetricValue],
) -> Result<bool> {
    let row: Option<(String, String, Value)> = sqlx::query_as(
        r#"
        SELECT s.challenge_id, s.agent_id, p.spec_json
        FROM solution_submissions s
        JOIN challenges p ON p.id = s.challenge_id
        WHERE s.id = $1
        LIMIT 1
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_id, agent_id, spec_json)) = row else {
        return Ok(false);
    };
    let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json).ok();

    let current: Option<(f64, Value)> = sqlx::query_as(
        "SELECT best_rank_score, aggregate_metrics_json FROM leaderboard_entries WHERE challenge_id = $1 AND target = $2 AND agent_id = $3 LIMIT 1"
    )
    .bind(&challenge_id)
    .bind(target.as_str())
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await?;
    let current: Option<(f64, Vec<MetricValue>)> = current
        .map(|(score, metrics_json)| {
            decode_optional_json(Some(metrics_json), "leaderboard aggregate metrics")
                .map(|metrics| (score, metrics.unwrap_or_default()))
        })
        .transpose()?;

    if !candidate_replaces_leaderboard_entry(spec.as_ref(), current, rank_score, aggregate_metrics)
    {
        return Ok(false);
    }

    let public_results_json =
        serde_json::to_value(public_results).map_err(|e| AppError::Internal(e.to_string()))?;
    let aggregate_metrics_json =
        serde_json::to_value(aggregate_metrics).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_id, target, agent_id, best_solution_submission_id, best_rank_score,
            public_results_json, aggregate_metrics_json, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
        ON CONFLICT (challenge_id, target, agent_id) DO UPDATE
        SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
            best_rank_score = EXCLUDED.best_rank_score,
            public_results_json = EXCLUDED.public_results_json,
            aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
            updated_at = NOW()
        "#,
    )
    .bind(&challenge_id)
    .bind(target.as_str())
    .bind(&agent_id)
    .bind(solution_submission_id.as_str())
    .bind(rank_score)
    .bind(&public_results_json)
    .bind(&aggregate_metrics_json)
    .execute(&mut **tx)
    .await?;

    Ok(true)
}

pub(super) async fn update_official_score_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    target: &TargetName,
    official_score: f64,
    official_metrics: &[MetricValue],
) -> Result<()> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT challenge_id, agent_id FROM solution_submissions WHERE id = $1 LIMIT 1",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_id, agent_id)) = row else {
        return Ok(());
    };

    let official_metrics_json =
        serde_json::to_value(official_metrics).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        "UPDATE leaderboard_entries SET official_score = $4, official_metrics_json = $5, updated_at = NOW() WHERE challenge_id = $1 AND target = $2 AND agent_id = $3"
    )
    .bind(&challenge_id)
    .bind(target.as_str())
    .bind(&agent_id)
    .bind(official_score)
    .bind(&official_metrics_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn compare_leaderboard_entries(
    spec: &ChallengeBundleSpec,
    a: &LeaderboardEntryDto,
    b: &LeaderboardEntryDto,
) -> Ordering {
    compare_rank_payloads(
        spec,
        a.rank_score,
        &a.aggregate_metrics,
        b.rank_score,
        &b.aggregate_metrics,
    )
    .then_with(|| a.updated_at.cmp(&b.updated_at))
    .then_with(|| a.agent_name.cmp(&b.agent_name))
    .then_with(|| {
        a.best_solution_submission_id
            .cmp(&b.best_solution_submission_id)
    })
}

fn compare_rank_payloads(
    spec: &ChallengeBundleSpec,
    a_score: f64,
    a_metrics: &[MetricValue],
    b_score: f64,
    b_metrics: &[MetricValue],
) -> Ordering {
    let score_order = compare_f64_desc(a_score, b_score);
    if score_order != Ordering::Equal {
        return score_order;
    }

    for metric_id in &spec.metric_schema.ranking.tie_breaker_metric_ids {
        let Some(definition) = spec.metric_schema.metric(metric_id) else {
            continue;
        };
        let ordering = compare_metric_by_direction(
            definition.direction,
            metric_value(a_metrics, metric_id),
            metric_value(b_metrics, metric_id),
        );
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    Ordering::Equal
}

fn candidate_replaces_leaderboard_entry(
    spec: Option<&ChallengeBundleSpec>,
    current: Option<(f64, Vec<MetricValue>)>,
    candidate_score: f64,
    candidate_metrics: &[MetricValue],
) -> bool {
    let Some((current_score, current_metrics)) = current else {
        return true;
    };

    if let Some(spec) = spec {
        return compare_rank_payloads(
            spec,
            candidate_score,
            candidate_metrics,
            current_score,
            &current_metrics,
        ) == Ordering::Less;
    }

    should_replace_leaderboard_entry(Some(current_score), candidate_score)
}

fn metric_value(metrics: &[MetricValue], metric_id: &str) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| metric.metric_id == metric_id)
        .map(|metric| metric.value)
}

fn compare_metric_by_direction(
    direction: MetricDirection,
    a: Option<f64>,
    b: Option<f64>,
) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => match direction {
            MetricDirection::Maximize => compare_f64_desc(a, b),
            MetricDirection::Minimize => compare_f64_asc(a, b),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_f64_desc(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

fn compare_f64_asc(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}
