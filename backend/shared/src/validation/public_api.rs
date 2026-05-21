//! Shared validation for public read API query contracts.

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::names::TargetName;

/// Default public challenge catalog page size.
pub const DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT: i64 = 100;
/// Default visible public solution submission page size.
pub const DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT: i64 = 20;
/// Default leaderboard page size.
pub const DEFAULT_PUBLIC_LEADERBOARD_LIMIT: i64 = 50;
/// Maximum page size for public list-style reads.
pub const MAX_PUBLIC_LIST_LIMIT: i64 = 100;

/// Bounded pagination parameters for a public collection endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicPagination {
    pub limit: i64,
    pub offset: i64,
}

/// Validate a public list limit without silently widening expensive reads.
pub fn bounded_public_limit(
    requested: Option<i64>,
    default_limit: i64,
    label: &str,
) -> Result<i64> {
    let limit = requested.unwrap_or(default_limit);
    if !(1..=MAX_PUBLIC_LIST_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest(format!(
            "{label} limit must be between 1 and {MAX_PUBLIC_LIST_LIMIT}"
        )));
    }
    Ok(limit)
}

/// Validate a public list offset without allowing negative pagination cursors.
pub fn bounded_public_offset(requested: Option<i64>, label: &str) -> Result<i64> {
    let offset = requested.unwrap_or(0);
    if offset < 0 {
        return Err(AppError::BadRequest(format!(
            "{label} offset must be greater than or equal to 0"
        )));
    }
    Ok(offset)
}

/// Validate limit and offset together for public list endpoints.
pub fn public_pagination(
    requested_limit: Option<i64>,
    requested_offset: Option<i64>,
    default_limit: i64,
    label: &str,
) -> Result<PublicPagination> {
    Ok(PublicPagination {
        limit: bounded_public_limit(requested_limit, default_limit, label)?,
        offset: bounded_public_offset(requested_offset, label)?,
    })
}

/// Resolve an explicit public target query against the challenge spec.
pub fn resolve_required_public_target(
    spec: &ChallengeBundleSpec,
    requested_target: Option<&str>,
) -> Result<TargetName> {
    let Some(target) = requested_target else {
        return Err(AppError::BadRequest(
            "target query parameter is required".to_string(),
        ));
    };
    let target = target
        .parse::<TargetName>()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if spec.target(&target).is_some() {
        return Ok(target);
    }
    Err(AppError::BadRequest(format!(
        "challenge does not support target `{target}`"
    )))
}

/// Resolve an optional public target filter against the challenge spec.
pub fn resolve_optional_public_target(
    spec: &ChallengeBundleSpec,
    requested_target: Option<&str>,
) -> Result<Option<TargetName>> {
    requested_target
        .map(|target| resolve_required_public_target(spec, Some(target)))
        .transpose()
}

#[cfg(test)]
mod tests {
    use crate::models::challenge::{
        ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
        ChallengeExecutionSpec, ChallengeSolutionPublicationPolicy, ChallengeVisibility,
        ChallengeVisibilitySpec, DatasetsSpec, EvaluatorSpec, PrivateBenchmarkPolicy,
        PublicChallengeBundleSpec, SeparatedEvaluatorExecutionSpec, SolutionSpec,
    };
    use crate::models::evaluation::ScoreVisibility;
    use crate::models::localization::LocalizedText;
    use crate::models::names::{ChallengeKeyword, ChallengeName, TargetName};
    use crate::models::paths::BundleRelativePath;
    use crate::zip_project::ZIP_PROJECT_PROTOCOL;

