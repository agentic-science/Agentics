use super::{
    RunnerAttempt, container_name, effective_phase_limits, evaluator_limits, prepare_limits,
    read_limited_result_json,
};
use crate::models::challenge::{
    EvaluatorStageProfiles, ResourceProfileSpec, SolutionStageProfiles, StageResourceProfile,
};
use crate::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
use crate::models::names::ResourceProfileName;
use crate::models::paths::ScriptPath;
use crate::zip_project::{
    DockerNetworkMode, ZipProjectNetworkAccess, ZipProjectPhaseName, ZipProjectResolvedPhase,
};

/// Verifies that network policy clamps to resource profile.
#[test]
fn network_policy_clamps_to_resource_profile() {
    assert_eq!(
        ZipProjectNetworkAccess::Enabled.clamp_to(ZipProjectNetworkAccess::Disabled),
        ZipProjectNetworkAccess::Disabled
    );
    assert_eq!(
        ZipProjectNetworkAccess::Loopback.docker_network_mode(),
        DockerNetworkMode::None
    );
}

/// Verifies that solution phase limits come directly from the resource profile.
#[test]
fn solution_phase_limits_come_from_resource_profile() {
    let profile = resource_profile();

    let setup = effective_phase_limits(&profile, &resolved_phase(ZipProjectPhaseName::Setup))
        .expect("setup limits should resolve");
    let build = effective_phase_limits(&profile, &resolved_phase(ZipProjectPhaseName::Build))
        .expect("build limits should resolve");
    let run = effective_phase_limits(&profile, &resolved_phase(ZipProjectPhaseName::Run))
        .expect("run limits should resolve");

    assert_eq!(setup.timeout_sec, 11);
    assert_eq!(setup.network_access, ZipProjectNetworkAccess::Enabled);
    assert_eq!(build.memory_limit_mb, 222);
    assert_eq!(build.network_access, ZipProjectNetworkAccess::Disabled);
    assert_eq!(run.cpu_limit_millis, 3333);
    assert_eq!(run.disk_limit_mb, 4444);
    assert_eq!(run.network_access, ZipProjectNetworkAccess::Loopback);
}

/// Verifies evaluator and prepare phases use challenge-owned network policy.
#[test]
fn evaluator_and_prepare_limits_use_challenge_owned_policy() {
    let profile = resource_profile();

    let evaluator = evaluator_limits(&profile);
    let prepare_limits = prepare_limits(&profile);

    assert_eq!(evaluator.timeout_sec, 55);
    assert_eq!(evaluator.network_access, ZipProjectNetworkAccess::Disabled);
    assert_eq!(prepare_limits.timeout_sec, 44);
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
        solution: SolutionStageProfiles {
            setup: stage_profile(11, 111, 1111, 1111, ZipProjectNetworkAccess::Enabled),
            build: stage_profile(22, 222, 2222, 2222, ZipProjectNetworkAccess::Disabled),
            run: Some(stage_profile(
                33,
                333,
                3333,
                4444,
                ZipProjectNetworkAccess::Loopback,
            )),
        },
        evaluator: EvaluatorStageProfiles {
            setup: stage_profile(44, 444, 4444, 4444, ZipProjectNetworkAccess::Enabled),
            run: stage_profile(55, 555, 5555, 5555, ZipProjectNetworkAccess::Disabled),
        },
        hardware_metadata: None,
    }
}

/// Build one test stage resource profile.
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

/// Build one resolved phase for limit selection tests.
fn resolved_phase(name: ZipProjectPhaseName) -> ZipProjectResolvedPhase {
    ZipProjectResolvedPhase {
        name,
        command: ScriptPath::try_new("run.sh").expect("script path"),
    }
}
