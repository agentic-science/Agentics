use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::challenge::{ChallengeBundleSpec, TargetAccelerator};
use agentics_domain::models::evaluation::{
    EvaluationDto, EvaluationJobPayload, EvaluationJobStatus, EvaluationStatus, MetricValue,
    ScoringMode, SolutionArtifactMetadata, SolutionSubmissionStatus,
};
use agentics_domain::models::ids::{AgentId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};

use super::evaluation_policy::{
    ensure_challenge_supports_eval_type_tx, ensure_validation_uses_public_bundle,
    lock_active_challenge_for_admission_tx,
};
use super::ids::{agent_id_from_row, challenge_name_from_row, solution_submission_id_from_row};
use super::json::decode_optional_json;

mod admission;
mod rows;
use admission::enforce_quota_admission;
use rows::{
    artifact_metadata_from_row, count_to_u64, optional_evaluation_job_id_from_row,
    optional_evaluation_job_status_from_row, optional_evaluation_status_from_row,
    optional_scoring_mode_from_row, optional_solution_submission_id_from_row, parse_eval_from_row,
    solution_submission_status_from_row, storage_key_from_row, target_from_row, u64_to_i64,
};

/// Input for creating a solution submission and its initial evaluation job.
#[derive(Debug, Clone)]
pub struct CreateSolutionSubmissionInput {
    pub solution_submission_id: SolutionSubmissionId,
    pub job_id: EvaluationJobId,
    pub agent_id: AgentId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub artifact_key: StorageKey,
    pub artifact_metadata: SolutionArtifactMetadata,
    pub note: String,
    pub eval_type: ScoringMode,
    pub explanation: String,
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    pub quota_admission: SolutionSubmissionQuotaAdmission,
}

