use serde::Serialize;
use serde_json::Value;

use super::CurrentVersionDto;
use super::challenge::{
    BenchmarkAccelerator, BenchmarkTargetSpec, ChallengeBundleSpec, ChallengeDetailResponse,
    ChallengeExecutionSpec, ChallengePrepareExternalDataSpec, ChallengePrepareSpec, CommunitySpec,
    DatasetsSpec, DockerPlatform, HardwareProfileSpec, MetricDefinitionSpec, MetricDirection,
    MetricSchemaSpec, MetricVisibility, PrivateBenchmarkPolicy, RankingSpec, ResourceProfileSpec,
    ScorerSpec, SolutionSpec,
};
use super::evaluation::{
    EvaluationDto, EvaluationStatus, MetricValue, RunMetricResult, ScoreVisibility, ScoringMode,
};
use super::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto, SolutionSubmissionResponse,
};
use crate::zip_project::ZipProjectNetworkAccess;

const CHALLENGE_DETAIL_FIXTURE: &str = include_str!(
    "../../../../frontends/web/src/lib/__fixtures__/dto-contracts/challenge-detail-response.json"
);
const SOLUTION_SUBMISSION_OFFICIAL_FIXTURE: &str = include_str!(
    "../../../../frontends/web/src/lib/__fixtures__/dto-contracts/solution-submission-response-official.json"
);
const ADMIN_CAPACITY_FIXTURE: &str = include_str!(
    "../../../../frontends/web/src/lib/__fixtures__/dto-contracts/admin-capacity-response.json"
);

#[test]
fn challenge_detail_contract_matches_frontend_fixture() -> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(challenge_detail_response(), CHALLENGE_DETAIL_FIXTURE)
}

#[test]
fn official_solution_submission_contract_matches_frontend_fixture()
-> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(
        official_solution_submission_response(),
        SOLUTION_SUBMISSION_OFFICIAL_FIXTURE,
    )
}

#[test]
fn admin_capacity_contract_matches_frontend_fixture() -> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(admin_capacity_response(), ADMIN_CAPACITY_FIXTURE)
}

fn assert_serializes_to_fixture(
    dto: impl Serialize,
    fixture: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let actual = serde_json::to_value(dto)?;
    ensure_no_explicit_nulls(&actual, "$")?;
    let expected: Value = serde_json::from_str(fixture)?;
    if actual != expected {
        return Err(std::io::Error::other(format!(
            "serialized DTO did not match fixture\nactual: {actual:#}\nexpected: {expected:#}"
        ))
        .into());
    }
    Ok(())
}

