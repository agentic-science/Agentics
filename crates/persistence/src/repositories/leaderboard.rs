use sqlx::PgPool;

use crate::db;
use crate::repositories::LeaderboardMetricEntry;
use agentics_domain::error::Result;
use agentics_domain::models::ids::ChallengeId;
use agentics_domain::models::names::TargetName;
use agentics_domain::models::request::LeaderboardEntryDto;

#[derive(Debug, Clone, Copy)]
pub struct LeaderboardRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl LeaderboardRepository<'_> {
    pub async fn list_entries(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<LeaderboardEntryDto>> {
        db::leaderboard::list_leaderboard_entries(self.pool, challenge_id, target, limit).await
    }

    pub async fn list_entries_with_metric_payloads(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<LeaderboardMetricEntry>> {
        db::leaderboard::list_leaderboard_entries_with_metric_payloads(
            self.pool,
            challenge_id,
            target,
            limit,
        )
        .await
    }
}
