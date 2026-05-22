use std::path::Path;

use crate::models::challenge::{
    ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
    ChallengeExecutionSpec, ChallengePrepareSpec, ChallengeResultDetailVisibility,
    ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
    ChallengeVisibilitySpec, CoexecutedBenchmarkExecutionSpec, CoexecutedBenchmarkPrepareSpec,
    DatasetsSpec, DockerPlatform, EvaluatorSpec, EvaluatorStageProfiles, HardwareProfileSpec,
    MetricDirection, MetricSchemaSpec, MetricVisibility, PipedStdioExecutionSpec,
    PipedStdioPrepareSpec, PrivateBenchmarkPolicy, ResourceProfileSpec,
    SeparatedEvaluatorExecutionSpec, SolutionSpec, SolutionStageProfiles, StageResourceProfile,
    TargetAccelerator,
};
use crate::models::evaluation::ScoreVisibility;
use crate::models::hashes::OciSha256Digest;
use crate::models::images::{
    ChallengeImageReference, LocalAgenticsImageReference, OciRegistryImageReference,
};
use crate::models::localization::LocalizedText;
use crate::models::names::{
    ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, TargetName,
};
use crate::models::paths::BundleRelativePath;
use crate::zip_project::ZipProjectNetworkAccess;

use super::{
    validate_challenge_bundle, validate_challenge_bundle_spec, validate_digest_pinned_images,
};

/// Handles test digest for this module.
fn test_digest() -> OciSha256Digest {
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

/// Handles base spec for this module.
fn base_spec() -> ChallengeBundleSpec {
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
            evaluator: EvaluatorSpec {
                command: vec!["python".to_string(), "evaluator/run.py".to_string()],
                result_file: bundle_path("result.json"),
            },
            validation_runs: Some(bundle_path("public/runs.json")),
            validation_prepare: None,
            official_runs: Some(bundle_path("private-benchmark/runs.json")),
            official_prepare: None,
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

/// Handles base piped-stdio spec for this module.
fn base_piped_stdio_spec() -> ChallengeBundleSpec {
    let mut spec = base_spec();
    spec.execution = ChallengeExecutionSpec::PipedStdio(PipedStdioExecutionSpec {
        interactor: EvaluatorSpec {
            command: vec!["python".to_string(), "interactor/run.py".to_string()],
            result_file: bundle_path("result.json"),
        },
        validation_session: Some(bundle_path("public/session.json")),
        validation_prepare: None,
        official_session: Some(bundle_path("private-benchmark/session.json")),
        official_prepare: None,
    });
    spec
}

/// Build a valid co-executed benchmark spec for tests.
fn base_coexecuted_benchmark_spec() -> ChallengeBundleSpec {
    let mut spec = base_spec();
    spec.execution =
        ChallengeExecutionSpec::CoexecutedBenchmark(CoexecutedBenchmarkExecutionSpec {
            benchmark: EvaluatorSpec {
                command: vec!["python".to_string(), "benchmark/run.py".to_string()],
                result_file: bundle_path("result.json"),
            },
            acknowledge_danger: true,
            validation_prepare: Some(coexecuted_prepare_spec()),
            official_prepare: Some(coexecuted_prepare_spec()),
        });
    for target in &mut spec.targets {
        target.resource_profile.solution.run = None;
    }
    spec
}

/// Build a stage resource profile for tests.
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

/// Borrow separated-evaluator execution in tests that start from `base_spec`.
fn separated_evaluator_mut(spec: &mut ChallengeBundleSpec) -> &mut SeparatedEvaluatorExecutionSpec {
    let ChallengeExecutionSpec::SeparatedEvaluator(execution) = &mut spec.execution else {
        panic!("base spec should use separated_evaluator execution");
    };
    execution
}

/// Borrow co-executed benchmark execution in tests that start from `base_coexecuted_benchmark_spec`.
fn coexecuted_benchmark_mut(
    spec: &mut ChallengeBundleSpec,
) -> &mut CoexecutedBenchmarkExecutionSpec {
    let ChallengeExecutionSpec::CoexecutedBenchmark(execution) = &mut spec.execution else {
        panic!("spec should use coexecuted_benchmark execution");
    };
    execution
}

/// Handles challenge name for this module.
fn challenge_name(value: &str) -> ChallengeName {
    ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
}

/// Build a valid public challenge keyword for bundle tests.
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

/// Build a local challenge image reference for tests.
fn local_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Local {
        reference: LocalAgenticsImageReference::try_new(value).expect("test local image is valid"),
    }
}

/// Build a registry challenge image reference for tests.
fn registry_image(value: &str) -> ChallengeImageReference {
    ChallengeImageReference::Registry {
        reference: OciRegistryImageReference::try_new(value).expect("test registry image is valid"),
    }
}

/// Handles pin images for this module.
fn pin_images(spec: &mut ChallengeBundleSpec) {
    let digest = test_digest();
    for target in &mut spec.targets {
        let image =
            format!("ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0@{digest}");
        target.resource_profile.solution_image = registry_image(&image);
        target.resource_profile.evaluator_image = registry_image(&image);
    }
}

/// Handles use cuda target for this module.
fn use_cuda_target(target: &mut ChallengeTargetSpec, cuda_variant: &str) {
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;
    target.resource_profile.hardware_metadata = Some(cuda_hardware());
    let image = format!("agentics-linux-arm64-cuda:{cuda_variant}-ubuntu24.04-local");
    target.resource_profile.solution_image = local_image(&image);
    target.resource_profile.evaluator_image = local_image(&image);
}

/// Handles cuda hardware for this module.
fn cuda_hardware() -> HardwareProfileSpec {
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

/// Verifies that legacy rounds field is rejected.
#[test]
fn legacy_rounds_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["rounds"] = serde_json::json!([
        {
            "id": "main",
            "title": "Main",
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_after_close"
            },
            "solution_publication": "public"
        }
    ]);

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy rounds should be an unknown field");
    assert!(error.to_string().contains("rounds"));
}