    use super::{
        DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT, bounded_public_limit, public_pagination,
        resolve_required_public_target,
    };

    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("target")
    }

    fn challenge_keyword(value: &str) -> ChallengeKeyword {
        ChallengeKeyword::try_new(value.to_string()).expect("keyword")
    }

    fn spec() -> ChallengeBundleSpec {
        let public: PublicChallengeBundleSpec =
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "challenge_name": "sample-sum",
                "challenge_title": "Sample Sum",
                "summary": {"en": "Sum numbers", "zh": "Sum numbers zh"},
                "keywords": ["arithmetic"],
                "solution": {"protocol": ZIP_PROJECT_PROTOCOL, "manifest_file": "agentics.solution.json"},
                "targets": [{
                    "name": "linux-arm64-cpu",
                    "docker_platform": "linux/arm64",
                    "accelerator": null,
                    "validation_enabled": true,
                    "resource_profile": {
                        "name": "agentics-small",
                        "solution_image": {"source": "local", "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"},
                        "evaluator_image": {"source": "local", "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"},
                        "timeout_sec": 30,
                        "memory_limit_mb": 512,
                        "cpu_limit_millis": 1000,
                        "disk_limit_mb": 1024,
                        "setup_network_access": "disabled",
                        "build_network_access": "disabled",
                        "run_network_access": "disabled",
                        "evaluator_network_access": "disabled"
                    }
                }],
                "starts_at": "2026-01-01T00:00:00Z",
                "eligibility": {"type": "open"},
                "visibility": {
                    "leaderboard": "public_live",
                    "score_distribution": "public_live",
                    "result_detail": "submitter_live_public_live"
                },
                "solution_publication": "private",
                "execution": {
                    "mode": "separated_evaluator",
                    "evaluator": {"command": ["python", "evaluator/run.py"], "result_file": "result.json"}
                },
                "datasets": {
                    "public_dir": "public",
                    "public_policy": "full",
                    "private_benchmark_policy": "score_only",
                    "private_benchmark_enabled": false
                },
                "metric_schema": {
                    "metrics": [{"name": "score", "label": "Score", "direction": "maximize", "visibility": "public"}],
                    "ranking": {"primary_metric_name": "score"}
                }
            }))
            .expect("fixture should deserialize");
        ChallengeBundleSpec {
            schema_version: public.schema_version,
            challenge_name: ChallengeName::try_new("sample-sum".to_string()).expect("name"),
            challenge_title: public.challenge_title,
            summary: LocalizedText {
                en: "Sum numbers".to_string(),
                zh: "Sum numbers zh".to_string(),
            },
            keywords: vec![challenge_keyword("arithmetic")],
            solution: SolutionSpec {
                protocol: ZIP_PROJECT_PROTOCOL.to_string(),
                manifest_file: BundleRelativePath::try_new("agentics.solution.json")
                    .expect("path"),
            },
            targets: public.targets,
            starts_at: "2026-01-01T00:00:00Z".to_string(),
            closes_at: None,
            eligibility: ChallengeEligibilitySpec {
                eligibility_type: ChallengeEligibilityType::Open,
            },
            validation_submission_limit: None,
            official_submission_limit: None,
            visibility: ChallengeVisibilitySpec {
                leaderboard: ChallengeVisibility::PublicLive,
                score_distribution: ChallengeVisibility::PublicLive,
                result_detail:
                    crate::models::challenge::ChallengeResultDetailVisibility::SubmitterLivePublicLive,
            },
            solution_publication: ChallengeSolutionPublicationPolicy::Private,
            execution: ChallengeExecutionSpec::SeparatedEvaluator(SeparatedEvaluatorExecutionSpec {
                evaluator: EvaluatorSpec {
                    command: vec!["python".to_string(), "evaluator/run.py".to_string()],
                    result_file: BundleRelativePath::try_new("result.json").expect("path"),
                },
                validation_runs: None,
                validation_prepare: None,
                official_runs: None,
                official_prepare: None,
            }),
            datasets: DatasetsSpec {
                public_dir: BundleRelativePath::try_new("public").expect("path"),
                private_benchmark_dir: None,
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: false,
            },
            metric_schema: public.metric_schema,
        }
    }

    #[test]
    fn validates_public_pagination() {
        let page = public_pagination(
            None,
            None,
            DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT,
            "challenge list",
        )
        .expect("default page should validate");
        assert_eq!(page.limit, 100);
        assert_eq!(page.offset, 0);
        assert!(bounded_public_limit(Some(0), 100, "items").is_err());
        assert!(public_pagination(Some(1), Some(-1), 100, "items").is_err());
    }

    #[test]
    fn resolves_required_public_target() {
        let spec = spec();
        assert_eq!(
            resolve_required_public_target(&spec, Some("linux-arm64-cpu")).expect("target"),
            target_name("linux-arm64-cpu")
        );
        assert!(resolve_required_public_target(&spec, None).is_err());
        assert!(resolve_required_public_target(&spec, Some("linux-arm64-cuda")).is_err());
    }
}
