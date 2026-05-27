use std::path::Path;

use crate::zip_project::ZipProjectNetworkAccess;
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
    ChallengeExecutionSpec, ChallengeResultDetailVisibility, ChallengeSetupSpec,
    ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
    ChallengeVisibilitySpec, CoexecutedBenchmarkExecutionSpec, CoexecutedBenchmarkSetupSpec,
    DatasetsSpec, DockerPlatform, EvaluatorSpec, EvaluatorStageProfiles, HardwareProfileSpec,
    MetricSchemaSpec, PipedStdioExecutionSpec, PipedStdioSetupSpec, PrivateBenchmarkPolicy,
    ResourceProfileSpec, SeparatedEvaluatorExecutionSpec, SolutionSpec, SolutionStageProfiles,
    StageResourceProfile, TargetAccelerator,
};
use agentics_domain::models::evaluation::ScoreVisibility;
use agentics_domain::models::hashes::OciSha256Digest;
use agentics_domain::models::images::{
    ChallengeImageReference, LocalAgenticsImageReference, OciRegistryImageReference,
};
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::names::{
    ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, TargetName,
};
use agentics_domain::models::paths::BundleRelativePath;

/// Handles test digest for this module.
pub(super) fn test_digest() -> OciSha256Digest {
    OciSha256Digest::try_new(format!("sha256:{}", "a".repeat(64)))
        .expect("test OCI digest is valid")
}

/// Build the standard localized challenge summary for bundle tests.
fn localized_summary() -> LocalizedText {
    LocalizedText::new(
        "Add numbers from worker-managed runs.",
        "在 worker 管理的运行中完成数字求和。",
    )
}

/// Build the base separated-evaluator spec for bundle tests.
pub(super) fn base_spec() -> ChallengeBundleSpec {
    ChallengeBundleSpec {
        schema_version: 1,
        challenge_name: challenge_name("sample-sum"),
        challenge_title: "Sample Sum".to_string(),
        summary: localized_summary(),
        keywords: vec![challenge_keyword("arithmetic")],
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
                    build: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Disabled),
                    run: Some(stage_profile(
                        30,
                        512,
                        1000,
                        1024,
                        ZipProjectNetworkAccess::Disabled,
                    )),
                },
                evaluator: EvaluatorStageProfiles {
                    setup: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Disabled),
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
            result_detail: ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose,
        },
        solution_publication: ChallengeSolutionPublicationPolicy::Public,
        execution: ChallengeExecutionSpec::SeparatedEvaluator(SeparatedEvaluatorExecutionSpec {
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
        }),
        datasets: DatasetsSpec {
            public_dir: bundle_path("public"),
            private_benchmark_dir: Some(bundle_path("private-benchmark")),
            public_policy: ScoreVisibility::Full,
            private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
            private_benchmark_enabled: true,
        },
        metric_schema: MetricSchemaSpec::default(),
    }
}

/// Build a base piped-stdio spec for bundle tests.
pub(super) fn base_piped_stdio_spec() -> ChallengeBundleSpec {
    let mut spec = base_spec();
    spec.execution = ChallengeExecutionSpec::PipedStdio(PipedStdioExecutionSpec {
        interactive_evaluator: EvaluatorSpec {
            command: vec![
                "python".to_string(),
                "interactive-evaluator/run.py".to_string(),
            ],
            result_file: bundle_path("result.json"),
        },
        acknowledge_stdio_protocol_framing: true,
        validation_session: Some(bundle_path("public/session.json")),
        validation_setup: None,
        official_session: Some(bundle_path("private-benchmark/session.json")),
        official_evaluation_setup: None,
    });
    spec
}

/// Build a valid coexecuted-evaluator spec for tests.
pub(super) fn base_coexecuted_benchmark_spec() -> ChallengeBundleSpec {
    let mut spec = base_spec();
    spec.execution =
        ChallengeExecutionSpec::CoexecutedBenchmark(CoexecutedBenchmarkExecutionSpec {
            coexecuted_evaluator: EvaluatorSpec {
                command: vec![
                    "python".to_string(),
                    "coexecuted-evaluator/run.py".to_string(),
                ],
                result_file: bundle_path("result.json"),
            },
            acknowledge_danger: true,
            validation_setup: Some(coexecuted_setup_spec()),
            official_evaluation_setup: Some(coexecuted_setup_spec()),
        });
    for target in &mut spec.targets {
        target.resource_profile.solution.run = None;
    }
    spec
}

/// Build a stage resource profile for tests.
pub(super) fn stage_profile(
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

/// Borrow separated-evaluator execution in tests that start from `base_spec`.
pub(super) fn separated_evaluator_mut(
    spec: &mut ChallengeBundleSpec,
) -> &mut SeparatedEvaluatorExecutionSpec {
    let ChallengeExecutionSpec::SeparatedEvaluator(execution) = &mut spec.execution else {
        panic!("base spec should use separated_evaluator execution");
    };
    execution
}

/// Borrow coexecuted-evaluator execution in tests that start from `base_coexecuted_benchmark_spec`.
pub(super) fn coexecuted_benchmark_mut(
    spec: &mut ChallengeBundleSpec,
) -> &mut CoexecutedBenchmarkExecutionSpec {
    let ChallengeExecutionSpec::CoexecutedBenchmark(execution) = &mut spec.execution else {
        panic!("spec should use coexecuted_benchmark execution");
    };
    execution
}

/// Handles challenge name for this module.
pub(super) fn challenge_name(value: &str) -> ChallengeName {
    ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
}

/// Handles metric name for this module.
pub(super) fn metric_name(value: &str) -> MetricName {
    MetricName::try_new(value.to_string()).expect("test metric name is valid")
}

/// Handles target name for this module.
pub(super) fn target_name(value: &str) -> TargetName {
    TargetName::try_new(value.to_string()).expect("test target is valid")
}

/// Handles bundle path for this module.
pub(super) fn bundle_path(value: &str) -> BundleRelativePath {
    BundleRelativePath::try_new(value).expect("test bundle path is valid")
}

/// Build a registry challenge image reference for tests.
pub(super) fn registry_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Registry {
        reference: OciRegistryImageReference::try_new(value).expect("test registry image is valid"),
    }
}

