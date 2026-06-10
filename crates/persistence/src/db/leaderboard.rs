use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::evaluation::{
    MetricValue, PublicCaseResult, compare_metric_payloads_by_ranking,
};
use agentics_domain::models::ids::{AgentId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, TargetName};
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
            COALESCE(oe.public_results_json, '[]'::jsonb) AS public_results,
            COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS aggregate_metrics,
            COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
        FROM solution_submissions s
        JOIN LATERAL (
            SELECT aggregate_metrics_json, official_summary_json, public_results_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed' AND target = s.target
            ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE s.challenge_name = $1 AND s.agent_id = $2::uuid AND s.id <> $3::uuid
          AND s.target = $4
          AND s.visible_after_eval = TRUE AND s.status = 'completed'
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
    compare_rank_payloads(spec, &a.aggregate_metrics, &b.aggregate_metrics)
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
            challenge_name, target, agent_id, best_solution_submission_id,
            public_results_json, aggregate_metrics_json, official_metrics_json, updated_at
        )
        VALUES ($1, $2, $3::uuid, $4::uuid, $5, $6, $7, NOW())
        ON CONFLICT (challenge_name, target, agent_id) DO UPDATE
        SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
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
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
    pub updated_at: DateTime<Utc>,
}

/// Internal leaderboard row that keeps metric payloads out of public DTOs.
#[derive(Debug, Clone)]
pub struct LeaderboardMetricEntry {
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
}

/// Candidate row used when repairing one agent's leaderboard entry after visibility changes.
#[derive(Debug)]
struct LeaderboardReplacementCandidate {
    id: SolutionSubmissionId,
    created_at: DateTime<Utc>,
    public_results_json: Value,
    aggregate_metrics: Vec<MetricValue>,
    aggregate_metrics_json: Value,
    official_metrics_json: Value,
}

impl LeaderboardRecord {
    /// Project this row into the backend-only metric payload shape.
    fn into_metric_entry(self) -> LeaderboardMetricEntry {
        LeaderboardMetricEntry {
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
    let rows = sqlx::query(
        r#"
        SELECT
            le.target, le.agent_id, a.display_name AS agent_display_name, le.best_solution_submission_id,
            le.aggregate_metrics_json, le.official_metrics_json,
            le.updated_at
        FROM leaderboard_entries le
        JOIN solution_submissions s ON s.id = le.best_solution_submission_id
        JOIN agents a ON a.id = le.agent_id
        JOIN challenges p ON p.challenge_name = le.challenge_name
        WHERE p.challenge_name = $1
          AND le.target = $2
          AND s.visible_after_eval = TRUE
          AND s.status = 'completed'
        "#
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
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

            Ok(LeaderboardRecord {
                target: target_from_row(&r, "target")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                best_solution_submission_id: solution_submission_id_from_row(
                    &r,
                    "best_solution_submission_id",
                )?,
                aggregate_metrics,
                official_metrics,
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    entries.sort_by(|a, b| {
        compare_metric_payloads_by_ranking(
            &spec.metric_schema,
            &a.aggregate_metrics,
            &b.aggregate_metrics,
        )
        .then_with(|| a.updated_at.cmp(&b.updated_at))
        .then_with(|| a.agent_display_name.cmp(&b.agent_display_name))
        .then_with(|| {
            a.best_solution_submission_id
                .cmp(&b.best_solution_submission_id)
        })
    });
    let requested_limit = usize::try_from(requested_limit).map_err(|_| {
        ServiceError::Validation("leaderboard limit exceeds supported range".to_string())
    })?;
    entries.truncate(requested_limit);

    Ok(entries)
}

/// Handles upsert leaderboard entry for solution submission tx for this module.
pub(super) async fn upsert_leaderboard_entry_for_solution_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    target: &TargetName,
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

    let current: Option<(Value,)> = sqlx::query_as(
        "SELECT aggregate_metrics_json FROM leaderboard_entries WHERE challenge_name = $1 AND target = $2 AND agent_id = $3::uuid LIMIT 1"
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(agent_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let current: Option<Vec<MetricValue>> = current
        .map(|(metrics_json,)| {
            decode_optional_json(Some(metrics_json), "leaderboard aggregate metrics")
                .map(|metrics| metrics.unwrap_or_default())
        })
        .transpose()?;

    if !candidate_replaces_leaderboard_entry(&spec, current, aggregate_metrics) {
        return Ok(false);
    }

    let public_results_json =
        serde_json::to_value(public_results).map_err(|e| ServiceError::Internal(e.to_string()))?;
    let aggregate_metrics_json = serde_json::to_value(aggregate_metrics)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_name, target, agent_id, best_solution_submission_id,
            public_results_json, aggregate_metrics_json, official_metrics_json, updated_at
        )
        VALUES ($1, $2, $3::uuid, $4::uuid, $5, $6, $7, NOW())
        ON CONFLICT (challenge_name, target, agent_id) DO UPDATE
        SET best_solution_submission_id = EXCLUDED.best_solution_submission_id,
            public_results_json = EXCLUDED.public_results_json,
            aggregate_metrics_json = EXCLUDED.aggregate_metrics_json,
            official_metrics_json = EXCLUDED.official_metrics_json,
            updated_at = NOW()
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(agent_id.as_str())
    .bind(solution_submission_id.as_str())
    .bind(&public_results_json)
    .bind(&aggregate_metrics_json)
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
    a_metrics: &[MetricValue],
    b_metrics: &[MetricValue],
) -> Ordering {
    compare_metric_payloads_by_ranking(&spec.metric_schema, a_metrics, b_metrics)
}

/// Handles candidate replaces leaderboard entry for this module.
fn candidate_replaces_leaderboard_entry(
    spec: &ChallengeBundleSpec,
    current: Option<Vec<MetricValue>>,
    candidate_metrics: &[MetricValue],
) -> bool {
    let Some(current_metrics) = current else {
        return true;
    };

    compare_rank_payloads(spec, candidate_metrics, &current_metrics) == Ordering::Less
}
