//! Shared target selection and MVP target policy validation.

use crate::error::{AppError, Result};
use crate::models::challenge::{ChallengeTargetSpec, DockerPlatform, TargetAccelerator};
use crate::models::names::{ChallengeName, TargetName};

/// Hosted MVP target with no accelerator.
pub const LINUX_ARM64_NO_ACCELERATOR_TARGET: &str = "linux-arm64-cpu";
/// Hosted MVP target with CUDA-capable accelerator access.
pub const LINUX_ARM64_ACCELERATOR_TARGET: &str = "linux-arm64-cuda";
/// Local process-rehearsal target for platform development only.
pub const MACOS_ARM64_NO_ACCELERATOR_DEV_TARGET: &str = "macos-arm64-cpu";

/// Target selection mode for submit and validate workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSelectionMode {
    Official,
    Validation,
}

/// Select target names from a challenge target list using the shared CLI/API contract.
pub fn select_targets_from_spec(
    challenge_name: &ChallengeName,
    targets: &[ChallengeTargetSpec],
    requested_target: Option<&TargetName>,
    all_targets: bool,
    mode: TargetSelectionMode,
) -> Result<Vec<TargetName>> {
    if all_targets {
        let selected = targets.iter().collect::<Vec<_>>();
        validate_selected_targets(challenge_name, &selected, mode)?;
        return Ok(selected.iter().map(|target| target.name.clone()).collect());
    }

    if let Some(target) = requested_target {
        let target = targets
            .iter()
            .find(|candidate| &candidate.name == target)
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "challenge `{challenge_name}` does not support target `{target}`"
                ))
            })?;
        validate_selected_targets(challenge_name, &[target], mode)?;
        return Ok(vec![target.name.clone()]);
    }

    match targets {
        [] => Err(AppError::Validation(format!(
            "challenge `{challenge_name}` does not declare any targets"
        ))),
        targets => {
            let available = targets
                .iter()
                .map(|target| target.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(AppError::Validation(format!(
                "target is required for challenge `{challenge_name}`; pass --target <target> or --all-targets. Available targets: {available}"
            )))
        }
    }
}

/// Validate that selected targets can be used for the requested workflow.
fn validate_selected_targets(
    challenge_name: &ChallengeName,
    targets: &[&ChallengeTargetSpec],
    mode: TargetSelectionMode,
) -> Result<()> {
    if mode != TargetSelectionMode::Validation {
        return Ok(());
    }

    let disabled = targets
        .iter()
        .filter(|target| !target.validation_enabled)
        .map(|target| target.name.as_str())
        .collect::<Vec<_>>();
    if disabled.is_empty() {
        return Ok(());
    }

    Err(AppError::Validation(format!(
        "validation pass is disabled for challenge `{challenge_name}` target(s): {}; submit officially or ask the challenge owner to enable validation",
        disabled.join(", ")
    )))
}

/// Validate one challenge target against the hosted MVP target policy.
pub fn validate_submission_target_policy(target: &ChallengeTargetSpec, field: &str) -> Result<()> {
    match target.name.as_str() {
        LINUX_ARM64_NO_ACCELERATOR_TARGET => require_target_shape(
            target,
            field,
            DockerPlatform::LinuxArm64,
            TargetAccelerator::None,
        ),
        LINUX_ARM64_ACCELERATOR_TARGET => require_target_shape(
            target,
            field,
            DockerPlatform::LinuxArm64,
            TargetAccelerator::Gpu,
        ),
        MACOS_ARM64_NO_ACCELERATOR_DEV_TARGET => Err(AppError::Validation(format!(
            "{field}.name `{}` is a platform-development target and cannot be used for hosted challenge deployment or submissions",
            target.name
        ))),
        "linux-amd64-cpu" | "linux-amd64-cuda" => Err(AppError::Validation(format!(
            "{field}.name `{}` is reserved for post-MVP deployment support",
            target.name
        ))),
        other => Err(AppError::Validation(format!(
            "{field}.name `{other}` is not supported for MVP hosted challenge deployment; supported targets: {LINUX_ARM64_NO_ACCELERATOR_TARGET}, {LINUX_ARM64_ACCELERATOR_TARGET}"
        ))),
    }
}

/// Validate a local platform-development target name.
pub fn validate_platform_dev_target_name(target: &TargetName, field: &str) -> Result<()> {
    match target.as_str() {
        LINUX_ARM64_NO_ACCELERATOR_TARGET
        | LINUX_ARM64_ACCELERATOR_TARGET
        | MACOS_ARM64_NO_ACCELERATOR_DEV_TARGET => Ok(()),
        "linux-amd64-cpu" | "linux-amd64-cuda" => Err(AppError::Validation(format!(
            "{field} `{target}` is reserved for post-MVP platform development"
        ))),
        other => Err(AppError::Validation(format!(
            "{field} `{other}` is not supported for MVP platform development; supported targets: {LINUX_ARM64_NO_ACCELERATOR_TARGET}, {LINUX_ARM64_ACCELERATOR_TARGET}, {MACOS_ARM64_NO_ACCELERATOR_DEV_TARGET}"
        ))),
    }
}

