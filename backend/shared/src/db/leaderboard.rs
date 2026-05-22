use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};

use crate::error::{Result, ServiceError};
use crate::models::challenge::{ChallengeBundleSpec, MetricDirection};
use crate::models::evaluation::{MetricValue, PublicCaseResult};
use crate::models::ids::{ChallengeId, SolutionSubmissionId};
use crate::models::names::{MetricName, TargetName};
use crate::models::request::LeaderboardEntryDto;

use super::challenges::get_public_challenge;
use super::ids::{agent_id_from_row, solution_submission_id_from_row, target_from_row};
use super::json::decode_optional_json;

/// Repair or remove the leaderboard row touched by one submission visibility change.
pub(super) async fn repair_leaderboard_entry_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT challenge_id::text AS challenge_id, target, agent_id::text AS agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1"
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_id, target, agent_id)) = row else {
        return Ok(());
    };

    lock_leaderboard_scope(tx, &challenge_id, &target, &agent_id).await?;

    let leaderboard_entry: Option<(String,)> = sqlx::query_as(
        "SELECT best_solution_submission_id::text AS best_solution_submission_id FROM leaderboard_entries WHERE challenge_id = $1::uuid AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(&challenge_id)
    .bind(&target)
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await?;

    if leaderboard_entry
        .map(|e| e.0 == solution_submission_id.as_str())
        .unwrap_or(false)
    {
        let spec_json: Option<(Value,)> = sqlx::query_as(
            "SELECT spec_json FROM challenges WHERE challenge_id = $1::uuid LIMIT 1",
        )
        .bind(&challenge_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((spec_json,)) = spec_json else {
            return Ok(());
        };
        let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json).map_err(|e| {
            ServiceError::Internal(format!("stored challenge spec is invalid: {e}"))
        })?;

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
            WHERE s.challenge_id = $1::uuid AND s.agent_id = $2::uuid AND s.id <> $3::uuid
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
        .bind(&challenge_id)
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
                    challenge_id, target, agent_id, best_solution_submission_id, best_rank_score,
                    public_results_json, aggregate_metrics_json, official_metrics_json, updated_at
                )
                VALUES ($1::uuid, $2, $3::uuid, $4::uuid, $5, $6, $7, $8, NOW())
                ON CONFLICT (challenge_id, target, agent_id) DO UPDATE
                SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
                    best_rank_score = EXCLUDED.best_rank_score,
                    public_results_json = EXCLUDED.public_results_json,
                    aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
                    official_metrics_json = EXCLUDED.official_metrics_json,
                    updated_at = NOW()
                "#,
            )
            .bind(&challenge_id)
            .bind(&target)
            .bind(&agent_id)
            .bind(&best.id)
            .bind(best.rank_score)
            .bind(&best.public_results_json)
            .bind(&best.aggregate_metrics_json)
            .bind(&best.official_metrics_json)
            .execute(&mut **tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM leaderboard_entries WHERE challenge_id = $1::uuid AND target = $2 AND agent_id = $3::uuid",
            )
            .bind(&challenge_id)
            .bind(&target)
            .bind(&agent_id)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

/// List leaderboard entries for a challenge id.
pub async fn list_leaderboard_entries(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<LeaderboardEntryDto>> {
    let (spec, rows) = list_leaderboard_rows(pool, challenge_id, target, limit).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.into_public_dto(&spec))
        .collect())
}

