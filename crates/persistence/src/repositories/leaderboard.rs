use sqlx::PgPool;

use crate::db;
use crate::repositories::LeaderboardMetricEntry;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::names::TargetName;
use agentics_domain::models::request::LeaderboardEntryDto;
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct LeaderboardRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl LeaderboardRepository<'_> {
    pub async fn list_entries(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<LeaderboardEntryDto>> {
        db::leaderboard::list_leaderboard_entries(self.pool, challenge_name, target, limit).await
    }

    pub async fn list_entries_with_metric_payloads(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<LeaderboardMetricEntry>> {
        db::leaderboard::list_leaderboard_entries_with_metric_payloads(
            self.pool,
            challenge_name,
            target,
            limit,
        )
        .await
    }
}
