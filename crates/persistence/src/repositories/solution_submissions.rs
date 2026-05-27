use sqlx::PgPool;

use crate::db;
use crate::repositories::CreateSolutionSubmissionInput;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    AdminSolutionSubmissionListItemDto, PublicSolutionSubmissionListItemDto,
};
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct SolutionSubmissionsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl SolutionSubmissionsRepository<'_> {
    pub async fn create_with_job(
        &self,
        input: &CreateSolutionSubmissionInput,
    ) -> Result<crate::repositories::SolutionSubmissionRecord> {
        db::solution_submissions::create_solution_submission_with_job(self.pool, input).await
    }

    pub async fn ensure_parent_matches_scope(
        &self,
        parent_solution_submission_id: Option<&SolutionSubmissionId>,
        agent_id: &AgentId,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<()> {
        db::solution_submissions::ensure_parent_solution_submission_matches_scope(
            self.pool,
            parent_solution_submission_id,
            agent_id,
            challenge_name,
            target,
        )
        .await
    }

    pub async fn delete(&self, solution_submission_id: &SolutionSubmissionId) -> Result<()> {
        db::solution_submissions::delete_solution_submission(self.pool, solution_submission_id)
            .await
    }

    pub async fn get_by_id(
        &self,
        id: &SolutionSubmissionId,
    ) -> Result<Option<crate::repositories::SolutionSubmissionRecord>> {
        db::solution_submissions::get_solution_submission_by_id(self.pool, id).await
    }

    pub async fn get_public_by_id(
        &self,
        id: &SolutionSubmissionId,
    ) -> Result<Option<crate::repositories::SolutionSubmissionRecord>> {
        db::solution_submissions::get_public_solution_submission_by_id(self.pool, id).await
    }

    pub async fn list_admin(&self, limit: i64) -> Result<Vec<AdminSolutionSubmissionListItemDto>> {
        db::solution_submissions::list_admin_solution_submissions(self.pool, limit).await
    }

    pub async fn list_public_for_challenge(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<PublicSolutionSubmissionListItemDto>> {
        db::solution_submissions::list_public_solution_submissions_for_challenge(
            self.pool,
            challenge_name,
            target,
            limit,
        )
        .await
    }

    pub async fn count_public_for_challenge(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<i64> {
        db::solution_submissions::count_public_solution_submissions_for_challenge(
            self.pool,
            challenge_name,
            target,
        )
        .await
    }

    pub async fn observer_stats(&self) -> Result<(i64, i64, i64)> {
        db::solution_submissions::public_observer_stats(self.pool).await
    }

    pub async fn count_recent_runs_for_agent_challenge(
        &self,
        agent_id: &AgentId,
        challenge_name: &ChallengeName,
        target: &TargetName,
        eval_type: ScoringMode,
        window_seconds: i64,
    ) -> Result<i64> {
        db::validation_quotas::count_recent_runs_for_agent_challenge(
            self.pool,
            agent_id,
            challenge_name,
            target,
            eval_type,
            window_seconds,
        )
        .await
    }

    pub async fn count_lifetime_runs_for_agent_challenge(
        &self,
        agent_id: &AgentId,
        challenge_name: &ChallengeName,
        target: &TargetName,
        eval_type: ScoringMode,
    ) -> Result<i64> {
        db::validation_quotas::count_lifetime_runs_for_agent_challenge(
            self.pool,
            agent_id,
            challenge_name,
            target,
            eval_type,
        )
        .await
    }
}
