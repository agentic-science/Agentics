use std::collections::HashMap;

use serde::Deserialize;

use super::ComposeProdError;

const ENV_PREFIX: &str = "AGENTICS_";

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct RawComposeProdEnv {
    pub(super) compose_prod_project: Option<String>,
    pub(super) compose_prod_env_file: Option<String>,
    pub(super) deployment_stage: Option<String>,
    pub(super) worker_accelerators: Option<String>,
    pub(super) worker_gpu_probe_image: Option<String>,
    pub(super) runner_namespace: Option<String>,
    pub(super) database_url: Option<String>,
    pub(super) docker_host: Option<String>,
    pub(super) docker_socket_path: Option<String>,
    pub(super) docker_socket_gid: Option<u32>,
    pub(super) dgx_state_root: Option<String>,
    pub(super) storage_work_root: Option<String>,
    pub(super) challenge_review_repository_host_root: Option<String>,
    pub(super) runner_runtime_root: Option<String>,
    pub(super) runner_phase_mount_root: Option<String>,
    pub(super) dgx_phase_mount_root: Option<String>,
    pub(super) dgx_docker_data_root: Option<String>,
    pub(super) dgx_runner_docker_exec_root: Option<String>,
    pub(super) dgx_runner_docker_pidfile: Option<String>,
    pub(super) dgx_runner_docker_log: Option<String>,
    pub(super) dgx_runner_docker_bridge: Option<String>,
    pub(super) dgx_runner_docker_bridge_cidr: Option<String>,
    pub(super) rustfs_backup_container: Option<String>,
}

impl RawComposeProdEnv {
    pub(super) fn from_process() -> Result<Self, ComposeProdError> {
        envy::prefixed(ENV_PREFIX)
            .from_env::<Self>()
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))
    }

    pub(super) fn from_map(values: &HashMap<String, String>) -> Result<Self, ComposeProdError> {
        envy::prefixed(ENV_PREFIX)
            .from_iter(values.clone())
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))
    }
}
