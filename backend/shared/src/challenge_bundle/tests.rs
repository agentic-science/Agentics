use std::path::Path;

use crate::models::challenge::{
    ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
    ChallengeExecutionSpec, ChallengePrepareSpec, ChallengeResultDetailVisibility,
    ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
    ChallengeVisibilitySpec, CommunitySpec, DatasetsSpec, DockerPlatform, HardwareProfileSpec,
    MetricDirection, MetricSchemaSpec, MetricVisibility, PrivateBenchmarkPolicy,
    ResourceProfileSpec, ScorerSpec, SolutionSpec, TargetAccelerator,
};
use crate::models::evaluation::ScoreVisibility;
use crate::models::hashes::OciSha256Digest;
use crate::models::names::{ChallengeName, MetricName, ResourceProfileName, TargetName};
use crate::models::paths::BundleRelativePath;
use crate::models::urls::MoltbookSubmoltUrl;
use crate::zip_project::ZipProjectNetworkAccess;

use super::{
    validate_challenge_bundle, validate_challenge_bundle_spec, validate_digest_pinned_images,
};

/// Handles test digest for this module.
fn test_digest() -> OciSha256Digest {
    OciSha256Digest::try_new(format!("sha256:{}", "a".repeat(64)))
        .expect("test OCI digest is valid")
}

/// Handles base spec for this module.
fn base_spec() -> ChallengeBundleSpec {
    ChallengeBundleSpec {
        schema_version: 1,
        challenge_name: challenge_name("sample-sum"),
        challenge_title: "Sample Sum".to_string(),
        challenge_summary: "Add numbers from worker-managed runs.".to_string(),
        solution: SolutionSpec {
            protocol: "zip_project".to_string(),
            manifest_file: bundle_path("agentics.solution.json"),
        },
        scorer: ScorerSpec {
            command: vec!["python".to_string(), "scorer/run.py".to_string()],
            result_file: bundle_path("result.json"),
        },
        targets: vec![ChallengeTargetSpec {
            name: target_name("linux-arm64-cpu"),
            docker_platform: DockerPlatform::LinuxArm64,
            accelerator: TargetAccelerator::Cpu,
            validation_enabled: true,
            resource_profile: ResourceProfileSpec {
                name: resource_profile_name("agentics-cpu-small"),
                resource_description: None,
                solution_image: "agentics-linux-arm64-cpu:ubuntu26.04-local".to_string(),
                solution_image_digest: None,
                scorer_image: "agentics-linux-arm64-cpu:ubuntu26.04-local".to_string(),
                scorer_image_digest: None,
                timeout_sec: 30,
                memory_limit_mb: 512,
                cpu_limit_millis: 1000,
                disk_limit_mb: 1024,
                setup_network_access: ZipProjectNetworkAccess::Enabled,
                build_network_access: ZipProjectNetworkAccess::Disabled,
                run_network_access: ZipProjectNetworkAccess::Disabled,
                scorer_network_access: ZipProjectNetworkAccess::Disabled,
                hardware: None,
            },
        }],
        starts_at: None,
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
        execution: ChallengeExecutionSpec {
            validation_runs: Some(bundle_path("public/runs.json")),
            validation_prepare: None,
            official_runs: Some(bundle_path("private-benchmark/runs.json")),
            official_prepare: None,
        },
        datasets: DatasetsSpec {
            public_dir: bundle_path("public"),
            private_benchmark_dir: Some(bundle_path("private-benchmark")),
            public_policy: ScoreVisibility::Full,
            private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
            private_benchmark_enabled: true,
        },
        community: None,
        metric_schema: MetricSchemaSpec::default(),
    }
}