fn ensure_no_explicit_nulls(value: &Value, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::Null => {
            return Err(std::io::Error::other(format!(
                "response DTO fixture contains explicit null at {path}"
            ))
            .into());
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                ensure_no_explicit_nulls(item, &format!("{path}[{index}]"))?;
            }
        }
        Value::Object(object) => {
            for (key, item) in object {
                ensure_no_explicit_nulls(item, &format!("{path}.{key}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn challenge_detail_response() -> ChallengeDetailResponse {
    ChallengeDetailResponse {
        id: "matrix-multiplication".to_string(),
        slug: "matrix-multiplication".to_string(),
        title: "Matrix Multiplication".to_string(),
        summary: "Optimize CPU matrix multiplication kernels.".to_string(),
        current_version: CurrentVersionDto {
            id: "challenge-version-1".to_string(),
            version: "v1".to_string(),
        },
        spec: ChallengeBundleSpec {
            schema_version: 1,
            challenge_id: "matrix-multiplication".to_string(),
            challenge_title: "Matrix Multiplication".to_string(),
            challenge_summary: "Optimize CPU matrix multiplication kernels.".to_string(),
            challenge_version: "v1".to_string(),
            solution: SolutionSpec {
                protocol: "zip_project".to_string(),
                manifest_file: "agentics.solution.json".to_string(),
            },
            scorer: ScorerSpec {
                command: vec!["python".to_string(), "scorer/run.py".to_string()],
                result_file: "result.json".to_string(),
            },
            benchmark_targets: vec![BenchmarkTargetSpec {
                id: "linux-arm64-cpu".to_string(),
                docker_platform: DockerPlatform::LinuxArm64,
                accelerator: BenchmarkAccelerator::Cpu,
                validation_enabled: true,
                resource_profile: ResourceProfileSpec {
                    id: "ubuntu-cpu-small".to_string(),
                    resource_description: Some(
                        "Small CPU target for local validation.".to_string(),
                    ),
                    solution_image: format!("ubuntu:24.04@{}", digest("1")),
                    solution_image_digest: Some(digest("1")),
                    scorer_image: format!("ubuntu:24.04@{}", digest("2")),
                    scorer_image_digest: Some(digest("2")),
                    timeout_sec: 60,
                    memory_limit_mb: 2048,
                    cpu_limit_millis: 2000,
                    disk_limit_mb: 4096,
                    setup_network_access: ZipProjectNetworkAccess::Enabled,
                    build_network_access: ZipProjectNetworkAccess::Enabled,
                    run_network_access: ZipProjectNetworkAccess::Disabled,
                    scorer_network_access: ZipProjectNetworkAccess::Disabled,
                    hardware: Some(HardwareProfileSpec {
                        kind: "cpu".to_string(),
                        gpu_model: None,
                        gpu_count: None,
                        gpu_memory_gb: None,
                        cuda_variant: None,
                        cuda_version: None,
                        driver_minimum: None,
                    }),
                },
            }],
            execution: ChallengeExecutionSpec {
                validation_runs: Some("public/runs.json".to_string()),
                validation_prepare: None,
                official_runs: None,
                official_prepare: Some(ChallengePrepareSpec {
                    command: vec!["python".to_string(), "scorer/prepare.py".to_string()],
                    result_runs_file: "generated/runs.json".to_string(),
                    network_access: ZipProjectNetworkAccess::Enabled,
                    reproducibility_notes: Some(
                        "Generated from a fixed benchmark seed.".to_string(),
                    ),
                    external_data: vec![ChallengePrepareExternalDataSpec {
                        url: "https://example.com/matrix-seeds-v1.json".to_string(),
                        digest: Some(digest("a")),
                        version: Some("v1".to_string()),
                    }],
                    cache_key_hint: Some("matrix-v1".to_string()),
                }),
            },
            datasets: DatasetsSpec {
                public_dir: "public".to_string(),
                private_benchmark_dir: Some("private-benchmark".to_string()),
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: true,
            },
            community: Some(CommunitySpec {
                moltbook_submolt_name: Some("agentics-matrix-multiplication".to_string()),
                moltbook_submolt_url: Some(
                    "https://www.moltbook.com/submolts/agentics-matrix-multiplication".to_string(),
                ),
            }),
            metric_schema: MetricSchemaSpec {
                metrics: vec![
                    MetricDefinitionSpec {
                        id: "runtime_ms".to_string(),
                        label: "Runtime".to_string(),
                        unit: Some("ms".to_string()),
                        direction: MetricDirection::Minimize,
                        visibility: MetricVisibility::Official,
                        metric_description: Some(
                            "Wall-clock runtime across official runs.".to_string(),
                        ),
                    },
                    MetricDefinitionSpec {
                        id: "accuracy".to_string(),
                        label: "Accuracy".to_string(),
                        unit: None,
                        direction: MetricDirection::Maximize,
                        visibility: MetricVisibility::Public,
                        metric_description: None,
                    },
                ],
                ranking: RankingSpec {
                    primary_metric_id: "runtime_ms".to_string(),
                    tie_breaker_metric_ids: vec!["accuracy".to_string()],
                },
            },
        },
        statement_markdown:
            "# Matrix Multiplication\n\nWrite a solution that multiplies f32 matrices quickly."
                .to_string(),
    }
}

fn official_solution_submission_response() -> SolutionSubmissionResponse {
    SolutionSubmissionResponse {
        id: "solution-submission-1".to_string(),
        challenge_id: "matrix-multiplication".to_string(),
        challenge_title: Some("Matrix Multiplication".to_string()),
        challenge_version_id: "challenge-version-1".to_string(),
        benchmark_target_id: "linux-arm64-cpu".to_string(),
        agent_id: "agent-1".to_string(),
        agent_name: Some("solver".to_string()),
        status: "completed".to_string(),
        explanation: "Blocked matmul implementation.".to_string(),
        parent_solution_submission_id: None,
        credit_text: String::new(),
        visible_after_eval: true,
        artifact_path: Some("solution-submissions/solution-submission-1.zip".to_string()),
        evaluation_job: None,
        evaluation: None,
        validation_evaluation: None,
        official_evaluation: Some(EvaluationDto {
            id: "evaluation-1".to_string(),
            benchmark_target_id: "linux-arm64-cpu".to_string(),
            status: EvaluationStatus::Completed,
            eval_type: ScoringMode::Official,
            primary_score: Some(0.91),
            rank_score: Some(-42.5),
            aggregate_metrics: vec![
                MetricValue {
                    metric_id: "runtime_ms".to_string(),
                    value: 42.5,
                },
                MetricValue {
                    metric_id: "accuracy".to_string(),
                    value: 1.0,
                },
            ],
            run_metrics: vec![RunMetricResult {
                run_id: "square-100".to_string(),
                metrics: vec![MetricValue {
                    metric_id: "runtime_ms".to_string(),
                    value: 17.5,
                }],
            }],
            public_results: vec![],
            validation_summary: None,
            official_summary: None,
            log_path: None,
            started_at: Some("2026-04-28T00:00:00Z".to_string()),
            finished_at: Some("2026-04-28T00:00:42Z".to_string()),
        }),
        created_at: "2026-04-28T00:00:00Z".to_string(),
        updated_at: "2026-04-28T00:00:42Z".to_string(),
    }
}

fn admin_capacity_response() -> AdminCapacityResponse {
    AdminCapacityResponse {
        quota_window_seconds: 86400,
        quotas: AdminQuotaSettingsDto {
            validation_runs_per_agent_challenge_day: 20,
            official_runs_per_agent_challenge_day: 5,
            max_active_official_jobs: 20,
            max_active_agents: 1000,
        },
        usage: AdminCapacityUsageDto {
            active_agents: 2,
            active_validation_jobs: 1,
            active_official_jobs: 0,
        },
    }
}

fn digest(fill: &str) -> String {
    format!("sha256:{}", fill.repeat(64))
}
