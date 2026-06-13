use sqlx::{Postgres, Transaction};

use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::AgentId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_error::{Result, ServiceError};

use super::CreateSolutionSubmissionInput;

/// Enforce solution-submission quota admission inside the creation transaction.
pub(super) async fn enforce_quota_admission(
    tx: &mut Transaction<'_, Postgres>,
    input: &CreateSolutionSubmissionInput,
) -> Result<()> {
    let mut scopes = vec![format!(
        "agent:{}:challenge:{}:target:{}:mode:{}:daily",
        input.agent_id,
        input.challenge_name,
        input.target,
        input.eval_type.as_str()
    )];
    if input.eval_type == ScoringMode::Official {
        scopes.push("global:official-active".to_string());
    }
    if input.quota_admission.challenge_lifetime_limit.is_some() {
        scopes.push(format!(
            "agent:{}:challenge:{}:target:{}:mode:{}:lifetime",
            input.agent_id,
            input.challenge_name,
            input.target,
            input.eval_type.as_str()
        ));
    }
    scopes.sort();

    for scope in scopes {
        lock_quota_scope(tx, &scope).await?;
    }

    let used = count_recent_runs_for_agent_challenge_tx(
        tx,
        &input.agent_id,
        &input.challenge_name,
        &input.target,
        input.eval_type,
        input.quota_admission.window_seconds,
    )
    .await?;
    let limit = input.quota_admission.per_agent_challenge_limit;
    if used >= limit {
        return Err(ServiceError::TooManyRequests(format!(
            "{} quota exceeded for challenge `{}`: {} of {} runs used in the last 24 hours",
            input.eval_type.as_str(),
            input.challenge_name,
            used,
            limit
        )));
    }

    if let Some(limit) = input.quota_admission.challenge_lifetime_limit {
        let used = count_lifetime_runs_for_agent_challenge_tx(
            tx,
            &input.agent_id,
            &input.challenge_name,
            &input.target,
            input.eval_type,
        )
        .await?;
        if used >= limit {
            return Err(ServiceError::TooManyRequests(format!(
                "{} challenge limit exceeded for challenge `{}`: {} of {} lifetime runs used",
                input.eval_type.as_str(),
                input.challenge_name,
                used,
                limit
            )));
        }
    }

    if let Some(max_active) = input.quota_admission.max_active_official_jobs {
        let active = count_active_evaluation_jobs_tx(tx, ScoringMode::Official).await?;
        if active >= max_active {
            return Err(ServiceError::TooManyRequests(format!(
                "official evaluation queue is full: {active} of {max_active} official jobs are staged, queued, or running"
            )));
        }
    }

    Ok(())
}

async fn lock_quota_scope(tx: &mut Transaction<'_, Postgres>, scope: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ($1)
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(scope)
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
    .bind(scope)
    .fetch_one(&mut **tx)
    .await?;

    Ok(())
}

async fn count_recent_runs_for_agent_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN LATERAL (
            SELECT eval_type
            FROM evaluation_jobs
            WHERE solution_submission_id = s.id
            ORDER BY created_at ASC, id ASC
            LIMIT 1
        ) first_job ON TRUE
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND first_job.eval_type = $4
          AND s.created_at >= NOW() - ($5::DOUBLE PRECISION * INTERVAL '1 second')
        "#,
    )
    .bind(agent_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(eval_type.as_str())
    .bind(window_seconds)
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

async fn count_lifetime_runs_for_agent_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN LATERAL (
            SELECT eval_type
            FROM evaluation_jobs
            WHERE solution_submission_id = s.id
            ORDER BY created_at ASC, id ASC
            LIMIT 1
        ) first_job ON TRUE
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND first_job.eval_type = $4
        "#,
    )
    .bind(agent_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(eval_type.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

async fn count_active_evaluation_jobs_tx(
    tx: &mut Transaction<'_, Postgres>,
    eval_type: ScoringMode,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM evaluation_jobs
        WHERE eval_type = $1
          AND status IN ('staged', 'queued', 'running')
        "#,
    )
    .bind(eval_type.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}
