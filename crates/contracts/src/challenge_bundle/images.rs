//! Target hardware and Docker image validation for challenge bundles.

use agentics_domain::models::challenge::{
    ChallengeTargetSpec, HardwareProfileSpec, ResourceProfileSpec, StageResourceProfile,
    TargetAccelerator,
};
use agentics_domain::models::images::ChallengeImageReference;
use agentics_error::{Result, ServiceError};
use garde::Validate;

use super::require_non_empty;

const SUPPORTED_CUDA_VARIANTS: &[(&str, &str)] =
    &[("cu126", "12.6"), ("cu130", "13.0"), ("cu132", "13.2")];
const SUPPORTED_CPU_IMAGE_REPOSITORIES: &[&str] = &[
    "agentics-linux-arm64-cpu",
    "ghcr.io/agentic-science/agentics-linux-arm64-cpu",
];
const SUPPORTED_CUDA_IMAGE_REPOSITORIES: &[&str] = &[
    "agentics-linux-arm64-cuda",
    "ghcr.io/agentic-science/agentics-linux-arm64-cuda",
];
const CPU_IMAGE_TAG_PREFIX: &str = "ubuntu26.04-";

/// Validate a target's platform, hardware, and supported image references.
pub(super) fn validate_target(target: &ChallengeTargetSpec, field: &str) -> Result<()> {
    validate_resource_profile(
        &target.resource_profile,
        &format!("{field}.resource_profile"),
    )?;

    match target.accelerator {
        TargetAccelerator::None => validate_supported_target_images(
            target,
            SupportedAcceleratorImage::NoAccelerator,
            field,
        )?,
        TargetAccelerator::Gpu => {
            let cuda_variant = validate_cuda_hardware(
                target.resource_profile.hardware_metadata.as_ref(),
                &format!("{field}.resource_profile.hardware_metadata"),
            )?;
            validate_supported_target_images(
                target,
                SupportedAcceleratorImage::Accelerator { cuda_variant },
                field,
            )?;
        }
    }

    Ok(())
}

/// Supported Agentics image families for one target kind.
enum SupportedAcceleratorImage<'a> {
    NoAccelerator,
    Accelerator { cuda_variant: &'a str },
}

/// Validate both solution and evaluator image references for a target.
fn validate_supported_target_images(
    target: &ChallengeTargetSpec,
    image_kind: SupportedAcceleratorImage<'_>,
    field: &str,
) -> Result<()> {
    validate_supported_image_reference(
        &target.resource_profile.solution_image,
        &format!("{field}.resource_profile.solution_image"),
        &image_kind,
    )?;
    validate_supported_image_reference(
        &target.resource_profile.evaluator_image,
        &format!("{field}.resource_profile.evaluator_image"),
        &image_kind,
    )
}

/// Validate that an image reference belongs to an Agentics-supported image family.
fn validate_supported_image_reference(
    image: &ChallengeImageReference,
    field: &str,
    image_kind: &SupportedAcceleratorImage<'_>,
) -> Result<()> {
    let repository = image.policy_repository();
    match image_kind {
        SupportedAcceleratorImage::NoAccelerator => {
            require_supported_image_repository(
                repository.as_ref(),
                SUPPORTED_CPU_IMAGE_REPOSITORIES,
                "linux-arm64-cpu",
                field,
            )?;
            if !image.tag().starts_with(CPU_IMAGE_TAG_PREFIX) {
                return Err(ServiceError::Validation(format!(
                    "{field} tag must start with `{CPU_IMAGE_TAG_PREFIX}` for target `linux-arm64-cpu`"
                )));
            }
        }
        SupportedAcceleratorImage::Accelerator { cuda_variant } => {
            require_supported_image_repository(
                repository.as_ref(),
                SUPPORTED_CUDA_IMAGE_REPOSITORIES,
                "linux-arm64-cuda",
                field,
            )?;
            let expected_prefix = format!("{cuda_variant}-");
            if !image.tag().starts_with(&expected_prefix) {
                return Err(ServiceError::Validation(format!(
                    "{field} tag must start with `{expected_prefix}` to match resource_profile.hardware_metadata.cuda_variant"
                )));
            }
        }
    }

    Ok(())
}

/// Require an image repository from the allowed list for one target family.
fn require_supported_image_repository(
    repository: &str,
    supported_repositories: &[&str],
    target: &str,
    field: &str,
) -> Result<()> {
    if supported_repositories.contains(&repository) {
        return Ok(());
    }
    let supported = supported_repositories.join(", ");
    Err(ServiceError::Validation(format!(
        "{field} must use a supported Agentics image repository for target `{target}`; supported repositories: {supported}"
    )))
}