/// Handles challenge name for this module.
fn challenge_name(value: &str) -> ChallengeName {
    ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
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

/// Handles pin images for this module.
fn pin_images(spec: &mut ChallengeBundleSpec) {
    let digest = test_digest();
    for target in &mut spec.targets {
        let image = format!("agentics-linux-arm64-cpu:ubuntu26.04-local@{digest}");
        target.resource_profile.solution_image = image.clone();
        target.resource_profile.solution_image_digest = Some(digest.clone());
        target.resource_profile.scorer_image = image;
        target.resource_profile.scorer_image_digest = Some(digest.clone());
    }
}

/// Handles use cuda target for this module.
fn use_cuda_target(target: &mut ChallengeTargetSpec, cuda_variant: &str) {
    target.name = target_name("linux-arm64-cuda");
    target.accelerator = TargetAccelerator::Gpu;
    target.resource_profile.hardware = Some(cuda_hardware());
    let image = format!("agentics-linux-arm64-cuda:{cuda_variant}-ubuntu24.04-local");
    target.resource_profile.solution_image = image.clone();
    target.resource_profile.scorer_image = image;
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

/// Verifies that targets are required.
#[test]
fn targets_are_required() {
    let mut spec = base_spec();
    spec.targets.clear();

    let error = validate_challenge_bundle_spec(&spec).expect_err("empty targets should fail");
    assert!(error.to_string().contains("targets"));
}

/// Verifies that target name is independent from docker platform.
#[test]
fn target_name_is_independent_from_docker_platform() {
    let mut spec = base_spec();
    spec.targets[0].name = target_name("main");

    validate_challenge_bundle_spec(&spec)
        .expect("target name should not be coupled to docker platform");
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
    assert!(error.to_string().contains("hardware.kind"));

    spec.targets[0].resource_profile.hardware = Some(cuda_hardware());
    let image = "agentics-linux-arm64-cuda:cu130-ubuntu24.04-local".to_string();
    spec.targets[0].resource_profile.solution_image = image.clone();
    spec.targets[0].resource_profile.scorer_image = image;
    validate_challenge_bundle_spec(&spec).expect("cuda target should validate");
}

/// Verifies that cpu target rejects unsupported image repository.
#[test]
fn cpu_target_rejects_unsupported_image_repository() {
    let mut spec = base_spec();
    spec.targets[0].resource_profile.solution_image = "python:3.12-slim-bookworm".to_string();

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
    let image = "agentics-linux-arm64-cpu:bookworm".to_string();
    spec.targets[0].resource_profile.solution_image = image.clone();
    spec.targets[0].resource_profile.scorer_image = image;

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
    target.resource_profile.hardware = Some(HardwareProfileSpec {
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
    target.resource_profile.hardware = Some(HardwareProfileSpec {
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

/// Verifies that image digest field must match image reference.
#[test]
fn image_digest_field_must_match_image_reference() {
    let mut spec = base_spec();
    pin_images(&mut spec);
    spec.targets[0].resource_profile.solution_image_digest = Some(
        OciSha256Digest::try_new(format!("sha256:{}", "b".repeat(64)))
            .expect("test OCI digest is valid"),
    );

    let error = validate_challenge_bundle_spec(&spec).expect_err("mismatched digest should fail");

    assert!(error.to_string().contains("must match"));
}

/// Verifies that challenge summary is required.
#[test]
fn challenge_summary_is_required() {
    let mut spec = base_spec();
    spec.challenge_summary.clear();

    let error = validate_challenge_bundle_spec(&spec).expect_err("empty summary should fail");
    assert!(error.to_string().contains("challenge_summary"));
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
    spec.execution.validation_runs = None;
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
    spec.execution.validation_runs = None;
    spec.execution.validation_prepare = Some(prepare_spec());

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that official prepare satisfies private benchmark execution.
#[test]
fn official_prepare_satisfies_private_benchmark_execution() {
    let mut spec = base_spec();
    spec.execution.official_runs = None;
    spec.execution.official_prepare = Some(prepare_spec());

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that official prepare may omit private benchmark directory.
#[test]
fn official_prepare_may_omit_private_benchmark_directory() {
    let mut spec = base_spec();
    spec.execution.official_runs = None;
    spec.execution.official_prepare = Some(prepare_spec());
    spec.datasets.private_benchmark_dir = None;

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that prepare and static runs are mutually exclusive per mode.
#[test]
fn prepare_and_static_runs_are_mutually_exclusive_per_mode() {
    let mut spec = base_spec();
    spec.execution.official_prepare = Some(prepare_spec());

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

/// Verifies that community accepts moltbook submolt metadata.
#[test]
fn community_accepts_moltbook_submolt_metadata() {
    let mut spec = base_spec();
    spec.community = Some(CommunitySpec {
        moltbook_submolt_name: Some("agentics-sample-sum".to_string()),
        moltbook_submolt_url: Some(
            MoltbookSubmoltUrl::try_new("https://www.moltbook.com/submolts/agentics-sample-sum")
                .expect("test Moltbook URL is valid"),
        ),
    });

    assert!(validate_challenge_bundle_spec(&spec).is_ok());
}

/// Verifies that community rejects non moltbook url.
#[test]
fn community_rejects_non_moltbook_url() {
    let mut value = serde_json::to_value(base_spec()).expect("base spec should serialize to JSON");
    value["community"] = serde_json::json!({
        "moltbook_submolt_name": "agentics-sample-sum",
        "moltbook_submolt_url": "https://example.com/agentics-sample-sum"
    });

    let error = serde_json::from_value::<ChallengeBundleSpec>(value)
        .expect_err("invalid URL should fail during typed deserialization");
    assert!(error.to_string().contains("moltbook_submolt_url"));
}

/// Verifies that community rejects invalid submolt name.
#[test]
fn community_rejects_invalid_submolt_name() {
    let mut spec = base_spec();
    spec.community = Some(CommunitySpec {
        moltbook_submolt_name: Some("agentics sample sum".to_string()),
        moltbook_submolt_url: None,
    });

    let error = validate_challenge_bundle_spec(&spec).expect_err("invalid name should fail");
    assert!(error.to_string().contains("moltbook_submolt_name"));
}

/// Creates bundle after validating caller inputs.
fn create_bundle(root: &Path, spec: &ChallengeBundleSpec) {
    std::fs::create_dir_all(root.join("scorer")).expect("failed to create scorer dir");
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
    std::fs::write(root.join("scorer/run.py"), "print('ok')\n").expect("failed to write scorer");
}

/// Handles prepare spec for this module.
fn prepare_spec() -> ChallengePrepareSpec {
    ChallengePrepareSpec {
        command: vec!["python".to_string(), "scorer/prepare.py".to_string()],
        result_runs_file: bundle_path("generated/runs.json"),
        network_access: ZipProjectNetworkAccess::Disabled,
        reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
        external_data: Vec::new(),
        cache_key_hint: None,
    }
}

/// Verifies that disabled private benchmark bundle does not require directory.
#[tokio::test]
async fn disabled_private_benchmark_bundle_does_not_require_directory() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-disabled-private-benchmark-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    create_bundle(&root, &spec);

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(result.is_ok());
}

/// Verifies that source backed run inputs must exist under bundle root.
#[tokio::test]
async fn source_backed_run_inputs_must_exist_under_bundle_root() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-source-input-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"public-1","interface":"file_system","input_files":[{"path":"input.txt","source_path":"public/input.txt"}],"output_files":["answer.txt"]}]}"#,
    )
    .expect("failed to write source-backed runs");

    let missing_result = validate_challenge_bundle(&root).await;
    std::fs::write(root.join("public/input.txt"), "payload\n")
        .expect("failed to write source input");
    let present_result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(missing_result.is_err());
    assert!(present_result.is_ok());
}

/// Verifies that enabled private benchmark bundle requires directory.
#[tokio::test]
async fn enabled_private_benchmark_bundle_requires_directory() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-enabled-private-benchmark-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = true;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    create_bundle(&root, &spec);

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(result.is_err());
}
