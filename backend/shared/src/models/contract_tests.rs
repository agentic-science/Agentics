use serde::Serialize;
use serde_json::Value;

use super::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, ChallengeEligibilitySpec,
    ChallengeEligibilityType, ChallengeExecutionSpec, ChallengePrepareSpec,
    ChallengeResultDetailVisibility, ChallengeSolutionPublicationPolicy, ChallengeTargetSpec,
    ChallengeVisibility, ChallengeVisibilitySpec, DatasetsSpec, DockerPlatform, EvaluatorSpec,
    HardwareProfileSpec, MetricDefinitionSpec, MetricDirection, MetricSchemaSpec, MetricVisibility,
    PrivateBenchmarkPolicy, RankingSpec, ResourceProfileSpec, SeparatedEvaluatorExecutionSpec,
    SolutionSpec, TargetAccelerator,
};
use super::evaluation::{
    EvaluationDto, EvaluationStatus, MetricValue, RunMetricResult, ScoreVisibility, ScoringMode,
    SolutionSubmissionStatus,
};
use super::hashes::OciSha256Digest;
use super::ids::{AgentId, EvaluationId, SolutionSubmissionId};
use super::images::{ChallengeImageReference, OciRegistryImageReference};
use super::localization::LocalizedText;
use super::names::{
    ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, RunName, TargetName,
};
use super::paths::BundleRelativePath;
use super::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto, SolutionSubmissionResponse,
};
use crate::storage::StorageKey;
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

/// Verifies that challenge detail contract matches frontend fixture.
#[test]
fn challenge_detail_contract_matches_frontend_fixture() -> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(challenge_detail_response(), CHALLENGE_DETAIL_FIXTURE)
}

/// Verifies that public challenge detail omits private benchmark locators.
#[test]
fn challenge_detail_public_projection_omits_private_benchmark_locators() {
    let value = serde_json::to_value(challenge_detail_response())
        .expect("challenge detail response should serialize to JSON");
    let text = value.to_string();

    assert!(!text.contains("private_benchmark_dir"));
    assert!(!text.contains("official_runs"));
    assert!(!text.contains("official_prepare"));
    assert!(!text.contains("private-benchmark"));
    assert_eq!(
        value["spec"]["execution"]["mode"],
        serde_json::json!("separated_evaluator")
    );
    assert_eq!(
        value["spec"]["execution"]["evaluator"]["command"],
        serde_json::json!(["python", "evaluator/run.py"])
    );
    assert_eq!(
        value["spec"]["datasets"]["private_benchmark_enabled"],
        serde_json::json!(true)
    );
}

/// Verifies that official solution submission contract matches frontend fixture.
#[test]
fn official_solution_submission_contract_matches_frontend_fixture()
-> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(
        official_solution_submission_response(),
        SOLUTION_SUBMISSION_OFFICIAL_FIXTURE,
    )
}

/// Verifies that admin capacity contract matches frontend fixture.
#[test]
fn admin_capacity_contract_matches_frontend_fixture() -> Result<(), Box<dyn std::error::Error>> {
    assert_serializes_to_fixture(admin_capacity_response(), ADMIN_CAPACITY_FIXTURE)
}

/// Handles assert serializes to fixture for this module.
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

