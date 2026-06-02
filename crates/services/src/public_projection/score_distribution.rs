mod stats;

use agentics_contracts::validation::public_api;
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::ScoreDistributionResponse;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{LeaderboardMetricEntry, Repositories};

use super::metrics::{
    ensure_metric_is_publicly_distributable, metric_value_from_leaderboard_entry,
};
use super::visibility::{ensure_visibility_allows_public, load_challenge_policy};
use stats::{build_histogram, build_quantiles};

const MAX_SCORE_DISTRIBUTION_ROWS: usize = 10_000;
const SCORE_DISTRIBUTION_TRUNCATION_WARNING: &str = "score distribution is limited to the first 10000 leaderboard rows; count, quantiles, histogram, and summary statistics are based on that truncated set";

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: Option<&str>,
    metric_name: MetricName,
) -> Result<ScoreDistributionResponse> {
    let (challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.score_distribution, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let fetch_limit = i64::try_from(MAX_SCORE_DISTRIBUTION_ROWS + 1).map_err(|_| {
        ServiceError::Internal("score distribution fetch limit overflow".to_string())
    })?;
    let mut entries = Repositories::new(pool)
        .leaderboard()
        .list_entries_with_metric_payloads(challenge_name, &target, fetch_limit, &spec)
        .await?;
    let truncated = entries.len() > MAX_SCORE_DISTRIBUTION_ROWS;
    if truncated {
        entries.truncate(MAX_SCORE_DISTRIBUTION_ROWS);
    }
    build_score_distribution_response(
        challenge.challenge_name,
        target,
        metric_name,
        &spec,
        entries,
        truncated,
    )
}

/// Build a distribution response from the visible best leaderboard entries in scope.
pub(super) fn build_score_distribution_response(
    challenge_name: ChallengeName,
    target: TargetName,
    metric_name: MetricName,
    spec: &ChallengeBundleSpec,
    entries: Vec<LeaderboardMetricEntry>,
    truncated: bool,
) -> Result<ScoreDistributionResponse> {
    ensure_metric_is_publicly_distributable(&metric_name, spec)?;
    let mut values = entries
        .iter()
        .filter_map(|entry| metric_value_from_leaderboard_entry(entry, &metric_name, spec))
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let count = i64::try_from(values.len())
        .map_err(|_| ServiceError::Internal("score distribution count overflow".to_string()))?;
    let (min, max, mean, quantiles, histogram) = if values.is_empty() {
        (None, None, None, Vec::new(), Vec::new())
    } else {
        let min = values.first().copied().ok_or_else(|| {
            ServiceError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let max = values.last().copied().ok_or_else(|| {
            ServiceError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let sum: f64 = values.iter().sum();
        let mean = sum / values.len() as f64;
        (
            Some(min),
            Some(max),
            Some(mean),
            build_quantiles(&values)?,
            build_histogram(&values)?,
        )
    };

    Ok(ScoreDistributionResponse {
        challenge_name,
        target,
        metric_name,
        count,
        min,
        max,
        mean,
        quantiles,
        histogram,
        warnings: if truncated {
            vec![SCORE_DISTRIBUTION_TRUNCATION_WARNING.to_string()]
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use agentics_contracts::zip_project::ZipProjectNetworkAccess;
    use agentics_domain::models::challenge::{
        ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
        ChallengeExecutionSpec, ChallengeResultDetailVisibility,
        ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
        ChallengeVisibilitySpec, DatasetsSpec, DockerPlatform, EvaluatorSpec,
        EvaluatorStageProfiles, MetricDefinitionSpec, MetricDirection, MetricSchemaSpec,
        MetricVisibility, PrivateBenchmarkPolicy, RankingSpec, ResourceProfileSpec,
        SeparatedEvaluatorExecutionSpec, SolutionSpec, SolutionStageProfiles, StageResourceProfile,
        TargetAccelerator,
    };
    use agentics_domain::models::evaluation::{MetricValue, ScoreVisibility};
    use agentics_domain::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use agentics_domain::models::localization::LocalizedText;
    use agentics_domain::models::names::{
        ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, TargetName,
    };
    use agentics_domain::models::paths::BundleRelativePath;
    use agentics_error::ServiceError;
    use agentics_persistence::LeaderboardMetricEntry;

    use super::{SCORE_DISTRIBUTION_TRUNCATION_WARNING, build_score_distribution_response};

    /// Parse a valid challenge name for a focused score-distribution test.
    fn challenge_name(value: &str) -> ChallengeName {
        ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
    }

    /// Parse a valid challenge keyword for a focused score-distribution test.
    fn challenge_keyword(value: &str) -> ChallengeKeyword {
        ChallengeKeyword::try_new(value.to_string()).expect("test challenge keyword is valid")
    }

    /// Parse a valid metric name for a focused score-distribution test.
    fn metric_name(value: &str) -> MetricName {
        MetricName::try_new(value.to_string()).expect("test metric name is valid")
    }

    /// Parse a valid target name for a focused score-distribution test.
    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("test target name is valid")
    }

    /// Parse a valid resource profile name for a focused score-distribution test.
    fn resource_profile_name(value: &str) -> ResourceProfileName {
        ResourceProfileName::try_new(value.to_string())
            .expect("test resource profile name is valid")
    }

    /// Parse a bundle-relative path for a focused score-distribution test.
    fn bundle_path(value: &str) -> BundleRelativePath {
        BundleRelativePath::try_new(value).expect("test bundle path is valid")
    }

    /// Build a local Agentics image reference for focused score-distribution tests.
    fn local_image(value: &str) -> ChallengeImageReference {
        ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(value)
                .expect("test local image is valid"),
        }
    }

    /// Build one stage resource profile for focused score-distribution tests.
    fn stage_profile(
        timeout_sec: u64,
        memory_limit_mb: u64,
        cpu_limit_millis: u32,
        disk_limit_mb: u64,
        network_access: ZipProjectNetworkAccess,
    ) -> StageResourceProfile {
        StageResourceProfile {
            timeout_sec,
            memory_limit_mb,
            cpu_limit_millis,
            disk_limit_mb,
            network_access,
        }
    }

    /// Build a minimal challenge contract whose primary metric is minimized.
    fn minimized_metric_spec() -> ChallengeBundleSpec {
        ChallengeBundleSpec {
            schema_version: 1,
            challenge_name: challenge_name("latency-challenge"),
            challenge_title: "Latency Challenge".to_string(),
            summary: LocalizedText::new("Measure raw latency.", "测量原始延迟。"),
            keywords: vec![challenge_keyword("latency")],
            solution: SolutionSpec {
                protocol: "zip_project".to_string(),
                manifest_file: bundle_path("agentics.solution.json"),
            },
            targets: vec![ChallengeTargetSpec {
                name: target_name("linux-arm64-cpu"),
                docker_platform: DockerPlatform::LinuxArm64,
                accelerator: TargetAccelerator::None,
                validation_enabled: true,
                resource_profile: ResourceProfileSpec {
                    name: resource_profile_name("agentics-cpu-small"),
                    resource_description: None,
                    solution_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                    evaluator_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                    solution: SolutionStageProfiles {
                        setup: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Enabled),
                        build: stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        ),
                        run: Some(stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        )),
                    },
                    evaluator: EvaluatorStageProfiles {
                        setup: stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        ),
                        run: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Disabled),
                    },
                    hardware_metadata: None,
                },
            }],
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
                result_detail: ChallengeResultDetailVisibility::SubmitterLivePublicLive,
            },
            solution_publication: ChallengeSolutionPublicationPolicy::Public,
            execution: ChallengeExecutionSpec::SeparatedEvaluator(
                SeparatedEvaluatorExecutionSpec {
                    separated_evaluator: EvaluatorSpec {
                        command: vec![
                            "python".to_string(),
                            "separated-evaluator/run.py".to_string(),
                        ],
                        result_file: bundle_path("result.json"),
                    },
                    validation_runs: Some(bundle_path("public/runs.json")),
                    validation_setup: None,
                    official_runs: Some(bundle_path("private-benchmark/runs.json")),
                    official_evaluation_setup: None,
                },
            ),
            datasets: DatasetsSpec {
                public_dir: bundle_path("public"),
                private_benchmark_dir: Some(bundle_path("private-benchmark")),
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: true,
            },
            metric_schema: MetricSchemaSpec {
                metrics: vec![MetricDefinitionSpec {
                    name: metric_name("latency_ms"),
                    label: "Latency".to_string(),
                    unit: Some("ms".to_string()),
                    direction: MetricDirection::Minimize,
                    visibility: MetricVisibility::Public,
                    metric_description: None,
                }],
                ranking: RankingSpec {
                    primary_metric_name: metric_name("latency_ms"),
                    tie_breaker_metric_names: Vec::new(),
                },
            },
        }
    }

    /// Build one leaderboard entry with distinct primary metric and rank scores.
    fn entry(raw_latency: f64, rank_score: f64) -> LeaderboardMetricEntry {
        LeaderboardMetricEntry {
            best_rank_score: rank_score,
            aggregate_metrics: vec![MetricValue {
                metric_name: metric_name("latency_ms"),
                value: raw_latency,
            }],
            official_metrics: vec![MetricValue {
                metric_name: metric_name("latency_ms"),
                value: raw_latency,
            }],
        }
    }

    /// Verifies primary-metric distributions use raw metric values, not rank values.
    #[test]
    fn primary_metric_distribution_uses_raw_metric_values_for_minimized_metrics() {
        let spec = minimized_metric_spec();
        let response = build_score_distribution_response(
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
            false,
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(20.0));
        assert_eq!(response.max, Some(50.0));
        assert!(response.warnings.is_empty());
    }

    /// Verifies rank-score distributions intentionally use comparator values.
    #[test]
    fn rank_score_distribution_uses_comparator_values() {
        let spec = minimized_metric_spec();
        let response = build_score_distribution_response(
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("rank_score"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
            true,
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(-50.0));
        assert_eq!(response.max, Some(-20.0));
        assert_eq!(
            response.warnings,
            vec![SCORE_DISTRIBUTION_TRUNCATION_WARNING.to_string()]
        );
    }

    /// Verifies official-only primary metrics are not distributable through the public endpoint.
    #[test]
    fn primary_metric_distribution_rejects_official_only_metric() {
        let mut spec = minimized_metric_spec();
        spec.metric_schema.metrics[0].visibility = MetricVisibility::Official;

        let error = build_score_distribution_response(
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0)],
            false,
        )
        .expect_err("official-only primary metric should be rejected");
        assert!(matches!(error, ServiceError::Forbidden(_)));

        let error = build_score_distribution_response(
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("official_score"),
            &spec,
            vec![entry(20.0, -20.0)],
            false,
        )
        .expect_err("official_score built-in is no longer exposed");
        assert!(matches!(error, ServiceError::Forbidden(_)));
    }
}