/// List leaderboard entries with metric payloads for internal aggregate calculations.
pub async fn list_leaderboard_entries_with_metric_payloads(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<LeaderboardMetricEntry>> {
    let (_spec, rows) = list_leaderboard_rows(pool, challenge_id, target, limit).await?;
    Ok(rows
        .into_iter()
        .map(LeaderboardRow::into_metric_entry)
        .collect())
}

/// Internal leaderboard row that keeps metric payloads out of public DTOs.
#[derive(Debug, Clone)]
pub struct LeaderboardMetricEntry {
    pub best_rank_score: f64,
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
}

/// Candidate row used when repairing one agent's leaderboard entry after visibility changes.
#[derive(Debug)]
struct LeaderboardReplacementCandidate {
    id: String,
    created_at: DateTime<Utc>,
    rank_score: f64,
    public_results_json: Value,
    aggregate_metrics: Vec<MetricValue>,
    aggregate_metrics_json: Value,
    official_metrics_json: Value,
}

/// Database row for one visible leaderboard entry before public projection.
struct LeaderboardRow {
    target: TargetName,
    agent_id: crate::models::ids::AgentId,
    agent_display_name: String,
    best_solution_submission_id: SolutionSubmissionId,
    best_rank_score: f64,
    aggregate_metrics: Vec<MetricValue>,
    official_metrics: Vec<MetricValue>,
    updated_at: DateTime<Utc>,
}

impl LeaderboardRow {
    /// Project this row into the public leaderboard DTO.
    fn into_public_dto(self, spec: &ChallengeBundleSpec) -> LeaderboardEntryDto {
        let official_primary_metric = MetricValue::find_by_name(
            &self.official_metrics,
            &spec.metric_schema.ranking.primary_metric_name,
        );
        LeaderboardEntryDto {
            target: self.target,
            agent_id: self.agent_id,
            agent_display_name: self.agent_display_name,
            best_solution_submission_id: self.best_solution_submission_id,
            best_rank_score: self.best_rank_score,
            rank_score: self.best_rank_score,
            official_primary_metric,
            updated_at: self.updated_at.to_rfc3339(),
        }
    }

    /// Project this row into the backend-only metric payload shape.
    fn into_metric_entry(self) -> LeaderboardMetricEntry {
        LeaderboardMetricEntry {
            best_rank_score: self.best_rank_score,
            aggregate_metrics: self.aggregate_metrics,
            official_metrics: self.official_metrics,
        }
    }
}

/// Query and order the visible leaderboard rows for one target.
async fn list_leaderboard_rows(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    target: &TargetName,
    limit: i64,
) -> Result<(ChallengeBundleSpec, Vec<LeaderboardRow>)> {
    let requested_limit = limit.max(1);
    let challenge = get_public_challenge(pool, challenge_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let spec = serde_json::from_value::<ChallengeBundleSpec>(challenge.spec_json)
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            le.target, le.agent_id, a.display_name AS agent_display_name, le.best_solution_submission_id,
            le.best_rank_score, le.aggregate_metrics_json, le.official_metrics_json,
            le.updated_at
        FROM leaderboard_entries le
        JOIN solution_submissions s ON s.id = le.best_solution_submission_id
        JOIN agents a ON a.id = le.agent_id
        JOIN challenges p ON p.challenge_id = le.challenge_id
        WHERE p.challenge_id =
        "#,
    );
    query
        .push_bind(challenge_id.as_str())
        .push("::uuid")
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

    let entries = rows
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

            Ok(LeaderboardRow {
                target: target_from_row(&r, "target")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                best_solution_submission_id: solution_submission_id_from_row(
                    &r,
                    "best_solution_submission_id",
                )?,
                best_rank_score,
                aggregate_metrics,
                official_metrics,
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok((spec, entries))
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
        SELECT s.challenge_id::text AS challenge_id, s.agent_id::text AS agent_id, p.spec_json
        FROM solution_submissions s
        JOIN challenges p ON p.challenge_id = s.challenge_id
        WHERE s.id = $1::uuid
        LIMIT 1
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_id, agent_id, spec_json)) = row else {
        return Ok(false);
    };
    let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    lock_leaderboard_scope(tx, &challenge_id, target.as_str(), &agent_id).await?;

    let current: Option<(f64, Value)> = sqlx::query_as(
        "SELECT best_rank_score, aggregate_metrics_json FROM leaderboard_entries WHERE challenge_id = $1::uuid AND target = $2 AND agent_id = $3::uuid LIMIT 1"
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

    if !candidate_replaces_leaderboard_entry(&spec, current, rank_score, aggregate_metrics) {
        return Ok(false);
    }

    let public_results_json =
        serde_json::to_value(public_results).map_err(|e| ServiceError::Internal(e.to_string()))?;
    let aggregate_metrics_json = serde_json::to_value(aggregate_metrics)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_id, target, agent_id, best_solution_submission_id, best_rank_score,
            public_results_json, aggregate_metrics_json, updated_at
        )
        VALUES ($1::uuid, $2, $3::uuid, $4::uuid, $5, $6, $7, NOW())
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

/// Update official aggregate metrics for a leaderboard entry after it becomes best.
pub(super) async fn update_official_metrics_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    target: &TargetName,
    official_metrics: &[MetricValue],
) -> Result<()> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT challenge_id::text AS challenge_id, agent_id::text AS agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some((challenge_id, agent_id)) = row else {
        return Ok(());
    };

    let official_metrics_json = serde_json::to_value(official_metrics)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

    sqlx::query(
        "UPDATE leaderboard_entries SET official_metrics_json = $4, updated_at = NOW() WHERE challenge_id = $1::uuid AND target = $2 AND agent_id = $3::uuid"
    )
    .bind(&challenge_id)
    .bind(target.as_str())
    .bind(&agent_id)
    .bind(&official_metrics_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Serialize leaderboard changes for one challenge, target, and agent.
async fn lock_leaderboard_scope(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &str,
    target: &str,
    agent_id: &str,
) -> Result<()> {
    let scope = format!("leaderboard:{challenge_id}:{target}:{agent_id}");
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ($1)
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(&scope)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        SELECT scope
        FROM quota_admission_locks
        WHERE scope = $1
        FOR UPDATE
        "#,
    )
    .bind(&scope)
    .fetch_one(&mut **tx)
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