/// Verifies that legacy community metadata is rejected.
#[test]
fn legacy_community_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["community"] = serde_json::json!({
        "moltbook_submolt_name": "agentics-sample-sum",
        "moltbook_submolt_url": "https://www.moltbook.com/submolts/agentics-sample-sum"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy community metadata should be an unknown field");
    assert!(error.to_string().contains("community"));
}

/// Verifies that challenge-authored Moltbook metadata remains platform-owned.
#[test]
fn challenge_authored_moltbook_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["moltbook"] = serde_json::json!({
        "discussion_url": "https://www.moltbook.com/post/sample-sum"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("Moltbook metadata should be an unknown field");
    assert!(error.to_string().contains("moltbook"));
}

/// Verifies that legacy top-level scorer contracts are rejected.
#[test]
fn legacy_top_level_scorer_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["scorer"] = serde_json::json!({
        "command": ["python", "scorer/run.py"],
        "result_file": "result.json"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy scorer field should be unknown");

    assert!(error.to_string().contains("scorer"));
}

/// Verifies that evaluator contracts do not silently ignore unknown fields.
#[test]
fn evaluator_unknown_fields_are_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["execution"]["evaluator"]["extra"] = serde_json::json!("ignored");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("unknown evaluator field should fail");

    assert!(error.to_string().contains("extra"));
}

/// Verifies that execution mode is required by the topology tag.
#[test]
fn execution_mode_is_required() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["execution"]
        .as_object_mut()
        .expect("execution should be an object")
        .remove("mode");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("missing execution mode should fail");

    assert!(error.to_string().contains("mode"));
}

/// Verifies that unknown execution modes are rejected.
#[test]
fn unknown_execution_modes_are_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["execution"]["mode"] = serde_json::json!("firecracker_benchmark");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("unknown execution mode should fail");

    assert!(error.to_string().contains("firecracker_benchmark"));
}

