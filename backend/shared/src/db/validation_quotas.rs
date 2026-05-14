use sqlx::PgPool;

use crate::error::Result;
use crate::models::evaluation::ScoringMode;

/// Count evaluation jobs that consumed quota for one agent and challenge.
///
/// Queued, running, completed, and failed jobs all count because
/// they consume API, storage, and worker capacity once accepted.
pub async fn count_recent_runs_for_agent_challenge(
    pool: &PgPool,
    agent_id: &str,
    challenge_id: &str,
    round_id: &str,
    benchmark_target_id: &str,
    eval_type: ScoringMode,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1
          AND s.challenge_id = $2
          AND s.round_id = $3
          AND s.benchmark_target_id = $4
          AND j.eval_type = $5
          AND s.created_at >= NOW() - ($6::DOUBLE PRECISION * INTERVAL '1 second')
        "#,
    )
    .bind(agent_id)
    .bind(challenge_id)
    .bind(round_id)
    .bind(benchmark_target_id)
    .bind(eval_type.as_str())
    .bind(window_seconds)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Count validation runs that consumed quota for one agent and challenge.
pub async fn count_recent_validation_runs_for_agent_challenge(
    pool: &PgPool,
    agent_id: &str,
    challenge_id: &str,
    round_id: &str,
    benchmark_target_id: &str,
    window_seconds: i64,
) -> Result<i64> {
    count_recent_runs_for_agent_challenge(
        pool,
        agent_id,
        challenge_id,
        round_id,
        benchmark_target_id,
        ScoringMode::Validation,
        window_seconds,
    )
    .await
}
