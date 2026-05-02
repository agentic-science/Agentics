use sqlx::PgPool;

use crate::error::Result;

/// Count validation runs that consumed quota for one agent and challenge.
///
/// Queued, running, completed, and failed validation jobs all count because
/// they consume API, storage, and worker capacity once accepted.
pub async fn count_recent_validation_runs_for_agent_challenge(
    pool: &PgPool,
    agent_id: &str,
    challenge_id: &str,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1
          AND s.challenge_id = $2
          AND j.eval_type = 'validation'
          AND s.created_at >= NOW() - ($3::DOUBLE PRECISION * INTERVAL '1 second')
        "#,
    )
    .bind(agent_id)
    .bind(challenge_id)
    .bind(window_seconds)
    .fetch_one(pool)
    .await?;

    Ok(count)
}