/// Verifies that targets are required.
#[test]
fn targets_are_required() {
    let mut spec = base_spec();
    spec.targets.clear();

    let error = validate_challenge_bundle_spec(&spec).expect_err("empty targets should fail");
    assert!(error.to_string().contains("targets"));
}

/// Verifies that challenge catalog keywords are required.
#[test]
fn keywords_are_required() {
    let mut spec = base_spec();
    spec.keywords.clear();

    let error = validate_challenge_bundle_spec(&spec).expect_err("empty keywords should fail");
    assert!(error.to_string().contains("keywords must contain between"));
}

/// Verifies that legacy string image fields are rejected by the source enum contract.
#[test]
fn legacy_string_image_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]["resource_profile"]["solution_image"] =
        serde_json::json!("agentics-linux-arm64-cpu:ubuntu26.04-local");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy image string should fail");

    assert!(
        error.to_string().contains("invalid type") || error.to_string().contains("source"),
        "unexpected error: {error}"
    );
}

/// Verifies that removed external digest fields are rejected by the resource profile contract.
#[test]
fn legacy_image_digest_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]["resource_profile"]["solution_image_digest"] =
        serde_json::json!(test_digest());

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy digest field should fail");

    assert!(error.to_string().contains("solution_image_digest"));
}

/// Verifies old resource profile scorer field names are rejected.
#[test]
fn legacy_scorer_resource_profile_fields_are_rejected() {
    for field in ["scorer_image", "scorer_network_access"] {
        let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
        spec_json["targets"][0]["resource_profile"][field] = if field == "scorer_image" {
            serde_json::json!({
                "source": "local",
                "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
            })
        } else {
            serde_json::json!("disabled")
        };

        let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
            .expect_err("legacy scorer resource field should fail");

        assert!(error.to_string().contains(field));
    }
}

/// Verifies old flat resource profile limits and network fields are rejected.
#[test]
fn legacy_flat_resource_profile_fields_are_rejected() {
    for field in [
        "timeout_sec",
        "memory_limit_mb",
        "cpu_limit_millis",
        "disk_limit_mb",
        "setup_network_access",
        "build_network_access",
        "run_network_access",
        "evaluator_network_access",
    ] {
        let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
        spec_json["targets"][0]["resource_profile"][field] = match field {
            "setup_network_access"
            | "build_network_access"
            | "run_network_access"
            | "evaluator_network_access" => serde_json::json!("disabled"),
            _ => serde_json::json!(30),
        };

        let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
            .expect_err("legacy flat resource profile field should fail");

        assert!(error.to_string().contains(field));
    }
}

/// Verifies all required stage profiles must be declared explicitly.
#[test]
fn missing_stage_profile_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]["resource_profile"]["solution"]
        .as_object_mut()
        .expect("solution profile should be an object")
        .remove("build");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("missing stage profile should fail");

    assert!(error.to_string().contains("build"));
}

/// Verifies stage resource limits must be positive.
#[test]
fn zero_stage_resource_limit_is_rejected() {
    let mut spec = base_spec();
    spec.targets[0]
        .resource_profile
        .solution
        .run
        .as_mut()
        .expect("base spec declares solution run")
        .disk_limit_mb = 0;

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("zero stage resource limit should fail");

    assert!(
        error
            .to_string()
            .contains("targets[0].resource_profile.solution.run.disk_limit_mb")
    );
}

/// Verifies that starts_at is now an explicit required challenge-level policy.
#[test]
fn starts_at_is_required() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json
        .as_object_mut()
        .expect("spec should be an object")
        .remove("starts_at");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("missing starts_at should fail");

    assert!(error.to_string().contains("starts_at"));
}