/// Require the platform and accelerator fields that a target name implies.
fn require_target_shape(
    target: &ChallengeTargetSpec,
    field: &str,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
) -> Result<()> {
    if target.docker_platform != docker_platform {
        return Err(AppError::Validation(format!(
            "{field}.docker_platform must be `{}` for target `{}`",
            docker_platform.as_str(),
            target.name
        )));
    }
    if target.accelerator != accelerator {
        return Err(AppError::Validation(format!(
            "{field}.accelerator must be {} for target `{}`",
            accelerator_json_name(accelerator),
            target.name
        )));
    }
    Ok(())
}

/// Render accelerator values in the public JSON notation.
fn accelerator_json_name(accelerator: TargetAccelerator) -> &'static str {
    match accelerator {
        TargetAccelerator::None => "null",
        TargetAccelerator::Gpu => "\"gpu\"",
    }
}

#[cfg(test)]
mod tests {
    use crate::models::challenge::{
        ChallengeTargetSpec, DockerPlatform, ResourceProfileSpec, TargetAccelerator,
    };
    use crate::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use crate::models::names::{ChallengeName, ResourceProfileName, TargetName};
    use crate::zip_project::ZipProjectNetworkAccess;

    use super::{
        LINUX_ARM64_ACCELERATOR_TARGET, LINUX_ARM64_NO_ACCELERATOR_TARGET, TargetSelectionMode,
        select_targets_from_spec, validate_platform_dev_target_name,
        validate_submission_target_policy,
    };

    fn challenge_name() -> ChallengeName {
        ChallengeName::try_new("sample-sum".to_string()).expect("challenge name")
    }

    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("target name")
    }

    fn target(
        value: &str,
        accelerator: TargetAccelerator,
        validation_enabled: bool,
    ) -> ChallengeTargetSpec {
        let image = ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(
                "agentics-linux-arm64-cpu:ubuntu26.04-local",
            )
            .expect("image"),
        };
        ChallengeTargetSpec {
            name: target_name(value),
            docker_platform: DockerPlatform::LinuxArm64,
            accelerator,
            validation_enabled,
            resource_profile: ResourceProfileSpec {
                name: ResourceProfileName::try_new("agentics-small".to_string()).expect("profile"),
                resource_description: None,
                solution_image: image.clone(),
                scorer_image: image,
                timeout_sec: 30,
                memory_limit_mb: 512,
                cpu_limit_millis: 1000,
                disk_limit_mb: 1024,
                setup_network_access: ZipProjectNetworkAccess::Disabled,
                build_network_access: ZipProjectNetworkAccess::Disabled,
                run_network_access: ZipProjectNetworkAccess::Disabled,
                scorer_network_access: ZipProjectNetworkAccess::Disabled,
                hardware_metadata: None,
            },
        }
    }

    #[test]
    fn selects_targets_with_validation_policy() {
        let challenge_name = challenge_name();
        let targets = vec![
            target(
                LINUX_ARM64_NO_ACCELERATOR_TARGET,
                TargetAccelerator::None,
                true,
            ),
            target(
                LINUX_ARM64_ACCELERATOR_TARGET,
                TargetAccelerator::Gpu,
                false,
            ),
        ];

        let selected = select_targets_from_spec(
            &challenge_name,
            &targets,
            Some(&target_name(LINUX_ARM64_NO_ACCELERATOR_TARGET)),
            false,
            TargetSelectionMode::Validation,
        )
        .expect("enabled target should select");
        assert_eq!(
            selected,
            vec![target_name(LINUX_ARM64_NO_ACCELERATOR_TARGET)]
        );

        assert!(
            select_targets_from_spec(
                &challenge_name,
                &targets,
                None,
                true,
                TargetSelectionMode::Validation,
            )
            .is_err()
        );
    }

    #[test]
    fn validates_mvp_target_policy() {
        let valid = target(
            LINUX_ARM64_NO_ACCELERATOR_TARGET,
            TargetAccelerator::None,
            true,
        );
        validate_submission_target_policy(&valid, "targets[0]").expect("target should validate");

        let invalid = target("main", TargetAccelerator::None, true);
        assert!(validate_submission_target_policy(&invalid, "targets[0]").is_err());

        let mismatched = target(
            LINUX_ARM64_ACCELERATOR_TARGET,
            TargetAccelerator::None,
            true,
        );
        assert!(validate_submission_target_policy(&mismatched, "targets[0]").is_err());
    }

    #[test]
    fn validates_platform_dev_targets() {
        validate_platform_dev_target_name(&target_name("macos-arm64-cpu"), "target")
            .expect("macos dev target should validate");
        assert!(
            validate_platform_dev_target_name(&target_name("linux-amd64-cpu"), "target").is_err()
        );
    }
}