/// Admin solution-submission list row before DTO projection.
#[derive(Debug, Clone)]
pub struct AdminSolutionSubmissionListItemRecord {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    pub target: TargetName,
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub status: SolutionSubmissionStatus,
    pub note: String,
    pub visible_after_eval: bool,
    pub latest_job_id: Option<EvaluationJobId>,
    pub latest_job_status: Option<EvaluationJobStatus>,
    pub latest_job_eval_type: Option<ScoringMode>,
    pub validation_status: Option<EvaluationStatus>,
    pub official_status: Option<EvaluationStatus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public solution-submission list row before DTO projection/redaction.
#[derive(Debug, Clone)]
pub struct PublicSolutionSubmissionListItemRecord {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub challenge_title: String,
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub status: SolutionSubmissionStatus,
    pub note: String,
    pub explanation: String,
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    pub official_metrics: Vec<MetricValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Aggregate public observer counters before transport projection.
#[derive(Debug, Clone, Copy)]
pub struct PublicObserverStatsRecord {
    pub challenge_count: u64,
    pub agent_count: u64,
    pub public_completed_submission_count: u64,
    pub total_solution_attempt_count: u64,
}

/// Authoritative quota limits applied inside the submission/job transaction.
#[derive(Debug, Clone, Copy)]
pub struct SolutionSubmissionQuotaAdmission {
    pub window_seconds: i64,
    pub per_agent_challenge_limit: i64,
    pub challenge_lifetime_limit: Option<i64>,
    pub max_active_official_jobs: Option<i64>,
}

/// Solution submission row with optional joined evaluation and job metadata.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionRecord {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub agent_id: AgentId,
    pub agent_display_name: Option<String>,
    pub challenge_title: Option<String>,
    pub challenge_spec: ChallengeBundleSpec,
    pub artifact_key: StorageKey,
    pub artifact_metadata: Option<SolutionArtifactMetadata>,
    pub note: String,
    pub status: String,
    pub explanation: String,
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    pub visible_after_eval: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub evaluation_job_id: Option<EvaluationJobId>,
    pub evaluation_job_status: Option<String>,
    pub evaluation: Option<EvaluationDto>,
    pub validation_evaluation: Option<EvaluationDto>,
    pub official_evaluation: Option<EvaluationDto>,
}

/// Create a solution submission and queue its first evaluation atomically.
pub async fn create_solution_submission_with_job(
    pool: &PgPool,
    input: &CreateSolutionSubmissionInput,
) -> Result<SolutionSubmissionRecord> {
    let mut tx = pool.begin().await?;
    let challenge = lock_active_challenge_for_admission_tx(&mut tx, &input.challenge_name).await?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type_tx(
        &mut tx,
        &challenge.challenge_name,
        &spec,
        &input.target,
        input.eval_type,
        &input.agent_id,
    )
    .await?;
    ensure_validation_uses_public_bundle(
        input.eval_type,
        &spec,
        &challenge.bundle_key,
        &challenge.public_bundle_key,
    )?;
    enforce_quota_admission(&mut tx, input).await?;
    ensure_parent_solution_submission_matches_scope_tx(
        &mut tx,
        input.parent_solution_submission_id.as_ref(),
        &input.agent_id,
        &challenge.challenge_name,
        &input.target,
    )
    .await?;

    let row = sqlx::query(
        r#"
        INSERT INTO solution_submissions (
            id, challenge_name, target, agent_id, artifact_key, note,
            artifact_zip_bytes, artifact_uncompressed_bytes, artifact_file_count, artifact_sha256,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval
        )
        VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6, $7, $8, $9, $10, 'pending', $11, $12::uuid, $13, FALSE)
        RETURNING
            id, challenge_name, target, agent_id, artifact_key, note,
            artifact_zip_bytes, artifact_uncompressed_bytes, artifact_file_count, artifact_sha256,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval,
            created_at, updated_at
        "#,
    )
    .bind(input.solution_submission_id.as_str())
    .bind(challenge.challenge_name.as_str())
    .bind(input.target.as_str())
    .bind(input.agent_id.as_str())
    .bind(input.artifact_key.as_str())
    .bind(&input.note)
    .bind(u64_to_i64(
        input.artifact_metadata.artifact_zip_bytes,
        "artifact_zip_bytes",
    )?)
    .bind(u64_to_i64(
        input.artifact_metadata.artifact_uncompressed_bytes,
        "artifact_uncompressed_bytes",
    )?)
    .bind(u64_to_i64(
        input.artifact_metadata.artifact_file_count,
        "artifact_file_count",
    )?)
    .bind(input.artifact_metadata.artifact_sha256.to_string())
    .bind(&input.explanation)
    .bind(
        input
            .parent_solution_submission_id
            .as_ref()
            .map(SolutionSubmissionId::as_str),
    )
    .bind(&input.credit_text)
    .fetch_one(&mut *tx)
    .await?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_key: input.artifact_key.clone(),
        bundle_key: challenge.bundle_key.clone(),
        public_bundle_key: challenge.public_bundle_key.clone(),
        challenge_name: challenge.challenge_name.clone(),
        target: input.target.clone(),
    })
    .map_err(|e| ServiceError::Internal(e.to_string()))?;

    let priority = if input.eval_type == ScoringMode::Official {
        10
    } else {
        0
    };
    let required_accelerator = required_accelerator_for_target(&spec, &input.target)?;

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id, solution_submission_id, challenge_name, target, required_accelerator, eval_type, status, priority, payload_json, scheduled_at
        )
        VALUES (
            $1::uuid, $2::uuid, $3, $4, $5, $6, 'staged', $7, $8,
            NOW()
        )
        "#,
    )
    .bind(input.job_id.as_str())
    .bind(input.solution_submission_id.as_str())
    .bind(challenge.challenge_name.as_str())
    .bind(input.target.as_str())
    .bind(required_accelerator.as_str())
    .bind(input.eval_type.as_str())
    .bind(priority)
    .bind(&payload)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(SolutionSubmissionRecord {
        id: solution_submission_id_from_row(&row, "id")?,
        challenge_name: challenge.challenge_name,
        target: target_from_row(&row, "target")?,
        agent_id: agent_id_from_row(&row, "agent_id")?,
        agent_display_name: None,
        challenge_title: None,
        challenge_spec: spec,
        artifact_key: storage_key_from_row(&row, "artifact_key")?,
        artifact_metadata: artifact_metadata_from_row(&row)?,
        note: row.try_get("note")?,
        status: row.try_get("status")?,
        explanation: row.try_get("explanation")?,
        parent_solution_submission_id: optional_solution_submission_id_from_row(
            &row,
            "parent_solution_submission_id",
        )?,
        credit_text: row.try_get("credit_text")?,
        visible_after_eval: row.try_get("visible_after_eval")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        evaluation_job_id: Some(input.job_id.clone()),
        evaluation_job_status: Some("staged".to_string()),
        evaluation: None,
        validation_evaluation: None,
        official_evaluation: None,
    })
}

