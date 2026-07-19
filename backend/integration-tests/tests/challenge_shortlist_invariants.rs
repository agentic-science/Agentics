//! Transactional challenge-shortlist lifecycle integration tests.

mod helpers;

use std::sync::Arc;

use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{AgentId, ChallengeShortlistRevisionId, HumanId};
use agentics_domain::models::names::ChallengeName;
use agentics_domain::storage::StorageKey;
use agentics_error::ServiceError;
use agentics_persistence::{CreateChallengeShortlistRevisionInput, Repositories};
use helpers::create_creator_session;
use tokio::sync::Barrier;

/// Verifies an archive that owns the challenge lock prevents a concurrent shortlist revision.
#[sqlx::test(migrations = "../migrations")]
async fn archive_winning_race_rejects_shortlist_revision(pool: sqlx::PgPool) {
    let creator = create_creator_session(&pool, 9201, "shortlist-race-owner").await;
    let human_id = HumanId::try_new(creator.human_id).expect("creator human id should parse");
    let challenge_name = ChallengeName::try_new("shortlist-archive-race".to_string())
        .expect("challenge name should parse");
    let agent_id = AgentId::generate();

    sqlx::query(
        r#"
        INSERT INTO challenges (
            challenge_name, title, summary, bundle_key, public_bundle_key,
            statement_key, spec_json, starts_at, status
        )
        VALUES (
            $1, 'Shortlist race', '{"en":"race","zh":"race"}'::jsonb,
            'challenge-bundles/race.tar', 'challenge-public-bundles/race.tar',
            'challenge-statements/race.md', '{}'::jsonb, NOW(), 'active'
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .execute(&pool)
    .await
    .expect("challenge should insert");
    Repositories::new(&pool)
        .challenges()
        .add_owner(&challenge_name, &human_id)
        .await
        .expect("challenge owner should insert");
    sqlx::query("INSERT INTO agents (id, display_name, status) VALUES ($1::uuid, $2, 'active')")
        .bind(agent_id.as_str())
        .bind("shortlist-race-agent")
        .execute(&pool)
        .await
        .expect("agent should insert");

    let mut archive_tx = pool
        .begin()
        .await
        .expect("archive transaction should begin");
    sqlx::query(
        "UPDATE challenges SET status = 'archived', updated_at = NOW() WHERE challenge_name = $1",
    )
    .bind(challenge_name.as_str())
    .execute(&mut *archive_tx)
    .await
    .expect("archive transaction should own challenge lock");

    let barrier = Arc::new(Barrier::new(2));
    let shortlist_pool = pool.clone();
    let shortlist_barrier = barrier.clone();
    let shortlist_challenge_name = challenge_name.clone();
    let shortlist_human_id = human_id.clone();
    let shortlist_agent_id = agent_id.clone();
    let shortlist = tokio::spawn(async move {
        shortlist_barrier.wait().await;
        Repositories::new(&shortlist_pool)
            .challenges()
            .create_shortlist_revision(&CreateChallengeShortlistRevisionInput {
                revision_id: ChallengeShortlistRevisionId::generate(),
                challenge_name: shortlist_challenge_name,
                uploader_human_id: shortlist_human_id,
                storage_key: StorageKey::try_new("challenge-shortlists/race/revision.json")
                    .expect("storage key should parse"),
                sha256: Sha256Digest::try_new("a".repeat(64)).expect("digest should parse"),
                requested_count: 1,
                agent_ids_to_add: vec![shortlist_agent_id],
            })
            .await
    });

    barrier.wait().await;
    archive_tx
        .commit()
        .await
        .expect("archive transaction should commit");
    let shortlist_result = shortlist.await.expect("shortlist task should join");
    assert!(matches!(shortlist_result, Err(ServiceError::Conflict)));

    let revision_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_shortlist_revisions WHERE challenge_name = $1",
    )
    .bind(challenge_name.as_str())
    .fetch_one(&pool)
    .await
    .expect("revision count should load");
    assert_eq!(revision_count, 0);
}