/// Verifies that invalid starts_at timestamps are rejected.
#[test]
fn starts_at_must_be_rfc3339() {
    let mut spec = base_spec();
    spec.starts_at = "not-a-time".to_string();

    let error = validate_challenge_bundle_spec(&spec).expect_err("invalid starts_at should fail");

    assert!(error.to_string().contains("starts_at"));
}

/// Verifies that no-accelerator targets must use an explicit JSON null.
#[test]
fn accelerator_requires_explicit_null_for_no_accelerator() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]
        .as_object_mut()
        .expect("target should be an object")
        .remove("accelerator");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("missing accelerator should fail");

    assert!(error.to_string().contains("accelerator"));
}

/// Verifies that the old cpu accelerator string is rejected.
#[test]
fn legacy_cpu_accelerator_string_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]["accelerator"] = serde_json::json!("cpu");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy cpu accelerator should fail");

    assert!(error.to_string().contains("cpu"));
}

/// Verifies that old resource_profile.hardware is rejected.
#[test]
fn legacy_hardware_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["targets"][0]["resource_profile"]["hardware"] = serde_json::json!({
        "kind": "cpu"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("legacy hardware field should fail");

    assert!(error.to_string().contains("hardware"));
}

/// Verifies removed prepare metadata fields are rejected.
#[test]
fn removed_prepare_metadata_fields_are_rejected() {
    for field in ["external_data", "cache_key_hint"] {
        let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
        spec_json["execution"]["official_prepare"] = serde_json::json!({
            "command": ["python", "evaluator/prepare.py"],
            "result_runs_file": "generated/runs.json"
        });
        spec_json["execution"]["official_prepare"][field] = if field == "external_data" {
            serde_json::json!([])
        } else {
            serde_json::json!("dataset-v1")
        };

        let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
            .expect_err("removed prepare metadata field should fail");

        assert!(error.to_string().contains(field));
    }
}

/// Verifies removed prepare network fields are rejected.
#[test]
fn removed_prepare_network_access_field_is_rejected() {
    let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
    spec_json["execution"]["official_prepare"] = serde_json::json!({
        "command": ["python", "evaluator/prepare.py"],
        "result_runs_file": "generated/runs.json",
        "network_access": "enabled"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("prepare network access should be stage-owned");

    assert!(error.to_string().contains("network_access"));
}

/// Verifies that hosted challenge target names must use the MVP allowlist.
#[test]
fn target_name_must_use_mvp_allowlist() {
    let mut spec = base_spec();
    spec.targets[0].name = target_name("main");

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("unsupported target names should fail");

    assert!(error.to_string().contains("not supported for MVP"));
}

/// Verifies that amd64 targets are reserved for post mvp.
#[test]
fn amd64_targets_are_reserved_for_post_mvp() {
    let mut spec = base_spec();
    spec.targets[0].name = target_name("linux-amd64-cpu");
    spec.targets[0].docker_platform = DockerPlatform::LinuxAmd64;

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("amd64 targets should be reserved for post-MVP");
    assert!(error.to_string().contains("post-MVP"));
}

/// Verifies that public after close solution publication requires close time.
#[test]
fn public_after_close_solution_publication_requires_close_time() {
    let mut spec = base_spec();
    spec.solution_publication = ChallengeSolutionPublicationPolicy::PublicAfterClose;

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("public-after-close artifacts need a close time");
    assert!(error.to_string().contains("solution_publication"));

    spec.closes_at = Some("2999-01-02T00:00:00Z".to_string());
    validate_challenge_bundle_spec(&spec).expect("close time should satisfy policy");
}

/// Verifies that cuda target requires cuda hardware metadata.
#[test]
fn cuda_target_requires_cuda_hardware_metadata() {
    let mut spec = base_spec();
    let target = &mut spec.targets[0];
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("missing cuda hardware should fail");
    assert!(error.to_string().contains("hardware_metadata.kind"));

    spec.targets[0].resource_profile.hardware_metadata = Some(cuda_hardware());
    let image = "agentics-linux-arm64-cuda:cu130-ubuntu24.04-local";
    spec.targets[0].resource_profile.solution_image = local_image(image);
    spec.targets[0].resource_profile.evaluator_image = local_image(image);
    validate_challenge_bundle_spec(&spec).expect("cuda target should validate");
}

/// Verifies that cpu target rejects unsupported image repository.
#[test]
fn cpu_target_rejects_unsupported_image_repository() {
    let mut spec = base_spec();
    spec.targets[0].resource_profile.solution_image =
        registry_image("ghcr.io/example/not-agentics-linux-arm64-cpu:ubuntu26.04-v0.1.0");

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("unsupported image repository should fail");

    assert!(
        error
            .to_string()
            .contains("supported Agentics image repository")
    );
}

/// Verifies that cpu target rejects unsupported image tag.
#[test]
fn cpu_target_rejects_unsupported_image_tag() {
    let mut spec = base_spec();
    let image = "agentics-linux-arm64-cpu:bookworm";
    spec.targets[0].resource_profile.solution_image = local_image(image);
    spec.targets[0].resource_profile.evaluator_image = local_image(image);

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("unsupported image tag should fail");

    assert!(error.to_string().contains("tag must start with"));
}

/// Verifies that cuda target accepts matching supported image.
#[test]
fn cuda_target_accepts_matching_supported_image() {
    let mut spec = base_spec();
    use_cuda_target(&mut spec.targets[0], "cu130");

    validate_challenge_bundle_spec(&spec).expect("matching cuda image should validate");
}

/// Verifies that cuda target rejects mismatched image variant.
#[test]
fn cuda_target_rejects_mismatched_image_variant() {
    let mut spec = base_spec();
    use_cuda_target(&mut spec.targets[0], "cu132");

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("mismatched cuda image variant should fail");

    assert!(error.to_string().contains("tag must start with `cu130-`"));
}

/// Verifies that cuda target rejects unsupported cuda variant.
#[test]
fn cuda_target_rejects_unsupported_cuda_variant() {
    let mut spec = base_spec();
    let target = &mut spec.targets[0];
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;
    target.resource_profile.hardware_metadata = Some(HardwareProfileSpec {
        cuda_variant: Some("cu129".to_string()),
        cuda_version: Some("12.9".to_string()),
        ..cuda_hardware()
    });

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("unsupported cuda variant should fail");
    assert!(error.to_string().contains("supported variants"));
}

/// Verifies that cuda target rejects mismatched cuda version.
#[test]
fn cuda_target_rejects_mismatched_cuda_version() {
    let mut spec = base_spec();
    let target = &mut spec.targets[0];
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;
    target.resource_profile.hardware_metadata = Some(HardwareProfileSpec {
        cuda_variant: Some("cu132".to_string()),
        cuda_version: Some("13.0".to_string()),
        ..cuda_hardware()
    });

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("mismatched cuda version should fail");
    assert!(error.to_string().contains("cuda_version"));
}

/// Verifies that digest pinned image policy rejects tag only images.
#[test]
fn digest_pinned_image_policy_rejects_tag_only_images() {
    let spec = base_spec();

    let error =
        validate_digest_pinned_images(&spec).expect_err("tag-only images should fail policy");

    assert!(error.to_string().contains("@sha256:<digest>"));
}

/// Verifies that digest pinned image policy accepts immutable references.
#[test]
fn digest_pinned_image_policy_accepts_immutable_references() {
    let mut spec = base_spec();
    pin_images(&mut spec);

    validate_challenge_bundle_spec(&spec).expect("pinned spec should validate");
    validate_digest_pinned_images(&spec).expect("pinned images should satisfy policy");
}

/// Verifies that hosted digest policy rejects local images even when locally valid.
#[test]
fn digest_pinned_image_policy_rejects_local_images() {
    let mut spec = base_spec();
    spec.targets[0].resource_profile.solution_image =
        local_image("agentics-linux-arm64-cpu:ubuntu26.04-local");

    let error =
        validate_digest_pinned_images(&spec).expect_err("local image should fail hosted policy");

    assert!(error.to_string().contains("registry image"));
}

/// Verifies that localized challenge summary is required.
#[test]
fn localized_summary_is_required() {
    let mut spec = base_spec();
    spec.summary.en.clear();

    let error = validate_challenge_bundle_spec(&spec).expect_err("empty summary should fail");
    assert!(error.to_string().contains("summary.en"));
}

/// Verifies that disabled private benchmark may still declare directory.
#[test]
fn disabled_private_benchmark_may_still_declare_directory() {
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that enabled private benchmark requires directory.
#[test]
fn enabled_private_benchmark_requires_directory() {
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = true;
    spec.datasets.private_benchmark_dir = None;

    assert!(validate_challenge_bundle_spec(&spec).is_err());
}

/// Verifies that validation run manifest required only when target enables validation.
#[test]
fn validation_run_manifest_required_only_when_target_enables_validation() {
    let mut spec = base_spec();
    separated_evaluator_mut(&mut spec).validation_runs = None;
    spec.targets[0].validation_enabled = false;

    assert!(validate_challenge_bundle_spec(&spec).is_ok());

    spec.targets[0].validation_enabled = true;
    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("target validation should require run manifest");
    assert!(error.to_string().contains("execution.validation_runs"));
}

/// Verifies that validation prepare satisfies validation enabled target.
#[test]
fn validation_prepare_satisfies_validation_enabled_target() {
    let mut spec = base_spec();
    let execution = separated_evaluator_mut(&mut spec);
    execution.validation_runs = None;
    execution.validation_prepare = Some(prepare_spec());

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that official prepare satisfies private benchmark execution.
#[test]
fn official_prepare_satisfies_private_benchmark_execution() {
    let mut spec = base_spec();
    let execution = separated_evaluator_mut(&mut spec);
    execution.official_runs = None;
    execution.official_prepare = Some(prepare_spec());

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that official prepare may omit private benchmark directory.
#[test]
fn official_prepare_may_omit_private_benchmark_directory() {
    let mut spec = base_spec();
    let execution = separated_evaluator_mut(&mut spec);
    execution.official_runs = None;
    execution.official_prepare = Some(prepare_spec());
    spec.datasets.private_benchmark_dir = None;

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that piped-stdio execution accepts static sessions and public projection hides official data.
#[test]
fn piped_stdio_static_sessions_are_valid_and_projected_publicly() {
    let spec = base_piped_stdio_spec();

    validate_challenge_bundle_spec(&spec).expect("piped stdio spec should validate");
    let public = crate::models::challenge::PublicChallengeBundleSpec::from(spec);
    let execution_json =
        serde_json::to_value(public.execution).expect("public execution serializes");

    assert_eq!(execution_json["mode"], serde_json::json!("piped_stdio"));
    assert_eq!(
        execution_json["validation_session"],
        serde_json::json!("public/session.json")
    );
    assert!(execution_json.get("official_session").is_none());
    assert!(execution_json.get("official_prepare").is_none());
}

/// Verifies that separated-evaluator-only run manifest fields are rejected for piped-stdio.
#[test]
fn piped_stdio_rejects_run_manifest_fields() {
    let mut spec_json = serde_json::to_value(base_piped_stdio_spec()).expect("spec serializes");
    spec_json["execution"]["validation_runs"] = serde_json::json!("public/runs.json");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("piped stdio should reject run manifest fields");

    assert!(error.to_string().contains("validation_runs"));
}

/// Verifies that static and prepared piped-stdio sessions are mutually exclusive.
#[test]
fn piped_stdio_static_and_prepare_sessions_are_mutually_exclusive() {
    let mut spec = base_piped_stdio_spec();
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.validation_prepare = Some(piped_prepare_spec());
    }

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("validation session and prepare should conflict");

    assert!(error.to_string().contains("validation_session"));
}

/// Verifies that piped-stdio validation requires a session source when validation is enabled.
#[test]
fn piped_stdio_validation_requires_session_source() {
    let mut spec = base_piped_stdio_spec();
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.validation_session = None;
    }

    let error =
        validate_challenge_bundle_spec(&spec).expect_err("validation should require a session");

    assert!(error.to_string().contains("validation_session"));
}

/// Verifies that co-executed benchmarks validate and hide official prepare metadata publicly.
#[test]
fn coexecuted_benchmark_is_valid_and_projected_publicly() {
    let spec = base_coexecuted_benchmark_spec();

    validate_challenge_bundle_spec(&spec).expect("co-executed benchmark spec should validate");
    let public = crate::models::challenge::PublicChallengeBundleSpec::from(spec);
    let execution_json =
        serde_json::to_value(public.execution).expect("public execution serializes");

    assert_eq!(
        execution_json["mode"],
        serde_json::json!("coexecuted_benchmark")
    );
    assert_eq!(
        execution_json["acknowledge_danger"],
        serde_json::json!(true)
    );
    assert!(execution_json.get("benchmark").is_some());
    assert!(execution_json.get("validation_prepare").is_some());
    assert!(execution_json.get("official_prepare").is_none());
}

/// Verifies that co-executed benchmarks require explicit danger acknowledgement.
#[test]
fn coexecuted_benchmark_requires_danger_acknowledgement() {
    let mut spec = base_coexecuted_benchmark_spec();
    coexecuted_benchmark_mut(&mut spec).acknowledge_danger = false;

    let error = validate_challenge_bundle_spec(&spec).expect_err("missing danger ack should fail");

    assert!(error.to_string().contains("acknowledge_danger"));
}

/// Verifies that co-executed benchmarks reject solution run-stage limits.
#[test]
fn coexecuted_benchmark_rejects_solution_run_profile() {
    let mut spec = base_coexecuted_benchmark_spec();
    spec.targets[0].resource_profile.solution.run = Some(stage_profile(
        30,
        512,
        1000,
        1024,
        ZipProjectNetworkAccess::Disabled,
    ));

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("co-executed benchmark should reject solution run profile");

    assert!(error.to_string().contains("solution.run"));
    assert!(error.to_string().contains("forbidden"));
}

/// Verifies that separated and piped modes require solution run-stage limits.
#[test]
fn solution_run_profile_is_required_for_modes_with_solution_run_container() {
    let mut separated = base_spec();
    separated.targets[0].resource_profile.solution.run = None;
    let separated_error = validate_challenge_bundle_spec(&separated)
        .expect_err("separated evaluator should require solution run profile");
    assert!(separated_error.to_string().contains("solution.run"));

    let mut piped = base_piped_stdio_spec();
    piped.targets[0].resource_profile.solution.run = None;
    let piped_error = validate_challenge_bundle_spec(&piped)
        .expect_err("piped stdio should require solution run profile");
    assert!(piped_error.to_string().contains("solution.run"));
}

/// Verifies that co-executed benchmarks reject static run and session locators.
#[test]
fn coexecuted_benchmark_rejects_run_and_session_locators() {
    let mut spec_json =
        serde_json::to_value(base_coexecuted_benchmark_spec()).expect("spec serializes");
    spec_json["execution"]["validation_runs"] = serde_json::json!("public/runs.json");
    spec_json["execution"]["validation_session"] = serde_json::json!("public/session.json");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("co-executed benchmark should reject foreign locator fields");

    let message = error.to_string();
    assert!(message.contains("validation_runs") || message.contains("validation_session"));
}

/// Verifies that co-executed prepare does not accept generated result-file locators.
#[test]
fn coexecuted_benchmark_prepare_rejects_result_file_locators() {
    let mut spec_json =
        serde_json::to_value(base_coexecuted_benchmark_spec()).expect("spec serializes");
    spec_json["execution"]["validation_prepare"]["result_runs_file"] =
        serde_json::json!("generated/runs.json");

    let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
        .expect_err("co-executed prepare should reject result-file locators");

    assert!(error.to_string().contains("result_runs_file"));
}

/// Verifies that prepare and static runs are mutually exclusive per mode.
#[test]
fn prepare_and_static_runs_are_mutually_exclusive_per_mode() {
    let mut spec = base_spec();
    separated_evaluator_mut(&mut spec).official_prepare = Some(prepare_spec());

    let error = validate_challenge_bundle_spec(&spec)
        .expect_err("official prepare and official runs should conflict");
    assert!(error.to_string().contains("official_runs"));
}

/// Verifies that metric schema rejects unknown primary metric.
#[test]
fn metric_schema_rejects_unknown_primary_metric() {
    let mut spec = base_spec();
    spec.metric_schema.ranking.primary_metric_name = metric_name("missing");

    assert!(validate_challenge_bundle_spec(&spec).is_err());
}

/// Verifies that metric schema rejects duplicate metric names.
#[test]
fn metric_schema_rejects_duplicate_metric_names() {
    let mut spec = base_spec();
    let mut duplicate = spec.metric_schema.metrics[0].clone();
    duplicate.label = "Duplicate Score".to_string();
    spec.metric_schema.metrics.push(duplicate);

    assert!(validate_challenge_bundle_spec(&spec).is_err());
}

/// Verifies that metric schema accepts tie breaker metadata.
#[test]
fn metric_schema_accepts_tie_breaker_metadata() {
    let mut spec = base_spec();
    spec.metric_schema
        .metrics
        .push(crate::models::challenge::MetricDefinitionSpec {
            name: metric_name("runtime_ms"),
            label: "Runtime".to_string(),
            unit: Some("ms".to_string()),
            direction: MetricDirection::Minimize,
            visibility: MetricVisibility::Public,
            metric_description: Some("Wall-clock runtime in milliseconds.".to_string()),
        });
    spec.metric_schema
        .ranking
        .tie_breaker_metric_names
        .push(metric_name("runtime_ms"));

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Creates bundle after validating caller inputs.
fn create_bundle(root: &Path, spec: &ChallengeBundleSpec) {
    std::fs::create_dir_all(root.join("evaluator")).expect("failed to create evaluator dir");
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
    std::fs::write(root.join("evaluator/run.py"), "print('ok')\n")
        .expect("failed to write evaluator");
}

/// Creates a piped-stdio bundle after validating caller inputs.
fn create_piped_stdio_bundle(root: &Path, spec: &ChallengeBundleSpec) {
    std::fs::create_dir_all(root.join("interactor")).expect("failed to create interactor dir");
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
    std::fs::write(root.join("interactor/run.py"), "print('ok')\n")
        .expect("failed to write interactor");
}

/// Handles prepare spec for this module.
fn prepare_spec() -> ChallengePrepareSpec {
    ChallengePrepareSpec {
        command: vec!["python".to_string(), "evaluator/prepare.py".to_string()],
        result_runs_file: bundle_path("generated/runs.json"),
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

/// Handles piped prepare spec for this module.
fn piped_prepare_spec() -> PipedStdioPrepareSpec {
    PipedStdioPrepareSpec {
        command: vec!["python".to_string(), "interactor/prepare.py".to_string()],
        result_session_file: bundle_path("generated/session.json"),
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

/// Handles co-executed benchmark prepare spec for this module.
fn coexecuted_prepare_spec() -> CoexecutedBenchmarkPrepareSpec {
    CoexecutedBenchmarkPrepareSpec {
        command: vec!["python".to_string(), "benchmark/prepare.py".to_string()],
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
    }
}

/// Verifies that disabled private benchmark bundle does not require directory.
mod bundle_files;
