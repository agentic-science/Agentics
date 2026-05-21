use super::{
    RunnerAttempt, container_name, effective_phase_limits, evaluator_limits, prepare_limits,
    read_limited_result_json,
};
use crate::models::challenge::{ChallengePrepareSpec, ResourceProfileSpec};
use crate::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
use crate::models::names::ResourceProfileName;
use crate::models::paths::{BundleRelativePath, ScriptPath};
use crate::zip_project::{ZipProjectNetworkAccess, ZipProjectPhaseName, ZipProjectResolvedPhase};

/// Verifies that network policy clamps to resource profile.
#[test]
fn network_policy_clamps_to_resource_profile() {
    assert_eq!(
        ZipProjectNetworkAccess::Enabled.clamp_to(ZipProjectNetworkAccess::Disabled),
        ZipProjectNetworkAccess::Disabled
    );
    assert_eq!(
        ZipProjectNetworkAccess::Loopback.docker_network_mode(),
        "none"
    );
}

/// Verifies that solution phase limits come directly from the resource profile.
#[test]
fn solution_phase_limits_come_from_resource_profile() {
    let profile = resource_profile();
    let phase = ZipProjectResolvedPhase {
        name: ZipProjectPhaseName::Run,
        command: ScriptPath::try_new("run.sh").expect("script path"),
    };

    let limits = effective_phase_limits(&profile, &phase);

    assert_eq!(limits.timeout_sec, 42);
    assert_eq!(limits.memory_limit_mb, 2048);
    assert_eq!(limits.cpu_limit_millis, 2500);
    assert_eq!(limits.disk_limit_mb, 4096);
    assert_eq!(limits.network_access, ZipProjectNetworkAccess::Loopback);
}

/// Verifies evaluator and prepare phases use challenge-owned network policy.
#[test]
fn evaluator_and_prepare_limits_use_challenge_owned_policy() {
    let profile = resource_profile();
    let prepare = ChallengePrepareSpec {
        command: vec!["python".to_string(), "prepare.py".to_string()],
        result_runs_file: BundleRelativePath::try_new("prepared/runs.json").expect("runs path"),
        network_access: ZipProjectNetworkAccess::Enabled,
        reproducibility_notes: None,
    };

    let evaluator = evaluator_limits(&profile);
    let prepare_limits = prepare_limits(&profile, &prepare);

    assert_eq!(evaluator.timeout_sec, profile.timeout_sec);
    assert_eq!(evaluator.network_access, ZipProjectNetworkAccess::Disabled);
    assert_eq!(prepare_limits.timeout_sec, profile.timeout_sec);
    assert_eq!(
        prepare_limits.network_access,
        ZipProjectNetworkAccess::Enabled
    );
}

/// Verifies retry attempts use distinct transient container identities.
#[test]
fn retry_attempts_have_distinct_container_names() {
    let first = RunnerAttempt::new("job/1", "worker a", 1);
    let second = RunnerAttempt::new("job/1", "worker a", 2);

    assert_ne!(
        container_name(&first, "run"),
        container_name(&second, "run")
    );
    assert!(container_name(&first, "run").contains("attempt-1"));
    assert!(container_name(&second, "run").contains("attempt-2"));
}

/// Verifies evaluator result reading rejects symlinks instead of following them.
#[cfg(unix)]
#[tokio::test]
async fn result_json_symlink_is_rejected() {
    let temp = std::env::temp_dir().join(format!(
        "agentics-result-json-symlink-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp).expect("tempdir should be created");
    let target = temp.join("target.json");
    let link = temp.join("result.json");
    std::fs::write(&target, "{}").expect("target should be writable");
    std::os::unix::fs::symlink(&target, &link).expect("symlink should be created");

    let error = read_limited_result_json(&link, 1024)
        .await
        .expect_err("symlink result.json must be rejected");

    assert!(error.to_string().contains("not a regular file"));
    drop(std::fs::remove_dir_all(temp));
}

/// Build a resource profile for runner limit tests.
fn resource_profile() -> ResourceProfileSpec {
    let image = ChallengeImageReference::Local {
        reference: LocalAgenticsImageReference::try_new(
            "agentics-linux-arm64-cpu:ubuntu26.04-local",
        )
        .expect("test image"),
    };
    ResourceProfileSpec {
        name: ResourceProfileName::try_new("python-cpu").expect("profile name"),
        resource_description: None,
        solution_image: image.clone(),
        evaluator_image: image,
        timeout_sec: 42,
        memory_limit_mb: 2048,
        cpu_limit_millis: 2500,
        disk_limit_mb: 4096,
        setup_network_access: ZipProjectNetworkAccess::Enabled,
        build_network_access: ZipProjectNetworkAccess::Disabled,
        run_network_access: ZipProjectNetworkAccess::Loopback,
        evaluator_network_access: ZipProjectNetworkAccess::Disabled,
        hardware_metadata: None,
    }
}
