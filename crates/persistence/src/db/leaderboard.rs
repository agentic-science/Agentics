use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};

use agentics_domain::models::challenge::{ChallengeBundleSpec, MetricDirection};
use agentics_domain::models::evaluation::{MetricValue, PublicCaseResult};
use agentics_domain::models::ids::{AgentId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_error::{Result, ServiceError};

use super::ids::{
    agent_id_from_row, challenge_name_from_row, solution_submission_id_from_row, target_from_row,
};
use super::json::decode_optional_json;

/// Repair or remove the leaderboard row touched by one submission visibility change.
pub(super) async fn repair_leaderboard_entry_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let Some(scope) = find_leaderboard_repair_scope(tx, solution_submission_id).await? else {
        return Ok(());
    };

    lock_leaderboard_scope(tx, &scope.challenge_name, &scope.target, &scope.agent_id).await?;
    if !leaderboard_entry_points_to_submission(tx, &scope, solution_submission_id).await? {
        return Ok(());
    }

    let Some(spec) = load_repair_challenge_spec(tx, &scope.challenge_name).await? else {
        return Ok(());
    };
    match best_replacement_candidate(tx, &scope, solution_submission_id, &spec).await? {
        Some(best) => upsert_replacement_leaderboard_entry(tx, &scope, &best).await?,
        None => delete_leaderboard_entry(tx, &scope).await?,
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct LeaderboardRepairScope {
    challenge_name: ChallengeName,
    target: TargetName,
    agent_id: AgentId,
}

async fn find_leaderboard_repair_scope<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<Option<LeaderboardRepairScope>> {
    let row = sqlx::query(
        "SELECT challenge_name, target, agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1"
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        Ok(LeaderboardRepairScope {
            challenge_name: challenge_name_from_row(&row, "challenge_name")?,
            target: target_from_row(&row, "target")?,
            agent_id: agent_id_from_row(&row, "agent_id")?,
        })
    })
    .transpose()
}

async fn leaderboard_entry_points_to_submission<'a>(
    tx: &mut Transaction<'a, Postgres>,
    scope: &LeaderboardRepairScope,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<bool> {
    let entry: Option<(String,)> = sqlx::query_as(
        "SELECT best_solution_submission_id::text AS best_solution_submission_id FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(scope.challenge_name.as_str())
    .bind(scope.target.as_str())
    .bind(scope.agent_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(entry
        .map(|entry| entry.0 == solution_submission_id.as_str())
        .unwrap_or(false))
}

async fn load_repair_challenge_spec<'a>(
    tx: &mut Transaction<'a, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<Option<ChallengeBundleSpec>> {
    let spec_json: Option<(Value,)> =
        sqlx::query_as("SELECT spec_json FROM challenges WHERE challenge_name = $1 LIMIT 1")
            .bind(challenge_name.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    spec_json
        .map(|(value,)| {
            serde_json::from_value::<ChallengeBundleSpec>(value).map_err(|error| {
                ServiceError::Internal(format!("stored challenge spec is invalid: {error}"))
            })
        })
        .transpose()
}

async fn best_replacement_candidate<'a>(
    tx: &mut Transaction<'a, Postgres>,
    scope: &LeaderboardRepairScope,
    solution_submission_id: &SolutionSubmissionId,
    spec: &ChallengeBundleSpec,
) -> Result<Option<LeaderboardReplacementCandidate>> {
    let rows = replacement_candidate_rows(tx, scope, solution_submission_id).await?;
    let mut candidates = rows
        .into_iter()
        .map(leaderboard_replacement_candidate_from_row)
        .collect::<Result<Vec<_>>>()?;
    candidates.sort_by(|a, b| compare_replacement_candidates(spec, a, b));
    Ok(candidates.into_iter().next())
}

async fn replacement_candidate_rows<'a>(
    tx: &mut Transaction<'a, Postgres>,
    scope: &LeaderboardRepairScope,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<Vec<sqlx::postgres::PgRow>> {
    Ok(sqlx::query(
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
        WHERE s.challenge_name = $1 AND s.agent_id = $2::uuid AND s.id <> $3::uuid
          AND s.target = $4
          AND s.visible_after_eval = TRUE AND s.status = 'completed'
          AND COALESCE(
                oe.rank_score,
                ve.rank_score,
                (oe.official_summary_json->>'score')::double precision,
                (ve.validation_summary_json->>'score')::double precision
              ) IS NOT NULL
        "#,
    )
    .bind(scope.challenge_name.as_str())
    .bind(scope.agent_id.as_str())
    .bind(solution_submission_id.as_str())
    .bind(scope.target.as_str())
    .fetch_all(&mut **tx)
    .await?)
}

fn leaderboard_replacement_candidate_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<LeaderboardReplacementCandidate> {
    let aggregate_metrics = decode_optional_json(
        Some(row.try_get::<Value, _>("aggregate_metrics")?),
        "leaderboard replacement aggregate metrics",
    )?
    .unwrap_or_default();
    Ok(LeaderboardReplacementCandidate {
        id: solution_submission_id_from_row(&row, "id")?,
        created_at: row.try_get("created_at")?,
        rank_score: row.try_get("ranking_score")?,
        public_results_json: row.try_get("public_results")?,
        aggregate_metrics,
        aggregate_metrics_json: row.try_get("aggregate_metrics")?,
        official_metrics_json: row.try_get("official_metrics")?,
    })
}

fn compare_replacement_candidates(
    spec: &ChallengeBundleSpec,
    a: &LeaderboardReplacementCandidate,
    b: &LeaderboardReplacementCandidate,
) -> Ordering {
    compare_rank_payloads(
        spec,
        a.rank_score,
        &a.aggregate_metrics,
        b.rank_score,
        &b.aggregate_metrics,
    )
    .then_with(|| a.created_at.cmp(&b.created_at))
    .then_with(|| a.id.cmp(&b.id))
}

async fn upsert_replacement_leaderboard_entry<'a>(
    tx: &mut Transaction<'a, Postgres>,
    scope: &LeaderboardRepairScope,
    best: &LeaderboardReplacementCandidate,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_name, target, agent_id, best_solution_submission_id, best_rank_score,
            public_results_json, aggregate_metrics_json, official_metrics_json, updated_at
        )
        VALUES ($1, $2, $3::uuid, $4::uuid, $5, $6, $7, $8, NOW())
        ON CONFLICT (challenge_name, target, agent_id) DO UPDATE
        SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
            best_rank_score = EXCLUDED.best_rank_score,
            public_results_json = EXCLUDED.public_results_json,
            aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
            official_metrics_json = EXCLUDED.official_metrics_json,
            updated_at = NOW()
        "#,
    )
    .bind(scope.challenge_name.as_str())
    .bind(scope.target.as_str())
    .bind(scope.agent_id.as_str())
    .bind(best.id.as_str())
    .bind(best.rank_score)
    .bind(&best.public_results_json)
    .bind(&best.aggregate_metrics_json)
    .bind(&best.official_metrics_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn delete_leaderboard_entry<'a>(
    tx: &mut Transaction<'a, Postgres>,
    scope: &LeaderboardRepairScope,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid",
    )
    .bind(scope.challenge_name.as_str())
    .bind(scope.target.as_str())
    .bind(scope.agent_id.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// List leaderboard entries for a challenge name.
pub async fn list_leaderboard_entries(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
    spec: &ChallengeBundleSpec,
) -> Result<Vec<LeaderboardRecord>> {
    list_leaderboard_rows(pool, challenge_name, target, limit, spec).await
}

/// List leaderboard entries with metric payloads for internal aggregate calculations.
pub async fn list_leaderboard_entries_with_metric_payloads(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
    spec: &ChallengeBundleSpec,
) -> Result<Vec<LeaderboardMetricEntry>> {
    let rows = list_leaderboard_rows(pool, challenge_name, target, limit, spec).await?;
    Ok(rows
        .into_iter()
        .map(LeaderboardRecord::into_metric_entry)
        .collect())
}

/// Database record for one visible leaderboard entry before public projection.
#[derive(Debug, Clone)]
pub struct LeaderboardRecord {
    pub target: TargetName,
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub best_solution_submission_id: SolutionSubmissionId,
    pub best_rank_score: f64,
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
    pub updated_at: DateTime<Utc>,
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
    id: SolutionSubmissionId,
    created_at: DateTime<Utc>,
    rank_score: f64,
    public_results_json: Value,
    aggregate_metrics: Vec<MetricValue>,
    aggregate_metrics_json: Value,
    official_metrics_json: Value,
}

impl LeaderboardRecord {
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
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
    spec: &ChallengeBundleSpec,
) -> Result<Vec<LeaderboardRecord>> {
    let requested_limit = limit.max(1);
    let mut query = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            le.target, le.agent_id, a.display_name AS agent_display_name, le.best_solution_submission_id,
            le.best_rank_score, le.aggregate_metrics_json, le.official_metrics_json,
            le.updated_at
        FROM leaderboard_entries le
        JOIN solution_submissions s ON s.id = le.best_solution_submission_id
        JOIN agents a ON a.id = le.agent_id
        JOIN challenges p ON p.challenge_name = le.challenge_name
        WHERE p.challenge_name =
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

            Ok(LeaderboardRecord {
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
    let row = sqlx::query(
        r#"
        SELECT s.challenge_name, s.agent_id, p.spec_json
        FROM solution_submissions s
        JOIN challenges p ON p.challenge_name = s.challenge_name
        WHERE s.id = $1::uuid
        LIMIT 1
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };
    let challenge_name = challenge_name_from_row(&row, "challenge_name")?;
    let agent_id = agent_id_from_row(&row, "agent_id")?;
    let spec_json: Value = row.try_get("spec_json")?;
    let spec = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    lock_leaderboard_scope(tx, &challenge_name, target, &agent_id).await?;

    let current: Option<(f64, Value)> = sqlx::query_as(
        "SELECT best_rank_score, aggregate_metrics_json FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(agent_id.as_str())
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
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(agent_id.as_str())
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
    let row = sqlx::query(
        "SELECT challenge_name, agent_id FROM solution_submissions WHERE id = $1::uuid LIMIT 1",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Ok(());
    };
    let challenge_name = challenge_name_from_row(&row, "challenge_name")?;
    let agent_id = agent_id_from_row(&row, "agent_id")?;

    let official_metrics_json = serde_json::to_value(official_metrics)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

    sqlx::query(
        "UPDATE leaderboard_entries SET official_metrics_json = $4, updated_at = NOW() WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid"
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(agent_id.as_str())
    .bind(&official_metrics_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Serialize leaderboard changes for one challenge, target, and agent.
async fn lock_leaderboard_scope(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    target: &TargetName,
    agent_id: &AgentId,
) -> Result<()> {
    let scope = format!(
        "leaderboard:{}:{}:{}",
        challenge_name.as_str(),
        target.as_str(),
        agent_id.as_str()
    );
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
