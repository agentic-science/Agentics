use sqlx::PgPool;

use crate::error::Result;
use crate::models::evaluation::ScoringMode;
use crate::models::ids::AgentId;
use crate::models::names::{ChallengeName, TargetName};

/// Count evaluation jobs that consumed quota for one agent and challenge.
///
/// Queued, running, completed, and failed jobs all count because
/// they consume API, storage, and worker capacity once accepted.
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
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND j.eval_type = $4
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

/// Count accepted evaluation jobs for one agent, challenge, target, and mode.
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
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND j.eval_type = $4
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
