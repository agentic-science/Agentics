use agentics_config::Config;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::AgentId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    PublishedChallengeAdmission, Repositories, SolutionSubmissionQuotaAdmission,
};

pub(super) const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// Performs pre-upload quota checks so abusive requests fail before artifact decode.
pub(super) async fn ensure_submission_quota_available(
    pool: &sqlx::PgPool,
    config: &Config,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
    challenge_lifetime_limit: Option<i64>,
) -> Result<()> {
    let repos = Repositories::new(pool);
    let limit = match eval_type {
        ScoringMode::Validation => i64::from(config.quotas.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(config.quotas.official_runs_per_agent_challenge_day),
    };
    let used = repos
        .solution_submissions()
        .count_recent_runs_for_agent_challenge(
            agent_id,
            challenge_name,
            target,
            eval_type,
            SUBMISSION_QUOTA_WINDOW_SECONDS,
        )
        .await?;

    if used >= limit {
        return Err(ServiceError::TooManyRequests(format!(
            "{} quota exceeded for challenge `{challenge_name}`: {used} of {limit} runs used in the last 24 hours",
            eval_type.as_str()
        )));
    }

    if let Some(limit) = challenge_lifetime_limit {
        let used = repos
            .solution_submissions()
            .count_lifetime_runs_for_agent_challenge(agent_id, challenge_name, target, eval_type)
            .await?;
        if used >= limit {
            return Err(ServiceError::TooManyRequests(format!(
                "{} challenge limit exceeded for challenge `{challenge_name}`: {used} of {limit} lifetime runs used",
                eval_type.as_str()
            )));
        }
    }

    if eval_type == ScoringMode::Official {
        let active = repos
            .evaluation_jobs()
            .count_active(ScoringMode::Official)
            .await?;
        let max_active = i64::from(config.quotas.max_active_official_jobs);
        if active >= max_active {
            return Err(ServiceError::TooManyRequests(format!(
                "official evaluation queue is full: {active} of {max_active} official jobs are queued or running"
            )));
        }
    }

    Ok(())
}

/// Selects the challenge-level run limit that applies to the requested scoring mode.
pub(super) fn challenge_lifetime_limit(
    admission: &PublishedChallengeAdmission,
    eval_type: ScoringMode,
) -> Option<i64> {
    match eval_type {
        ScoringMode::Validation => admission.validation_submission_limit,
        ScoringMode::Official => admission.official_submission_limit,
    }
}

/// Build the transaction-owned quota admission policy for one submission.
pub(super) fn quota_admission(
    config: &Config,
    eval_type: ScoringMode,
    challenge_lifetime_limit: Option<i64>,
) -> SolutionSubmissionQuotaAdmission {
    let per_agent_challenge_limit = match eval_type {
        ScoringMode::Validation => i64::from(config.quotas.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(config.quotas.official_runs_per_agent_challenge_day),
    };
    let max_active_official_jobs = (eval_type == ScoringMode::Official)
        .then_some(i64::from(config.quotas.max_active_official_jobs));

    SolutionSubmissionQuotaAdmission {
        window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
        per_agent_challenge_limit,
        challenge_lifetime_limit,
        max_active_official_jobs,
    }
}