/// Ensures no explicit nulls before continuing.
fn ensure_no_explicit_nulls(value: &Value, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::Null => {
            if path.ends_with(".accelerator") {
                return Ok(());
            }
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

/// Handles challenge detail response for this module.
fn challenge_detail_response() -> ChallengeDetailResponse {
    ChallengeDetailResponse {
        name: challenge_name("matrix-multiplication"),
        title: "Matrix Multiplication".to_string(),
        summary: LocalizedText::new(
            "Optimize CPU matrix multiplication kernels.",
            "优化 CPU 矩阵乘法内核。",
        ),
        keywords: vec![
            challenge_keyword("linear algebra"),
            challenge_keyword("performance"),
            challenge_keyword("matrix"),
        ],
        spec: ChallengeBundleSpec {
            schema_version: 1,
            challenge_name: challenge_name("matrix-multiplication"),
            challenge_title: "Matrix Multiplication".to_string(),
            summary: LocalizedText::new(
                "Optimize CPU matrix multiplication kernels.",
                "优化 CPU 矩阵乘法内核。",
            ),
            keywords: vec![
                challenge_keyword("linear algebra"),
                challenge_keyword("performance"),
                challenge_keyword("matrix"),
            ],
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
                    name: resource_profile_name("ubuntu-cpu-small"),
                    resource_description: Some(
                        "Small CPU target for local validation.".to_string(),
                    ),
                    solution_image: registry_image(&format!(
                        "ghcr.io/agentics-reifying/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@{}",
                        image_digest("1")
                    )),
                    evaluator_image: registry_image(&format!(
                        "ghcr.io/agentics-reifying/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@{}",
                        image_digest("2")
                    )),
                    timeout_sec: 60,
                    memory_limit_mb: 2048,
                    cpu_limit_millis: 2000,
                    disk_limit_mb: 4096,
                    setup_network_access: ZipProjectNetworkAccess::Enabled,
                    build_network_access: ZipProjectNetworkAccess::Enabled,
                    run_network_access: ZipProjectNetworkAccess::Disabled,
                    evaluator_network_access: ZipProjectNetworkAccess::Disabled,
                    hardware_metadata: Some(HardwareProfileSpec {
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
            execution: ChallengeExecutionSpec::SeparatedEvaluator(
                SeparatedEvaluatorExecutionSpec {
                    evaluator: EvaluatorSpec {
                        command: vec!["python".to_string(), "evaluator/run.py".to_string()],
                        result_file: bundle_path("result.json"),
                    },
                    validation_runs: Some(bundle_path("public/runs.json")),
                    validation_prepare: None,
                    official_runs: None,
                    official_prepare: Some(ChallengePrepareSpec {
                        command: vec!["python".to_string(), "evaluator/prepare.py".to_string()],
                        result_runs_file: bundle_path("generated/runs.json"),
                        network_access: ZipProjectNetworkAccess::Enabled,
                        reproducibility_notes: Some(
                            "Generated from a fixed benchmark seed.".to_string(),
                        ),
                    }),
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
                metrics: vec![
                    MetricDefinitionSpec {
                        name: metric_name("runtime_ms"),
                        label: "Runtime".to_string(),
                        unit: Some("ms".to_string()),
                        direction: MetricDirection::Minimize,
                        visibility: MetricVisibility::Official,
                        metric_description: Some(
                            "Wall-clock runtime across official runs.".to_string(),
                        ),
                    },
                    MetricDefinitionSpec {
                        name: metric_name("accuracy"),
                        label: "Accuracy".to_string(),
                        unit: None,
                        direction: MetricDirection::Maximize,
                        visibility: MetricVisibility::Public,
                        metric_description: None,
                    },
                ],
                ranking: RankingSpec {
                    primary_metric_name: metric_name("runtime_ms"),
                    tie_breaker_metric_names: vec![metric_name("accuracy")],
                },
            },
        }
        .into(),
        statement_markdown:
            "# Matrix Multiplication\n\nWrite a solution that multiplies f32 matrices quickly."
                .to_string(),
    }
}

/// Handles challenge name for this module.
fn challenge_name(value: &str) -> ChallengeName {
    ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
}

/// Build a valid public challenge keyword for contract tests.
fn challenge_keyword(value: &str) -> ChallengeKeyword {
    ChallengeKeyword::try_new(value.to_string()).expect("test challenge keyword is valid")
}

/// Handles target name for this module.
fn target_name(value: &str) -> TargetName {
    TargetName::try_new(value.to_string()).expect("test target is valid")
}

/// Handles metric name for this module.
fn metric_name(value: &str) -> MetricName {
    MetricName::try_new(value.to_string()).expect("test metric name is valid")
}

/// Handles resource profile name for this module.
fn resource_profile_name(value: &str) -> ResourceProfileName {
    ResourceProfileName::try_new(value.to_string()).expect("test resource profile name is valid")
}

/// Handles bundle path for this module.
fn bundle_path(value: &str) -> BundleRelativePath {
    BundleRelativePath::try_new(value).expect("test bundle path is valid")
}

/// Handles run name for this module.
fn run_name(value: &str) -> RunName {
    RunName::try_new(value.to_string()).expect("test run name is valid")
}

/// Handles solution submission id for this module.
fn solution_submission_id(value: &str) -> SolutionSubmissionId {
    SolutionSubmissionId::try_new(value).expect("test submission id is valid")
}

/// Handles agent id for this module.
fn agent_id(value: &str) -> AgentId {
    AgentId::try_new(value).expect("test agent id is valid")
}

/// Handles evaluation id for this module.
fn evaluation_id(value: &str) -> EvaluationId {
    EvaluationId::try_new(value).expect("test evaluation id is valid")
}

/// Handles storage key for this module.
fn storage_key(value: &str) -> StorageKey {
    StorageKey::try_new(value).expect("test storage key is valid")
}

/// Handles official solution submission response for this module.
fn official_solution_submission_response() -> SolutionSubmissionResponse {
    SolutionSubmissionResponse {
        id: solution_submission_id("11111111-1111-4111-8111-111111111111"),
        challenge_name: challenge_name("matrix-multiplication"),
        challenge_title: Some("Matrix Multiplication".to_string()),
        target: target_name("linux-arm64-cpu"),
        agent_id: agent_id("22222222-2222-4222-8222-222222222222"),
        agent_display_name: Some("solver".to_string()),
        status: SolutionSubmissionStatus::Completed,
        note: "Uses blocked tiling.".to_string(),
        explanation: "Blocked matmul implementation.".to_string(),
        parent_solution_submission_id: None,
        credit_text: String::new(),
        visible_after_eval: true,
        artifact_key: Some(storage_key(
            "solution-submissions/11111111-1111-4111-8111-111111111111.zip",
        )),
        evaluation_job: None,
        evaluation: None,
        validation_evaluation: None,
        official_evaluation: Some(EvaluationDto {
            id: evaluation_id("33333333-3333-4333-8333-333333333333"),
            target: target_name("linux-arm64-cpu"),
            status: EvaluationStatus::Completed,
            eval_type: ScoringMode::Official,
            primary_score: Some(0.91),
            rank_score: Some(-42.5),
            aggregate_metrics: vec![
                MetricValue {
                    metric_name: metric_name("runtime_ms"),
                    value: 42.5,
                },
                MetricValue {
                    metric_name: metric_name("accuracy"),
                    value: 1.0,
                },
            ],
            run_metrics: vec![RunMetricResult {
                run_name: run_name("square-100"),
                metrics: vec![MetricValue {
                    metric_name: metric_name("runtime_ms"),
                    value: 17.5,
                }],
            }],
            public_results: vec![],
            validation_summary: None,
            official_summary: None,
            log_key: None,
            started_at: Some("2026-04-28T00:00:00Z".to_string()),
            finished_at: Some("2026-04-28T00:00:42Z".to_string()),
        }),
        created_at: "2026-04-28T00:00:00Z".to_string(),
        updated_at: "2026-04-28T00:00:42Z".to_string(),
    }
}

/// Handles admin capacity response for this module.
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

/// Handles digest for this module.
fn digest(fill: &str) -> String {
    format!("sha256:{}", fill.repeat(64))
}

/// Handles image digest for this module.
fn image_digest(fill: &str) -> OciSha256Digest {
    OciSha256Digest::try_new(digest(fill)).expect("test OCI digest is valid")
}

/// Handles registry image for this module.
fn registry_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Registry {
        reference: OciRegistryImageReference::try_new(value).expect("test registry image is valid"),
    }
}
