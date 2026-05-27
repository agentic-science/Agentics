use chrono::{DateTime, Utc};

use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeResultDetailVisibility, ChallengeSolutionPublicationPolicy,
    ChallengeVisibility,
};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{ChallengeRecord, Repositories, SolutionSubmissionRecord};

/// Audience-specific projection for solution submission details.
#[derive(Debug, Clone, Copy)]
pub enum SolutionSubmissionAudience {
    /// The submitting agent can see its artifact path, job id, and validation details.
    Owner,
    /// Public viewers can only see ranking-visible data.
    Public,
}

impl SolutionSubmissionAudience {
    /// Returns whether this audience may see the stored solution artifact key.
    pub(super) fn includes_artifact_key(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see the current evaluation job handle.
    pub(super) fn includes_evaluation_job(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see validation-mode evaluation details.
    pub(super) fn includes_validation_details(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see submitter-facing official aggregate feedback.
    pub(super) fn includes_official_aggregate_feedback(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// Loads a visible public submission by id.
pub(super) async fn public_visible_solution_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionRecord> {
    let solution_submission = Repositories::new(pool)
        .solution_submissions()
        .get_public_by_id(id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(ServiceError::NotFound);
    }
    Ok(solution_submission)
}

/// Loads the public challenge record together with its parsed policy-bearing spec.
pub(super) async fn load_challenge_policy(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<(ChallengeRecord, ChallengeBundleSpec)> {
    let challenge = Repositories::new(pool)
        .challenges()
        .get_public(challenge_name)
        .await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    Ok((challenge, spec))
}

/// Enforces whether unauthenticated users may inspect a submission's detailed result report.
pub(super) async fn ensure_public_result_detail_visible(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_public_result_detail_visible_for_spec(&spec)
}

/// Enforces whether unauthenticated users may inspect detailed results for a parsed spec.
pub(super) fn ensure_public_result_detail_visible_for_spec(
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => Ok(()),
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(spec)? =>
        {
            Ok(())
        }
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => Err(ServiceError::NotFound),
    }
}

/// Enforces whether unauthenticated users may download a submission artifact.
pub(super) async fn ensure_public_solution_artifact_visible(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(&spec)? => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => return Err(ServiceError::NotFound),
    }

    match spec.solution_publication {
        ChallengeSolutionPublicationPolicy::Public => Ok(()),
        ChallengeSolutionPublicationPolicy::PublicAfterClose if challenge_has_closed(&spec)? => {
            Ok(())
        }
        ChallengeSolutionPublicationPolicy::Private
        | ChallengeSolutionPublicationPolicy::PublicAfterClose => Err(ServiceError::NotFound),
    }
}

/// Applies challenge visibility policy to an aggregate public surface.
pub(super) fn ensure_visibility_allows_public(
    visibility: ChallengeVisibility,
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    match visibility {
        ChallengeVisibility::PublicLive => Ok(()),
        ChallengeVisibility::PublicAfterClose if challenge_has_closed(spec)? => Ok(()),
        ChallengeVisibility::PublicAfterClose | ChallengeVisibility::Hidden => {
            Err(ServiceError::NotFound)
        }
    }
}

/// Rejects ranking-context requests whose scope does not match the submission record.
pub fn ensure_ranking_scope_matches_submission(
    solution_submission: &SolutionSubmissionRecord,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<()> {
    if solution_submission.challenge_name != *challenge_name
        || solution_submission.target != *target
    {
        return Err(ServiceError::BadRequest(
            "ranking scope must match the solution submission challenge_name and target"
                .to_string(),
        ));
    }
    Ok(())
}

/// Returns whether the current wall clock is past the challenge close time.
fn challenge_has_closed(spec: &ChallengeBundleSpec) -> Result<bool> {
    let Some(closes_at) = spec.closes_at.as_deref() else {
        return Ok(false);
    };
    let closes_at = DateTime::parse_from_rfc3339(closes_at)
        .map_err(|e| ServiceError::Internal(format!("invalid persisted challenge closes_at: {e}")))?
        .with_timezone(&Utc);
    Ok(Utc::now() >= closes_at)
}
