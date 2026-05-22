//! Score-distribution builders for public challenge metric views.

use crate::error::ApiResult as Result;
use shared::db::LeaderboardMetricEntry;
use shared::error::ServiceError;
use shared::models::challenge::{ChallengeBundleSpec, MetricVisibility};
use shared::models::evaluation::MetricValue;
use shared::models::ids::ChallengeId;
use shared::models::names::{ChallengeName, MetricName, TargetName};
use shared::models::request::{
    ScoreDistributionBucketDto, ScoreDistributionQuantileDto, ScoreDistributionResponse,
};

/// Build a distribution response from the visible best leaderboard entries in scope.
pub(super) fn build_score_distribution_response(
    challenge_id: ChallengeId,
    challenge_name: ChallengeName,
    target: TargetName,
    metric_name: MetricName,
    spec: &ChallengeBundleSpec,
    entries: Vec<LeaderboardMetricEntry>,
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
        challenge_id,
        challenge_name,
        target,
        metric_name,
        count,
        min,
        max,
        mean,
        quantiles,
        histogram,
    })
}

/// Select the metric value that participates in one distribution.
fn metric_value_from_leaderboard_entry(
    entry: &LeaderboardMetricEntry,
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Option<f64> {
    match metric_name.as_str() {
        "rank_score" | "best_rank_score" => Some(entry.best_rank_score),
        _ if metric_name == &spec.metric_schema.ranking.primary_metric_name => {
            metric_value_by_name(&entry.official_metrics, metric_name)
                .or_else(|| metric_value_by_name(&entry.aggregate_metrics, metric_name))
        }
        _ => None,
    }
}

/// Find one metric by name in an evaluator aggregate metric payload.
fn metric_value_by_name(metrics: &[MetricValue], metric_name: &MetricName) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| &metric.metric_name == metric_name)
        .map(|metric| metric.value)
}

/// Reject distribution requests that would require private aggregate metrics.
fn ensure_metric_is_publicly_distributable(
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    if matches!(metric_name.as_str(), "rank_score" | "best_rank_score") {
        return Ok(());
    }

    if metric_name == &spec.metric_schema.ranking.primary_metric_name
        && spec
            .metric_schema
            .metric(metric_name)
            .is_some_and(|metric| metric.visibility == MetricVisibility::Public)
    {
        return Ok(());
    }

    Err(ServiceError::Forbidden(
        "score distribution is available only for rank_score, best_rank_score, or the public primary ranking metric"
            .to_string(),
    )
    .into())
}

/// Build nearest-rank quantiles used by the public distribution API.
fn build_quantiles(values: &[f64]) -> Result<Vec<ScoreDistributionQuantileDto>> {
    [
        (0.0, 0usize, 4usize),
        (0.25, 1usize, 4usize),
        (0.5, 2usize, 4usize),
        (0.75, 3usize, 4usize),
        (0.9, 9usize, 10usize),
        (1.0, 4usize, 4usize),
    ]
    .into_iter()
    .map(|(quantile, numerator, denominator)| {
        Ok(ScoreDistributionQuantileDto {
            quantile,
            value: nearest_rank_quantile(values, numerator, denominator)?,
        })
    })
    .collect()
}

/// Select one nearest-rank quantile from already-sorted finite values.
fn nearest_rank_quantile(values: &[f64], numerator: usize, denominator: usize) -> Result<f64> {
    let max_index = values.len().saturating_sub(1);
    let rounded_index = max_index
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .and_then(|value| value.checked_div(denominator))
        .ok_or_else(|| ServiceError::Internal("quantile index overflow".to_string()))?
        .min(max_index);
    Ok(values
        .get(rounded_index)
        .copied()
        .ok_or_else(|| ServiceError::Internal("quantile index out of range".to_string()))?)
}