/// Validate image, timeout, memory, CPU, disk, and hardware fields for a target.
fn validate_resource_profile(profile: &ResourceProfileSpec, field: &str) -> Result<()> {
    validate_garde(profile, field)?;
    validate_stage_resource_profile(&profile.solution.setup, &format!("{field}.solution.setup"))?;
    validate_stage_resource_profile(&profile.solution.build, &format!("{field}.solution.build"))?;
    if let Some(run) = &profile.solution.run {
        validate_stage_resource_profile(run, &format!("{field}.solution.run"))?;
    }
    validate_stage_resource_profile(
        &profile.evaluator.setup,
        &format!("{field}.evaluator.setup"),
    )?;
    validate_stage_resource_profile(&profile.evaluator.run, &format!("{field}.evaluator.run"))?;
    if let Some(resource_description) = &profile.resource_description {
        require_non_empty(
            resource_description,
            &format!("{field}.resource_description"),
        )?;
    }
    if let Some(hardware) = &profile.hardware_metadata {
        validate_hardware_profile(hardware, &format!("{field}.hardware_metadata"))?;
    }

    Ok(())
}

/// Validate limits for one execution stage.
fn validate_stage_resource_profile(stage: &StageResourceProfile, field: &str) -> Result<()> {
    validate_garde(stage, field)?;

    Ok(())
}

/// Validate generic hardware fields independent of target accelerator policy.
fn validate_hardware_profile(hardware: &HardwareProfileSpec, field: &str) -> Result<()> {
    validate_garde(hardware, field)?;

    Ok(())
}

fn validate_garde<T>(value: &T, field: &str) -> Result<()>
where
    T: Validate<Context = ()>,
{
    value
        .validate()
        .map_err(|report| ServiceError::Validation(format_garde_report(field, &report)))
}

fn format_garde_report(field: &str, report: &garde::Report) -> String {
    report
        .iter()
        .map(|(path, error)| {
            if path.is_empty() {
                format!("{field}: {error}")
            } else {
                format!("{field}.{path}: {error}")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn validate_positive_u32(value: u32, field: &str) -> Result<()> {
    if value == 0 {
        return Err(ServiceError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }
    Ok(())
}

/// Validate CUDA hardware fields required when a target declares GPU acceleration.
fn validate_cuda_hardware<'a>(
    hardware: Option<&'a HardwareProfileSpec>,
    field: &str,
) -> Result<&'a str> {
    let hardware = hardware.ok_or_else(|| {
        ServiceError::Validation(format!("{field}.kind must be `cuda` for accelerator `gpu`"))
    })?;
    if hardware.kind != "cuda" {
        return Err(ServiceError::Validation(format!(
            "{field}.kind must be `cuda` for accelerator `gpu`"
        )));
    }

    require_required_optional_string(&hardware.gpu_model, &format!("{field}.gpu_model"))?;
    let gpu_count = hardware.gpu_count.ok_or_else(|| {
        ServiceError::Validation(format!("{field}.gpu_count must be greater than zero"))
    })?;
    validate_positive_u32(gpu_count, &format!("{field}.gpu_count"))?;

    let cuda_variant =
        require_required_optional_string(&hardware.cuda_variant, &format!("{field}.cuda_variant"))?;
    let cuda_version =
        require_required_optional_string(&hardware.cuda_version, &format!("{field}.cuda_version"))?;
    let Some(expected_cuda_version) = cuda_version_for_variant(cuda_variant) else {
        let supported = SUPPORTED_CUDA_VARIANTS
            .iter()
            .map(|(variant, _)| *variant)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ServiceError::Validation(format!(
            "{field}.cuda_variant `{cuda_variant}` is not supported for new CUDA targets; supported variants: {supported}"
        )));
    };
    if cuda_version != expected_cuda_version {
        return Err(ServiceError::Validation(format!(
            "{field}.cuda_version must be `{expected_cuda_version}` for cuda_variant `{cuda_variant}`"
        )));
    }

    Ok(cuda_variant)
}

/// Return the expected CUDA version for a supported Agentics CUDA image variant.
fn cuda_version_for_variant(cuda_variant: &str) -> Option<&'static str> {
    SUPPORTED_CUDA_VARIANTS
        .iter()
        .find_map(|(variant, version)| (*variant == cuda_variant).then_some(*version))
}

/// Require an optional string field to be present and non-empty.
fn require_required_optional_string<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str> {
    match value {
        Some(value) => {
            require_non_empty(value, field)?;
            Ok(value)
        }
        None => Err(ServiceError::Validation(format!("{field} is required"))),
    }
}

/// Require an image reference to include an immutable digest suffix.
pub(super) fn require_image_digest_reference(
    image: &ChallengeImageReference,
    field: &str,
) -> Result<()> {
    if image.is_local() {
        return Err(ServiceError::Validation(format!(
            "{field} must use a registry image with an immutable @sha256:<digest> reference"
        )));
    }
    if image.digest().is_none() {
        return Err(ServiceError::Validation(format!(
            "{field} must include an immutable @sha256:<digest> reference"
        )));
    }

    Ok(())
}
