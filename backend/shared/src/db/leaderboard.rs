use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};

use crate::error::{AppError, Result};
use crate::models::challenge::{ChallengeBundleSpec, MetricDirection};
use crate::models::evaluation::{MetricValue, PublicCaseResult};
use crate::models::ids::SolutionSubmissionId;
use crate::models::names::{ChallengeName, MetricName, TargetName};
use crate::models::request::LeaderboardEntryDto;

use super::challenges::get_published_challenge;
use super::ids::{agent_id_from_row, solution_submission_id_from_row, target_from_row};
use super::json::decode_optional_json;

/// Hide a solution submission and repair or remove the affected leaderboard entry.
pub async fn hide_solution_submission(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE solution_submissions SET visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1::uuid RETURNING id::text"
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    if row.is_none() {
        return Err(AppError::NotFound);
    };

    repair_leaderboard_entry_for_solution_submission_tx(&mut tx, solution_submission_id).await?;

    tx.commit().await?;
    Ok(())
}

/// Repair or remove the leaderboard row touched by one submission visibility change.
pub(super) async fn repair_leaderboard_entry_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT challenge_name, target, agent_id::text AS agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1"
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_name, target, agent_id)) = row else {
        return Ok(());
    };

    let leaderboard_entry: Option<(String,)> = sqlx::query_as(
        "SELECT best_solution_submission_id::text AS best_solution_submission_id FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(&challenge_name)
    .bind(&target)
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await?;

    if leaderboard_entry
        .map(|e| e.0 == solution_submission_id.as_str())
        .unwrap_or(false)
    {
        let spec_json: Option<(Value,)> =
            sqlx::query_as("SELECT spec_json FROM challenges WHERE name = $1 LIMIT 1")
                .bind(&challenge_name)
                .fetch_optional(&mut **tx)
                .await?;
        let Some((spec_json,)) = spec_json else {
            return Ok(());
        };
        let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
            .map_err(|e| AppError::Internal(format!("stored challenge spec is invalid: {e}")))?;

        let replacement_rows = sqlx::query(
            r#"
            SELECT
                s.id::text AS id,
                s.created_at,
                COALESCE(
                    oe.rank_score,
                    ve.rank_score,
                    (oe.official_summary_json->>'score')::double precision,
                    (ve.validation_summary_json->>'score')::double precision
                ) AS ranking_score,
                COALESCE(oe.public_results_json, ve.public_results_json, '[]'::jsonb) AS public_results,
                COALESCE(oe.aggregate_metrics_json, ve.aggregate_metrics_json, '[]'::jsonb) AS aggregate_metrics,
                oe.primary_score AS official_score,
                COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
            FROM solution_submissions s
            LEFT JOIN LATERAL (
                SELECT rank_score, aggregate_metrics_json, validation_summary_json, public_results_json
                FROM evaluations
                WHERE solution_submission_id = s.id AND eval_type = 'validation' AND status = 'completed' AND target = s.target
                ORDER BY created_at DESC LIMIT 1
            ) ve ON TRUE
            LEFT JOIN LATERAL (
                SELECT primary_score, rank_score, aggregate_metrics_json, official_summary_json, public_results_json
                FROM evaluations
                WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed' AND target = s.target
                ORDER BY created_at DESC LIMIT 1
            ) oe ON TRUE
            WHERE s.challenge_name = $1 AND s.agent_id = $2::uuid AND s.id <> $3::uuid
              AND s.target = $4
              AND s.visible_after_eval = TRUE AND s.status = 'completed'
              AND COALESCE(
                    oe.rank_score,
                    ve.rank_score,
                    (oe.official_summary_json->>'score')::double precision,
                    (ve.validation_summary_json->>'score')::double precision
                  ) IS NOT NULL
            "#
        )
        .bind(&challenge_name)
        .bind(&agent_id)
        .bind(solution_submission_id.as_str())
        .bind(&target)
        .fetch_all(&mut **tx)
        .await?;

        let mut candidates = replacement_rows
            .into_iter()
            .map(|row| {
                let aggregate_metrics = decode_optional_json(
                    Some(row.try_get::<Value, _>("aggregate_metrics")?),
                    "leaderboard replacement aggregate metrics",
                )?
                .unwrap_or_default();
                Ok(LeaderboardReplacementCandidate {
                    id: row.try_get("id")?,
                    created_at: row.try_get("created_at")?,
                    rank_score: row.try_get("ranking_score")?,
                    public_results_json: row.try_get("public_results")?,
                    aggregate_metrics,
                    aggregate_metrics_json: row.try_get("aggregate_metrics")?,
                    official_score: row.try_get("official_score")?,
                    official_metrics_json: row.try_get("official_metrics")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        candidates.sort_by(|a, b| {
            compare_rank_payloads(
                &spec,
                a.rank_score,
                &a.aggregate_metrics,
                b.rank_score,
                &b.aggregate_metrics,
            )
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
        });

        if let Some(best) = candidates.into_iter().next() {
            sqlx::query(
                r#"
                INSERT INTO leaderboard_entries (
                    challenge_name, target, agent_id, best_solution_submission_id, best_rank_score,
                    public_results_json, aggregate_metrics_json, official_score,
                    official_metrics_json, updated_at
                )
                VALUES ($1, $2, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, NOW())
                ON CONFLICT (challenge_name, target, agent_id) DO UPDATE
                SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
                    best_rank_score = EXCLUDED.best_rank_score,
                    public_results_json = EXCLUDED.public_results_json,
                    aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
                    official_score = EXCLUDED.official_score,
                    official_metrics_json = EXCLUDED.official_metrics_json,
                    updated_at = NOW()
                "#,
            )
            .bind(&challenge_name)
            .bind(&target)
            .bind(&agent_id)
            .bind(&best.id)
            .bind(best.rank_score)
            .bind(&best.public_results_json)
            .bind(&best.aggregate_metrics_json)
            .bind(best.official_score)
            .bind(&best.official_metrics_json)
            .execute(&mut **tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid",
            )
            .bind(&challenge_name)
            .bind(&target)
            .bind(&agent_id)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

/// List leaderboard entries for a challenge name.
pub async fn list_leaderboard_entries(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<LeaderboardEntryDto>> {
    list_leaderboard_entries_inner(pool, challenge_name, target, limit, true).await
}

/// List leaderboard entries with metric payloads for internal aggregate calculations.
pub async fn list_leaderboard_entries_with_metric_payloads(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<LeaderboardEntryDto>> {
    list_leaderboard_entries_inner(pool, challenge_name, target, limit, false).await
}

/// Candidate row used when repairing one agent's leaderboard entry after hiding a submission.
#[derive(Debug)]
struct LeaderboardReplacementCandidate {
    id: String,
    created_at: DateTime<Utc>,
    rank_score: f64,
    public_results_json: Value,
    aggregate_metrics: Vec<MetricValue>,
    aggregate_metrics_json: Value,
    official_score: Option<f64>,
    official_metrics_json: Value,
}

/// Query, order, and optionally redact the visible leaderboard rows for one target.
async fn list_leaderboard_entries_inner(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
    redact_metric_payloads: bool,
) -> Result<Vec<LeaderboardEntryDto>> {
    let requested_limit = limit.max(1);
    let challenge = get_published_challenge(pool, challenge_name)
        .await?
        .ok_or(AppError::NotFound)?;
    let spec = serde_json::from_value::<ChallengeBundleSpec>(challenge.spec_json)
        .map_err(|e| AppError::Internal(format!("stored challenge spec is invalid: {e}")))?;
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            le.target, le.agent_id, a.display_name AS agent_display_name, le.best_solution_submission_id,
            le.best_rank_score, le.aggregate_metrics_json, le.official_score,
            le.official_metrics_json, le.updated_at
        FROM leaderboard_entries le
        JOIN solution_submissions s ON s.id = le.best_solution_submission_id
        JOIN agents a ON a.id = le.agent_id
        JOIN challenges p ON p.name = le.challenge_name
        WHERE p.name =
        "#,
    );
    query
        .push_bind(challenge_name.as_str())
        .push(
            r#"
          AND le.target =
        "#,
        )
        .push_bind(target.as_str())
        .push(
            r#"
          AND s.visible_after_eval = TRUE
          AND s.status = 'completed'
        ORDER BY le.best_rank_score DESC
        "#,
        );
    for metric_name in &spec.metric_schema.ranking.tie_breaker_metric_names {
        let Some(definition) = spec.metric_schema.metric(metric_name) else {
            continue;
        };
        query.push(
            ", (SELECT (metric->>'value')::double precision \
             FROM jsonb_array_elements(COALESCE(le.aggregate_metrics_json, '[]'::jsonb)) AS metric \
             WHERE metric->>'metric_name' = ",
        );
        query.push_bind(metric_name.as_str());
        query.push(" LIMIT 1) ");
        match definition.direction {
            MetricDirection::Maximize => query.push("DESC NULLS LAST"),
            MetricDirection::Minimize => query.push("ASC NULLS LAST"),
        };
    }
    query
        .push(", le.updated_at ASC, a.display_name ASC, le.best_solution_submission_id ASC LIMIT ")
        .push_bind(requested_limit);

    let rows = query.build().fetch_all(pool).await?;

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
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
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

    if redact_metric_payloads {
        for entry in &mut entries {
            entry.aggregate_metrics.clear();
            entry.official_metrics.clear();
        }
    }

    Ok(entries)
}

/// Handles upsert leaderboard entry for solution submission tx for this module.
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
        SELECT s.challenge_name, s.agent_id::text AS agent_id, p.spec_json
        FROM solution_submissions s
        JOIN challenges p ON p.name = s.challenge_name
        WHERE s.id = $1::uuid
        LIMIT 1
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_name, agent_id, spec_json)) = row else {
        return Ok(false);
    };
    let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .map_err(|e| AppError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    let current: Option<(f64, Value)> = sqlx::query_as(
        "SELECT best_rank_score, aggregate_metrics_json FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(&challenge_name)
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

    if !candidate_replaces_leaderboard_entry(&spec, current, rank_score, aggregate_metrics) {
        return Ok(false);
    }

    let public_results_json =
        serde_json::to_value(public_results).map_err(|e| AppError::Internal(e.to_string()))?;
    let aggregate_metrics_json =
        serde_json::to_value(aggregate_metrics).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_name, target, agent_id, best_solution_submission_id, best_rank_score,
            public_results_json, aggregate_metrics_json, updated_at
        )
        VALUES ($1, $2, $3::uuid, $4::uuid, $5, $6, $7, NOW())
        ON CONFLICT (challenge_name, target, agent_id) DO UPDATE
        SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
            best_rank_score = EXCLUDED.best_rank_score,
            public_results_json = EXCLUDED.public_results_json,
            aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
            updated_at = NOW()
        "#,
    )
    .bind(&challenge_name)
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

/// Handles update official score for solution submission tx for this module.
pub(super) async fn update_official_score_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    target: &TargetName,
    official_score: Option<f64>,
    official_metrics: &[MetricValue],
) -> Result<()> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT challenge_name, agent_id::text AS agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_name, agent_id)) = row else {
        return Ok(());
    };

    let official_metrics_json =
        serde_json::to_value(official_metrics).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        "UPDATE leaderboard_entries SET official_score = $4, official_metrics_json = $5, updated_at = NOW() WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid"
    )
    .bind(&challenge_name)
    .bind(target.as_str())
    .bind(&agent_id)
    .bind(official_score)
    .bind(&official_metrics_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Handles compare rank payloads for this module.
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

    for metric_name in &spec.metric_schema.ranking.tie_breaker_metric_names {
        let Some(definition) = spec.metric_schema.metric(metric_name) else {
            continue;
        };
        let ordering = compare_metric_by_direction(
            definition.direction,
            metric_value(a_metrics, metric_name),
            metric_value(b_metrics, metric_name),
        );
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    Ordering::Equal
}

/// Handles candidate replaces leaderboard entry for this module.
fn candidate_replaces_leaderboard_entry(
    spec: &ChallengeBundleSpec,
    current: Option<(f64, Vec<MetricValue>)>,
    candidate_score: f64,
    candidate_metrics: &[MetricValue],
) -> bool {
    let Some((current_score, current_metrics)) = current else {
        return true;
    };

    compare_rank_payloads(
        spec,
        candidate_score,
        candidate_metrics,
        current_score,
        &current_metrics,
    ) == Ordering::Less
}

/// Handles metric value for this module.
fn metric_value(metrics: &[MetricValue], metric_name: &MetricName) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| &metric.metric_name == metric_name)
        .map(|metric| metric.value)
}

/// Handles compare metric by direction for this module.
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

/// Handles compare f64 desc for this module.
fn compare_f64_desc(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Handles compare f64 asc for this module.
fn compare_f64_asc(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}
