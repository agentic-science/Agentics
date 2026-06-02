use sqlx::{PgPool, Postgres, Transaction};

use agentics_domain::models::ids::AgentId;
use agentics_domain::models::names::ChallengeName;
use agentics_error::Result;

/// Grant challenge-owner permissions to an agent.
pub async fn add_challenge_owner(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    add_challenge_owner_tx(&mut tx, challenge_name, agent_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Handles add challenge owner tx for this module.
pub async fn add_challenge_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO challenge_owners (challenge_name, agent_id)
        VALUES ($1, $2::uuid)
        ON CONFLICT (challenge_name, agent_id) DO NOTHING
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(agent_id.as_str())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Check whether an agent is an owner of a challenge.
pub async fn agent_owns_challenge(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
            WHERE challenge_name = $1 AND agent_id = $2::uuid
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(agent_id.as_str())
    .fetch_one(pool)
    .await?;

    Ok(exists)
}
