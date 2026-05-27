use sqlx::PgPool;

use crate::db;
use crate::repositories::{HeartbeatPayload, StaleJobReapResult};
use agentics_domain::error::Result;
use agentics_domain::models::request::AdminServiceHeartbeatDto;

#[derive(Debug, Clone, Copy)]
pub struct MaintenanceRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl MaintenanceRepository<'_> {
    pub async fn upsert_service_heartbeat(
        &self,
        worker_id: &str,
        payload: &HeartbeatPayload,
    ) -> Result<()> {
        db::maintenance::upsert_service_heartbeat(self.pool, worker_id, payload).await
    }

    pub async fn list_service_heartbeats(&self) -> Result<Vec<AdminServiceHeartbeatDto>> {
        db::maintenance::list_service_heartbeats(self.pool).await
    }

    pub async fn reap_stuck_jobs(&self, timeout_minutes: i32) -> Result<StaleJobReapResult> {
        db::maintenance::reap_stuck_jobs(self.pool, timeout_minutes).await
    }
}