/// Verify that an optional parent submission belongs to the same agent and ranking scope.
pub async fn ensure_parent_solution_submission_matches_scope(
    pool: &PgPool,
    parent_solution_submission_id: Option<&SolutionSubmissionId>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_parent_solution_submission_matches_scope_tx(
        &mut tx,
        parent_solution_submission_id,
        agent_id,
        challenge_name,
        target,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Return the accelerator requirement declared by the selected challenge target.
fn required_accelerator_for_target(
    spec: &ChallengeBundleSpec,
    target: &TargetName,
) -> Result<TargetAccelerator> {
    let target_spec = spec.target(target).ok_or_else(|| {
        ServiceError::Internal(format!(
            "challenge `{}` does not declare target `{target}` after admission validation",
            spec.challenge_name
        ))
    })?;
    Ok(target_spec.accelerator)
}

/// Enforce parent-submission lineage invariants inside a submission transaction.
async fn ensure_parent_solution_submission_matches_scope_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    parent_solution_submission_id: Option<&SolutionSubmissionId>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<()> {
    let Some(parent_solution_submission_id) = parent_solution_submission_id else {
        return Ok(());
    };

    let row = sqlx::query(
        r#"
        SELECT agent_id, challenge_name, target, status, visible_after_eval
        FROM solution_submissions
        WHERE id = $1::uuid
        LIMIT 1
        "#,
    )
    .bind(parent_solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(ServiceError::BadRequest(
            "parent_solution_submission_id does not reference an existing solution submission"
                .to_string(),
        ));
    };

    let parent_agent_id = agent_id_from_row(&row, "agent_id")?;
    let parent_challenge_name = challenge_name_from_row(&row, "challenge_name")?;
    let parent_target = target_from_row(&row, "target")?;
    let parent_status: String = row.try_get("status")?;
    let parent_visible: bool = row.try_get("visible_after_eval")?;

    if &parent_agent_id != agent_id
        || &parent_challenge_name != challenge_name
        || &parent_target != target
    {
        return Err(ServiceError::BadRequest(
            "parent_solution_submission_id must belong to the same agent, challenge_name, and target"
                .to_string(),
        ));
    }
    if parent_status != SolutionSubmissionStatus::Completed.as_str() || !parent_visible {
        return Err(ServiceError::BadRequest(
            "parent_solution_submission_id must reference a completed visible solution submission"
                .to_string(),
        ));
    }

    Ok(())
}

/// Delete a solution submission and its dependent jobs/evaluations.
pub async fn delete_solution_submission(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    sqlx::query("DELETE FROM solution_submissions WHERE id = $1::uuid")
        .bind(solution_submission_id.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch one solution submission with latest job state and validation/official evaluations.
pub async fn get_solution_submission_by_id(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<Option<SolutionSubmissionRecord>> {
    get_solution_submission_by_id_inner(pool, solution_submission_id, false).await
}

/// Fetch one public result-of-record submission with the latest completed official evaluation.
pub async fn get_public_solution_submission_by_id(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<Option<SolutionSubmissionRecord>> {
    get_solution_submission_by_id_inner(pool, solution_submission_id, true).await
}

/// Fetch one solution submission while optionally restricting official evaluations to completed
/// public result-of-record rows.
async fn get_solution_submission_by_id_inner(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
    completed_official_only: bool,
) -> Result<Option<SolutionSubmissionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_name, s.target, s.agent_id,
            p.title AS challenge_title, p.spec_json AS challenge_spec_json,
            a.display_name AS agent_display_name,
            s.artifact_key, s.note, s.status, s.explanation,
            s.artifact_zip_bytes, s.artifact_uncompressed_bytes, s.artifact_file_count, s.artifact_sha256,
            s.parent_solution_submission_id, s.credit_text, s.visible_after_eval,
            s.created_at, s.updated_at,
            j.id AS latest_job_id, j.status AS latest_job_status,
            pe.id AS validation_eval_id,
            pe.target AS validation_eval_target,
            pe.status AS validation_eval_status,
            pe.eval_type AS validation_eval_eval_type,
            pe.aggregate_metrics_json AS validation_eval_aggregate_metrics,
            pe.run_metrics_json AS validation_eval_run_metrics,
            pe.public_results_json AS validation_eval_public_results,
            pe.validation_summary_json AS validation_eval_validation_summary,
            pe.official_summary_json AS validation_eval_official_summary,
            pe.runner_log_storage_key AS validation_eval_runner_log_storage_key,
            pe.started_at AS validation_eval_started_at,
            pe.finished_at AS validation_eval_finished_at,
            oe.id AS official_eval_id,
            oe.target AS official_eval_target,
            oe.status AS official_eval_status,
            oe.eval_type AS official_eval_eval_type,
            oe.aggregate_metrics_json AS official_eval_aggregate_metrics,
            oe.run_metrics_json AS official_eval_run_metrics,
            oe.public_results_json AS official_eval_public_results,
            oe.validation_summary_json AS official_eval_validation_summary,
            oe.official_summary_json AS official_eval_official_summary,
            oe.runner_log_storage_key AS official_eval_runner_log_storage_key,
            oe.started_at AS official_eval_started_at,
            oe.finished_at AS official_eval_finished_at
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.challenge_name = s.challenge_name
        LEFT JOIN LATERAL (
            SELECT id, status FROM evaluation_jobs WHERE solution_submission_id = s.id ORDER BY created_at DESC LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, target, status, eval_type, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, runner_log_storage_key, started_at, finished_at
            FROM evaluations WHERE solution_submission_id = s.id AND eval_type = 'validation' AND target = s.target ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, target, status, eval_type, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, runner_log_storage_key, started_at, finished_at
            FROM evaluations
            WHERE solution_submission_id = s.id
              AND eval_type = 'official'
              AND target = s.target
              AND (NOT $2::boolean OR status = 'completed')
            ORDER BY created_at DESC
            LIMIT 1
        ) oe ON TRUE
        WHERE s.id = $1::uuid
        LIMIT 1
        "#
    )
        .bind(solution_submission_id.as_str())
        .bind(completed_official_only)
        .fetch_optional(pool)
        .await?;

    let Some(r) = row else {
        return Ok(None);
    };

    let validation_eval = parse_eval_from_row(&r, "validation_eval")?;
    let official_eval = parse_eval_from_row(&r, "official_eval")?;
    let challenge_spec_json: Value = r.try_get("challenge_spec_json")?;
    let challenge_spec = serde_json::from_value::<ChallengeBundleSpec>(challenge_spec_json)
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    Ok(Some(SolutionSubmissionRecord {
        id: solution_submission_id_from_row(&r, "id")?,
        challenge_name: challenge_name_from_row(&r, "challenge_name")?,
        target: target_from_row(&r, "target")?,
        agent_id: agent_id_from_row(&r, "agent_id")?,
        agent_display_name: r.try_get::<Option<String>, _>("agent_display_name")?,
        challenge_title: r.try_get::<Option<String>, _>("challenge_title")?,
        challenge_spec,
        artifact_key: storage_key_from_row(&r, "artifact_key")?,
        artifact_metadata: artifact_metadata_from_row(&r)?,
        note: r.try_get("note")?,
        status: r.try_get("status")?,
        explanation: r.try_get("explanation")?,
        parent_solution_submission_id: optional_solution_submission_id_from_row(
            &r,
            "parent_solution_submission_id",
        )?,
        credit_text: r.try_get("credit_text")?,
        visible_after_eval: r.try_get("visible_after_eval")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
        evaluation_job_id: optional_evaluation_job_id_from_row(&r, "latest_job_id")?,
        evaluation_job_status: r.try_get::<Option<String>, _>("latest_job_status")?,
        evaluation: official_eval.clone().or_else(|| validation_eval.clone()),
        validation_evaluation: validation_eval,
        official_evaluation: official_eval,
    }))
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<AdminSolutionSubmissionListItemRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id,
            s.challenge_name,
            s.target,
            p.title AS challenge_title,
            s.agent_id,
            a.display_name AS agent_display_name,
            s.note,
            s.status,
            s.visible_after_eval,
            s.created_at,
            s.updated_at,
            j.id AS latest_job_id,
            j.status AS latest_job_status,
            j.eval_type AS latest_job_eval_type,
            ve.status AS validation_status,
            oe.status AS official_status
        FROM solution_submissions s
        JOIN challenges p ON p.challenge_name = s.challenge_name
        JOIN agents a ON a.id = s.agent_id
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type
            FROM evaluation_jobs
            WHERE solution_submission_id = s.id
            ORDER BY created_at DESC
            LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT status
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'validation'
            ORDER BY created_at DESC
            LIMIT 1
        ) ve ON TRUE
        LEFT JOIN LATERAL (
            SELECT status
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official'
            ORDER BY created_at DESC
            LIMIT 1
        ) oe ON TRUE
        ORDER BY s.updated_at DESC, s.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(AdminSolutionSubmissionListItemRecord {
                id: solution_submission_id_from_row(&r, "id")?,
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                challenge_title: r.try_get("challenge_title")?,
                target: target_from_row(&r, "target")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                note: r.try_get("note")?,
                status: solution_submission_status_from_row(&r, "status")?,
                visible_after_eval: r.try_get("visible_after_eval")?,
                latest_job_id: optional_evaluation_job_id_from_row(&r, "latest_job_id")?,
                latest_job_status: optional_evaluation_job_status_from_row(
                    &r,
                    "latest_job_status",
                )?,
                latest_job_eval_type: optional_scoring_mode_from_row(&r, "latest_job_eval_type")?,
                validation_status: optional_evaluation_status_from_row(&r, "validation_status")?,
                official_status: optional_evaluation_status_from_row(&r, "official_status")?,
                created_at: r.try_get("created_at")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// List solution submissions for a challenge after an official evaluation makes them visible.
pub async fn list_public_solution_submissions_for_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    limit: i64,
) -> Result<Vec<PublicSolutionSubmissionListItemRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_name, s.target, p.title AS challenge_title,
            s.agent_id, a.display_name AS agent_display_name, s.status, s.note, s.explanation,
            s.parent_solution_submission_id, s.credit_text, s.created_at, s.updated_at,
            COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.challenge_name = s.challenge_name
        LEFT JOIN LATERAL (
            SELECT aggregate_metrics_json, official_summary_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed' AND target = s.target
            ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE p.challenge_name = $1
          AND s.visible_after_eval = TRUE
          AND s.target = $2
        ORDER BY s.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let official_metrics: Vec<MetricValue> = decode_optional_json(
                r.try_get::<Option<Value>, _>("official_metrics")?,
                "solution submission official metrics",
            )?
            .unwrap_or_default();
            Ok(PublicSolutionSubmissionListItemRecord {
                id: solution_submission_id_from_row(&r, "id")?,
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                target: target_from_row(&r, "target")?,
                challenge_title: r.try_get("challenge_title")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                status: solution_submission_status_from_row(&r, "status")?,
                note: r.try_get("note")?,
                explanation: r.try_get("explanation")?,
                parent_solution_submission_id: optional_solution_submission_id_from_row(
                    &r,
                    "parent_solution_submission_id",
                )?,
                credit_text: r.try_get("credit_text")?,
                official_metrics,
                created_at: r.try_get("created_at")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Count visible solution submissions for a challenge and target.
pub async fn count_public_solution_submissions_for_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM solution_submissions s
        WHERE s.challenge_name = $1
          AND s.visible_after_eval = TRUE
          AND s.target = $2
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Count aggregate currently public observer stats.
pub async fn public_observer_stats(pool: &PgPool) -> Result<PublicObserverStatsRecord> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        WITH public_challenges AS (
            SELECT challenge_name, spec_json
            FROM challenges
            WHERE status = 'active'
              AND spec_json IS NOT NULL
        ),
        public_submissions AS (
            SELECT s.agent_id
            FROM solution_submissions s
            JOIN public_challenges c ON c.challenge_name = s.challenge_name
            WHERE s.visible_after_eval = TRUE
              AND s.status = 'completed'
              AND (
                c.spec_json #>> '{visibility,result_detail}' = 'submitter_live_public_live'
                OR (
                    c.spec_json #>> '{visibility,result_detail}' = 'submitter_live_public_after_close'
                    AND (c.spec_json ->> 'closes_at')::timestamptz <= NOW()
                )
              )
        ),
        public_challenge_attempts AS (
            SELECT s.id
            FROM solution_submissions s
            JOIN public_challenges c ON c.challenge_name = s.challenge_name
        )
        SELECT
            (SELECT COUNT(*)::bigint FROM public_challenges) AS challenge_count,
            (SELECT COUNT(DISTINCT agent_id)::bigint FROM public_submissions) AS agent_count,
            (SELECT COUNT(*)::bigint FROM public_submissions) AS public_completed_submission_count,
            (SELECT COUNT(*)::bigint FROM public_challenge_attempts) AS total_solution_attempt_count
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(PublicObserverStatsRecord {
        challenge_count: count_to_u64("challenge_count", row.0)?,
        agent_count: count_to_u64("agent_count", row.1)?,
        public_completed_submission_count: count_to_u64(
            "public_completed_submission_count",
            row.2,
        )?,
        total_solution_attempt_count: count_to_u64("total_solution_attempt_count", row.3)?,
    })
}
