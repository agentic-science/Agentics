use shared::models::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, ChallengeEligibilitySpec,
    ChallengeEligibilityType, ChallengeExecutionSpec, ChallengeListItemDto, ChallengeListResponse,
    ChallengeResultDetailVisibility, ChallengeSolutionPublicationPolicy, ChallengeTargetSpec,
    ChallengeVisibility, ChallengeVisibilitySpec, CoexecutedBenchmarkExecutionSpec, DatasetsSpec,
    DockerPlatform, EvaluatorSpec, EvaluatorStageProfiles, MetricSchemaSpec,
    PrivateBenchmarkPolicy, ResourceProfileSpec, SeparatedEvaluatorExecutionSpec, SolutionSpec,
    SolutionStageProfiles, StageResourceProfile, TargetAccelerator,
};
use shared::models::evaluation::ScoreVisibility;
use shared::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
use shared::models::localization::LocalizedText;
use shared::models::names::{ChallengeKeyword, ChallengeName, ResourceProfileName, TargetName};
use shared::models::paths::BundleRelativePath;
use shared::zip_project::ZipProjectNetworkAccess;

use super::{OutputFormat, render_challenge_detail, render_challenge_list};

/// Build a local Agentics image reference for CLI rendering tests.
fn local_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Local {
        reference: LocalAgenticsImageReference::try_new(value).expect("test local image is valid"),
    }
}

/// Verifies that renders challenge list table.
#[test]
fn renders_challenge_list_table() {
    let output = render_challenge_list(
        &ChallengeListResponse {
            items: vec![ChallengeListItemDto {
                name: challenge_name("sample-sum"),
                title: "Sample Sum".to_string(),
                summary: localized_summary(),
                keywords: vec![challenge_keyword("arithmetic")],
                starts_at: "2026-01-01T00:00:00Z".to_string(),
                closes_at: None,
                eligibility: ChallengeEligibilitySpec {
                    eligibility_type: ChallengeEligibilityType::Open,
                },
            }],
            total_count: 1,
            limit: 100,
            offset: 0,
            has_more: false,
        },
        OutputFormat::Table,
    )
    .expect("render should succeed");

    assert_eq!(
        output,
        "NAME        ELIGIBILITY  KEYWORDS    TITLE\nsample-sum  open         arithmetic  Sample Sum"
    );
}

/// Verifies that renders challenge detail table.
#[test]
fn renders_challenge_detail_table() {
    let output = render_challenge_detail(&challenge_detail(), OutputFormat::Table)
        .expect("render should succeed");

    assert!(output.contains("Sample Sum (sample-sum)"));
    assert!(output.contains("eligibility: open"));
    assert!(output.contains("solution_publication: public"));
    assert!(
        output.contains(
                "  - linux-arm64-cpu: linux/arm64 none, profile=python-cpu-small, solution_image=agentics-linux-arm64-cpu:ubuntu26.04-local, evaluator_image=agentics-linux-arm64-cpu:ubuntu26.04-evaluator-local, solution_run=30 sec/512 MB, evaluator_run=30 sec/512 MB, validation=disabled"
            )
    );
    assert!(output.contains("execution_mode: separated_evaluator"));
    assert!(output.contains("evaluator: command=python evaluator/run.py, result_file=result.json"));
    assert!(output.contains("ranking_metric: score"));
    assert!(output.ends_with("# Statement\n\nReturn the sum."));
}

