use agentics_contracts::zip_project::{
    ZipProjectPhaseLimits, ZipProjectPhaseName, ZipProjectResolvedPhase,
};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{
    ResourceProfileSpec, StageResourceProfile, TargetAccelerator,
};

use super::storage::WritablePhase;

/// Platform-owned limits applied to one runner evaluation.
#[derive(Clone, Copy)]
pub(super) struct EvaluationLimitConfig {
    pub(super) max_runs: u64,
    pub(super) max_result_json_bytes: u64,
    pub(super) max_public_results: u64,
    pub(super) max_result_log_bytes: u64,
}

/// Return the enforced accelerator count for one container request.
pub(super) fn effective_accelerator_count(
    profile: &ResourceProfileSpec,
    accelerator: TargetAccelerator,
) -> Result<Option<u32>> {
    match accelerator {
        TargetAccelerator::None => Ok(None),
        TargetAccelerator::Gpu => {
            let hardware = profile.hardware_metadata.as_ref().ok_or_else(|| {
                ServiceError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata".to_string(),
                )
            })?;
            let count = hardware.gpu_count.ok_or_else(|| {
                ServiceError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata.gpu_count"
                        .to_string(),
                )
            })?;
            if count == 0 {
                return Err(ServiceError::Runner(
                    "resource_profile.hardware_metadata.gpu_count must be greater than zero"
                        .to_string(),
                ));
            }
            Ok(Some(count))
        }
    }
}

pub(super) fn effective_phase_limits(
    profile: &ResourceProfileSpec,
    phase: &ZipProjectResolvedPhase,
) -> Result<ZipProjectPhaseLimits> {
    let stage = match phase.name {
        ZipProjectPhaseName::Setup => &profile.solution.setup,
        ZipProjectPhaseName::Build => &profile.solution.build,
        ZipProjectPhaseName::Run => profile.solution.run.as_ref().ok_or_else(|| {
            ServiceError::Runner(
                "resource_profile.solution.run is required for solution run".to_string(),
            )
        })?,
    };
    Ok(stage_limits(stage))
}

pub(super) fn evaluator_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    stage_limits(&profile.evaluator.run)
}

pub(super) fn evaluator_setup_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    stage_limits(&profile.evaluator.setup)
}

fn stage_limits(stage: &StageResourceProfile) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: stage.timeout_sec,
        memory_limit_mb: stage.memory_limit_mb,
        cpu_limit_millis: stage.cpu_limit_millis,
        disk_limit_mb: stage.disk_limit_mb,
        network_access: stage.network_access,
    }
}

pub(super) fn writable_phase_for_solution_phase(phase: ZipProjectPhaseName) -> WritablePhase {
    match phase {
        ZipProjectPhaseName::Setup => WritablePhase::SolutionSetup,
        ZipProjectPhaseName::Build => WritablePhase::SolutionBuild,
        ZipProjectPhaseName::Run => WritablePhase::SolutionRun,
    }
}
