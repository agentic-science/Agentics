use sqlx::PgPool;

use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::AgentId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_error::Result;

/// Count participant-created submissions that consumed quota for one agent and challenge.
///
/// Queued, running, completed, and failed first jobs all count because the
/// participant consumed API, storage, and worker capacity once accepted. Admin
/// rejudges are extra jobs for an existing submission and do not consume a new
/// participant submission quota slot.
pub async fn count_recent_runs_for_agent_challenge(
    pool: &PgPool,
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
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Count participant-created accepted submissions for one agent, challenge, target, and mode.
pub async fn count_lifetime_runs_for_agent_challenge(
    pool: &PgPool,
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
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Count validation runs that consumed quota for one agent and challenge.
pub async fn count_recent_validation_runs_for_agent_challenge(
    pool: &PgPool,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    window_seconds: i64,
) -> Result<i64> {
    count_recent_runs_for_agent_challenge(
        pool,
        agent_id,
        challenge_name,
        target,
        ScoringMode::Validation,
        window_seconds,
    )
    .await
}
