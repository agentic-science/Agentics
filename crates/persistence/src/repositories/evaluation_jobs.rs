use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    EvaluationJobRecord, MarkEvaluationStartedInput, PersistedEvaluationResult,
    QueueEvaluationJobInput,
};
use agentics_config::WorkerAccelerators;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::EvaluationJobId;
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct EvaluationJobsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl EvaluationJobsRepository<'_> {
    pub async fn claim_next(
        &self,
        worker_id: &str,
        accelerators: WorkerAccelerators,
    ) -> Result<Option<EvaluationJobRecord>> {
        db::evaluation_jobs::claim_next_evaluation_job(self.pool, worker_id, accelerators).await
    }

    pub async fn refresh_claim(
        &self,
        job_id: &EvaluationJobId,
        worker_id: &str,
        attempt_count: i32,
    ) -> Result<bool> {
        db::evaluation_jobs::refresh_evaluation_job_claim(
            self.pool,
            job_id,
            worker_id,
            attempt_count,
        )
        .await
    }

    pub async fn runner_claim(
        &self,
        job_id: &EvaluationJobId,
        stale_minutes: i32,
    ) -> Result<Option<crate::repositories::RunnerJobClaimRecord>> {
        db::evaluation_jobs::get_runner_job_claim(self.pool, job_id, stale_minutes).await
    }

    pub async fn requeue_for_capacity(
        &self,
        job_id: &EvaluationJobId,
        worker_id: &str,
        attempt_count: i32,
        last_error: &str,
    ) -> Result<bool> {
        db::evaluation_jobs::requeue_running_evaluation_job_for_capacity(
            self.pool,
            job_id,
            worker_id,
            attempt_count,
            last_error,
        )
        .await
    }

    pub async fn mark_ready(&self, job_id: &EvaluationJobId) -> Result<()> {
        db::evaluation_jobs::mark_evaluation_job_ready(self.pool, job_id).await
    }

    pub async fn queue(&self, input: &QueueEvaluationJobInput) -> Result<EvaluationJobRecord> {
        db::evaluation_jobs::queue_evaluation_job(self.pool, input).await
    }

    pub async fn count_active(&self, eval_type: ScoringMode) -> Result<i64> {
        db::evaluation_jobs::count_active_evaluation_jobs(self.pool, eval_type).await
    }

    pub async fn mark_started(&self, input: &MarkEvaluationStartedInput) -> Result<bool> {
        db::evaluations::mark_evaluation_started(self.pool, input).await
    }

    pub async fn mark_finished(&self, input: &PersistedEvaluationResult) -> Result<bool> {
        db::evaluations::mark_evaluation_finished(self.pool, input).await
    }
}