/// Build at most ten histogram buckets for already-sorted finite values.
fn build_histogram(values: &[f64]) -> Result<Vec<ScoreDistributionBucketDto>> {
    let min = values
        .first()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    let max = values
        .last()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    if min == max {
        return Ok(vec![ScoreDistributionBucketDto {
            lower: min,
            upper: max,
            count: i64::try_from(values.len())
                .map_err(|_| ServiceError::Internal("histogram count overflow".to_string()))?,
        }]);
    }

    let bucket_count = values.len().min(10);
    let width = (max - min) / bucket_count as f64;
    let mut counts = vec![0i64; bucket_count];
    for value in values {
        let index = histogram_bucket_index(*value, min, width, bucket_count)?;
        let count = counts
            .get_mut(index)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index invalid".to_string()))?;
        *count = count
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram count overflow".to_string()))?;
    }

    let mut buckets = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        let lower = min + width * index as f64;
        let upper = match index.checked_add(1) {
            Some(next_index) if next_index == bucket_count => max,
            Some(next_index) => min + width * next_index as f64,
            None => {
                return Err(
                    ServiceError::Internal("histogram bucket index overflow".to_string()).into(),
                );
            }
        };
        buckets.push(ScoreDistributionBucketDto {
            lower,
            upper,
            count,
        });
    }
    Ok(buckets)
}

/// Locate the histogram bucket for a value without using unchecked indexing.
fn histogram_bucket_index(value: f64, min: f64, width: f64, bucket_count: usize) -> Result<usize> {
    for index in 0..bucket_count {
        let next_index = index
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index overflow".to_string()))?;
        if next_index == bucket_count {
            return Ok(index);
        }
        let upper = min + width * next_index as f64;
        if value < upper {
            return Ok(index);
        }
    }
    Ok(bucket_count
        .checked_sub(1)
        .ok_or_else(|| ServiceError::Internal("histogram bucket count invalid".to_string()))?)
}

#[cfg(test)]
mod tests {
    use shared::db::LeaderboardMetricEntry;
    use shared::error::ServiceError;
    use shared::models::challenge::{
        ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
        ChallengeExecutionSpec, ChallengeResultDetailVisibility,
        ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
        ChallengeVisibilitySpec, DatasetsSpec, DockerPlatform, EvaluatorSpec,
        EvaluatorStageProfiles, MetricDefinitionSpec, MetricDirection, MetricSchemaSpec,
        MetricVisibility, PrivateBenchmarkPolicy, RankingSpec, ResourceProfileSpec,
        SeparatedEvaluatorExecutionSpec, SolutionSpec, SolutionStageProfiles, StageResourceProfile,
        TargetAccelerator,
    };
    use shared::models::evaluation::{MetricValue, ScoreVisibility};
    use shared::models::ids::ChallengeId;
    use shared::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use shared::models::localization::LocalizedText;
    use shared::models::names::{
        ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, TargetName,
    };
    use shared::models::paths::BundleRelativePath;
    use shared::zip_project::ZipProjectNetworkAccess;

    use super::build_score_distribution_response;

    /// Parse a valid challenge name for a focused score-distribution test.
    fn challenge_name(value: &str) -> ChallengeName {
        ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
    }

    /// Parse a valid challenge id for a focused score-distribution test.
    fn challenge_id(value: &str) -> ChallengeId {
        ChallengeId::try_new(value).expect("test challenge id is valid")
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
                    evaluator: EvaluatorSpec {
                        command: vec!["python".to_string(), "evaluator/run.py".to_string()],
                        result_file: bundle_path("result.json"),
                    },
                    validation_runs: Some(bundle_path("public/runs.json")),
                    validation_prepare: None,
                    official_runs: Some(bundle_path("private-benchmark/runs.json")),
                    official_prepare: None,
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
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(20.0));
        assert_eq!(response.max, Some(50.0));
    }

    /// Verifies rank-score distributions intentionally use comparator values.
    #[test]
    fn rank_score_distribution_uses_comparator_values() {
        let spec = minimized_metric_spec();
        let response = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("rank_score"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(-50.0));
        assert_eq!(response.max, Some(-20.0));
    }

    /// Verifies official-only primary metrics are not distributable through the public endpoint.
    #[test]
    fn primary_metric_distribution_rejects_official_only_metric() {
        let mut spec = minimized_metric_spec();
        spec.metric_schema.metrics[0].visibility = MetricVisibility::Official;

        let error = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0)],
        )
        .expect_err("official-only primary metric should be rejected");
        assert!(matches!(
            error.as_service_error(),
            ServiceError::Forbidden(_)
        ));

        let error = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("official_score"),
            &spec,
            vec![entry(20.0, -20.0)],
        )
        .expect_err("official_score built-in is no longer exposed");
        assert!(matches!(
            error.as_service_error(),
            ServiceError::Forbidden(_)
        ));
    }
}
