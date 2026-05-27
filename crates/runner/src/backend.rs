use async_trait::async_trait;
use bollard::Docker;

use agentics_config::RunnerNamespace;

use super::docker::{
    self, ContainerOutcome, ContainerRequest, InteractiveSessionOutcome,
    RunnerContainerCleanupSummary, RunnerContainerSnapshot,
};
use agentics_domain::models::challenge::DockerPlatform;
use agentics_error::Result;

/// Captures the platform execution backend used by topology orchestration.
#[async_trait]
pub(super) trait RunnerBackend: Sync {
    async fn pre_pull_image(&self, image: &str, platform: DockerPlatform) -> Result<()>;

    async fn run_container(&self, request: ContainerRequest) -> Result<ContainerOutcome>;

    async fn run_interactive_stdio_session(
        &self,
        participant: ContainerRequest,
        interactive_evaluator: ContainerRequest,
        max_interaction_bytes_per_direction: u64,
        shutdown_grace_secs: u64,
    ) -> Result<InteractiveSessionOutcome>;

    async fn list_hosted_worker_containers(&self) -> Result<Vec<RunnerContainerSnapshot>>;

    async fn remove_runner_container(&self, container_id: &str) -> Result<()>;

    async fn kill_runner_container(&self, container_id: &str) -> Result<()>;

    async fn remove_stopped_runner_containers(&self) -> Result<u64>;

    async fn remove_stale_local_validation_containers(
        &self,
    ) -> Result<RunnerContainerCleanupSummary>;
}

/// Docker-backed runner backend used for MVP execution.
pub(super) struct DockerRunnerBackend<'a> {
    docker: &'a Docker,
    runner_namespace: &'a RunnerNamespace,
}

impl<'a> DockerRunnerBackend<'a> {
    pub(super) const fn new(docker: &'a Docker, runner_namespace: &'a RunnerNamespace) -> Self {
        Self {
            docker,
            runner_namespace,
        }
    }
}

#[async_trait]
impl RunnerBackend for DockerRunnerBackend<'_> {
    async fn pre_pull_image(&self, image: &str, platform: DockerPlatform) -> Result<()> {
        docker::pre_pull_image(self.docker, image, platform).await
    }

    async fn run_container(&self, request: ContainerRequest) -> Result<ContainerOutcome> {
        docker::run_container(self.docker, request).await
    }

    async fn run_interactive_stdio_session(
        &self,
        participant: ContainerRequest,
        interactive_evaluator: ContainerRequest,
        max_interaction_bytes_per_direction: u64,
        shutdown_grace_secs: u64,
    ) -> Result<InteractiveSessionOutcome> {
        docker::run_interactive_stdio_session(
            self.docker,
            participant,
            interactive_evaluator,
            max_interaction_bytes_per_direction,
            shutdown_grace_secs,
        )
        .await
    }

    async fn list_hosted_worker_containers(&self) -> Result<Vec<RunnerContainerSnapshot>> {
        docker::list_hosted_worker_runner_containers(self.docker, self.runner_namespace).await
    }

    async fn remove_runner_container(&self, container_id: &str) -> Result<()> {
        docker::remove_runner_container(self.docker, container_id).await
    }

    async fn kill_runner_container(&self, container_id: &str) -> Result<()> {
        docker::kill_runner_container(self.docker, container_id).await
    }

    async fn remove_stopped_runner_containers(&self) -> Result<u64> {
        docker::remove_stopped_runner_containers(self.docker, self.runner_namespace).await
    }

    async fn remove_stale_local_validation_containers(
        &self,
    ) -> Result<RunnerContainerCleanupSummary> {
        docker::remove_stale_local_validation_containers(self.docker, self.runner_namespace).await
    }
}