/// Handles pin images for this module.
pub(super) fn pin_images(spec: &mut ChallengeBundleSpec) {
    let digest = test_digest();
    for target in &mut spec.targets {
        let image =
            format!("ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@{digest}");
        target.resource_profile.solution_image = registry_image(&image);
        target.resource_profile.evaluator_image = registry_image(&image);
    }
}

/// Handles use cuda target for this module.
pub(super) fn use_cuda_target(target: &mut ChallengeTargetSpec, cuda_variant: &str) {
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;
    target.resource_profile.hardware_metadata = Some(cuda_hardware());
    let image = format!("agentics-linux-arm64-cuda:{cuda_variant}-ubuntu24.04-local");
    target.resource_profile.solution_image = local_image(&image);
    target.resource_profile.evaluator_image = local_image(&image);
}

/// Creates bundle after validating caller inputs.
pub(super) fn create_bundle(root: &Path, spec: &ChallengeBundleSpec) {
    std::fs::create_dir_all(root.join("separated-evaluator"))
        .expect("failed to create separated-evaluator dir");
    std::fs::create_dir_all(root.join("public")).expect("failed to create public dir");
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"public-1","interface":"stdio","stdin_text":"1"}]}"#,
    )
    .expect("failed to write public runs");
    std::fs::write(
        root.join("spec.json"),
        serde_json::to_string(spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
    std::fs::write(root.join("statement.md"), "# Sample\n\nBody\n")
        .expect("failed to write statement");
    std::fs::write(root.join("separated-evaluator/run.py"), "print('ok')\n")
        .expect("failed to write separated-evaluator");
}

/// Creates a piped-stdio bundle after validating caller inputs.
pub(super) fn create_piped_stdio_bundle(root: &Path, spec: &ChallengeBundleSpec) {
    std::fs::create_dir_all(root.join("interactive-evaluator"))
        .expect("failed to create interactive-evaluator dir");
    std::fs::create_dir_all(root.join("public")).expect("failed to create public dir");
    std::fs::create_dir_all(root.join("private-benchmark"))
        .expect("failed to create private benchmark dir");
    std::fs::write(
        root.join("public/session.json"),
        r#"{"session_name":"public-1","input_files":[{"path":"prompt.txt","source_path":"public/prompt.txt"}],"metadata":{"kind":"sample"}}"#,
    )
    .expect("failed to write public session");
    std::fs::write(root.join("public/prompt.txt"), "payload\n")
        .expect("failed to write session input");
    std::fs::write(
        root.join("private-benchmark/session.json"),
        r#"{"session_name":"official-1"}"#,
    )
    .expect("failed to write official session");
    std::fs::write(
        root.join("spec.json"),
        serde_json::to_string(spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
    std::fs::write(root.join("statement.md"), "# Sample\n\nBody\n")
        .expect("failed to write statement");
    std::fs::write(root.join("interactive-evaluator/run.py"), "print('ok')\n")
        .expect("failed to write interactive-evaluator");
}

/// Handles setup spec for this module.
pub(super) fn setup_spec() -> ChallengeSetupSpec {
    ChallengeSetupSpec {
        command: vec![
            "python".to_string(),
            "separated-evaluator/setup.py".to_string(),
        ],
        result_runs_file: bundle_path("generated/runs.json"),
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

/// Handles piped setup spec for this module.
pub(super) fn piped_setup_spec() -> PipedStdioSetupSpec {
    PipedStdioSetupSpec {
        command: vec![
            "python".to_string(),
            "interactive-evaluator/setup.py".to_string(),
        ],
        result_session_file: bundle_path("generated/session.json"),
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

/// Handles coexecuted-evaluator setup spec for this module.
pub(super) fn coexecuted_setup_spec() -> CoexecutedBenchmarkSetupSpec {
    CoexecutedBenchmarkSetupSpec {
        command: vec![
            "python".to_string(),
            "coexecuted-evaluator/setup.py".to_string(),
        ],
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

fn challenge_keyword(value: &str) -> ChallengeKeyword {
    ChallengeKeyword::try_new(value.to_string()).expect("test challenge keyword is valid")
}

fn resource_profile_name(value: &str) -> ResourceProfileName {
    ResourceProfileName::try_new(value.to_string()).expect("test resource profile name is valid")
}

pub(super) fn local_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Local {
        reference: LocalAgenticsImageReference::try_new(value).expect("test local image is valid"),
    }
}

pub(super) fn cuda_hardware() -> HardwareProfileSpec {
    HardwareProfileSpec {
        kind: "cuda".to_string(),
        gpu_model: Some("NVIDIA GB10".to_string()),
        gpu_count: Some(1),
        gpu_memory_gb: Some(128),
        cuda_variant: Some("cu130".to_string()),
        cuda_version: Some("13.0".to_string()),
        driver_minimum: Some(">=580".to_string()),
    }
}