/// Verifies that renders co-executed challenge detail table with the trust boundary.
#[test]
fn renders_coexecuted_challenge_detail_table() {
    let mut detail = challenge_detail();
    detail.spec.execution =
        ChallengeExecutionSpec::CoexecutedBenchmark(CoexecutedBenchmarkExecutionSpec {
            benchmark: EvaluatorSpec {
                command: vec!["python".to_string(), "benchmark/run.py".to_string()],
                result_file: bundle_path("result.json"),
            },
            acknowledge_danger: true,
            validation_prepare: None,
            official_prepare: None,
        })
        .into();
    detail.spec.targets[0].resource_profile.solution.run = None;

    let output =
        render_challenge_detail(&detail, OutputFormat::Table).expect("render should succeed");

    assert!(output.contains("execution_mode: coexecuted_benchmark"));
    assert!(output.contains("benchmark: command=python benchmark/run.py, result_file=result.json"));
    assert!(output.contains("trust_boundary: benchmark harness and participant workspace"));
}

/// Verifies that impossible co-executed DTOs fail before rendering.
#[test]
fn rejects_impossible_coexecuted_challenge_detail_table() {
    let mut detail = challenge_detail();
    detail.spec.execution =
        ChallengeExecutionSpec::CoexecutedBenchmark(CoexecutedBenchmarkExecutionSpec {
            benchmark: EvaluatorSpec {
                command: vec!["python".to_string(), "benchmark/run.py".to_string()],
                result_file: bundle_path("result.json"),
            },
            acknowledge_danger: true,
            validation_prepare: None,
            official_prepare: None,
        })
        .into();

    assert!(render_challenge_detail(&detail, OutputFormat::Table).is_err());
}

/// Handles challenge detail for this module.
fn challenge_detail() -> ChallengeDetailResponse {
    ChallengeDetailResponse {
        name: challenge_name("sample-sum"),
        title: "Sample Sum".to_string(),
        summary: localized_summary(),
        keywords: vec![challenge_keyword("arithmetic")],
        spec: ChallengeBundleSpec {
            schema_version: 1,
            challenge_name: challenge_name("sample-sum"),
            challenge_title: "Sample Sum".to_string(),
            summary: localized_summary(),
            keywords: vec![challenge_keyword("arithmetic")],
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
                validation_enabled: false,
                resource_profile: ResourceProfileSpec {
                    name: resource_profile_name("python-cpu-small"),
                    resource_description: None,
                    solution_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                    evaluator_image: local_image(
                        "agentics-linux-arm64-cpu:ubuntu26.04-evaluator-local",
                    ),
                    solution: SolutionStageProfiles {
                        setup: stage_profile(
                            60,
                            1024,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Enabled,
                        ),
                        build: stage_profile(
                            60,
                            1024,
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
                            60,
                            1024,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Enabled,
                        ),
                        run: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Disabled),
                    },
                    hardware_metadata: None,
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
                    official_runs: Some(bundle_path("private-benchmark/runs.json")),
                    official_prepare: None,
                },
            ),
            datasets: DatasetsSpec {
                public_dir: bundle_path("data/public"),
                private_benchmark_dir: None,
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: false,
            },
            metric_schema: MetricSchemaSpec::default(),
        }
        .into(),
        statement_markdown: "# Statement\n\nReturn the sum.".to_string(),
    }
}

/// Handles target name for this module.
fn target_name(value: &str) -> TargetName {
    TargetName::try_new(value.to_string()).expect("test target is valid")
}

/// Build the standard localized challenge summary for output tests.
fn localized_summary() -> LocalizedText {
    LocalizedText::new("Add numbers", "数字求和")
}

/// Handles challenge name for this module.
fn challenge_name(value: &str) -> ChallengeName {
    ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
}

/// Build a valid public challenge keyword for output tests.
fn challenge_keyword(value: &str) -> ChallengeKeyword {
    ChallengeKeyword::try_new(value.to_string()).expect("test challenge keyword is valid")
}

/// Handles resource profile name for this module.
fn resource_profile_name(value: &str) -> ResourceProfileName {
    ResourceProfileName::try_new(value.to_string()).expect("test resource profile name is valid")
}

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

/// Handles bundle path for this module.
fn bundle_path(value: &str) -> BundleRelativePath {
    BundleRelativePath::try_new(value).expect("test bundle path is valid")
}
