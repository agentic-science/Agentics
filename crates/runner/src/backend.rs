use async_trait::async_trait;
use bollard::Docker;
use sqlx::PgPool;

use super::docker::{
    self, ContainerOutcome, ContainerRequest, InteractiveSessionOutcome,
    RunnerContainerCleanupSummary,
};
use agentics_domain::error::Result;
use agentics_domain::models::challenge::DockerPlatform;

/// Captures the platform execution backend used by topology orchestration.
#[async_trait]
pub(super) trait RunnerBackend: Sync {
    async fn pre_pull_image(&self, image: &str, platform: DockerPlatform) -> Result<()>;

    async fn run_container(&self, request: ContainerRequest) -> Result<ContainerOutcome>;

    async fn run_interactive_stdio_session(
        &self,
        participant: ContainerRequest,
        interactor: ContainerRequest,
        max_interaction_bytes_per_direction: u64,
        shutdown_grace_secs: u64,
    ) -> Result<InteractiveSessionOutcome>;

    async fn reconcile_containers(
        &self,
        pool: &PgPool,
        stale_minutes: i32,
    ) -> Result<RunnerContainerCleanupSummary>;

    async fn remove_stopped_runner_containers(&self) -> Result<u64>;

    async fn remove_stale_local_validation_containers(
        &self,
    ) -> Result<RunnerContainerCleanupSummary>;
}

/// Docker-backed runner backend used for MVP execution.
pub(super) struct DockerRunnerBackend<'a> {
    docker: &'a Docker,
}

impl<'a> DockerRunnerBackend<'a> {
    pub(super) const fn new(docker: &'a Docker) -> Self {
        Self { docker }
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
        interactor: ContainerRequest,
        max_interaction_bytes_per_direction: u64,
        shutdown_grace_secs: u64,
    ) -> Result<InteractiveSessionOutcome> {
        docker::run_interactive_stdio_session(
            self.docker,
            participant,
            interactor,
            max_interaction_bytes_per_direction,
            shutdown_grace_secs,
        )
        .await
    }

    async fn reconcile_containers(
        &self,
        pool: &PgPool,
        stale_minutes: i32,
    ) -> Result<RunnerContainerCleanupSummary> {
        docker::reconcile_runner_containers(self.docker, pool, stale_minutes).await
    }

    async fn remove_stopped_runner_containers(&self) -> Result<u64> {
        docker::remove_stopped_runner_containers(self.docker).await
    }

    async fn remove_stale_local_validation_containers(
        &self,
    ) -> Result<RunnerContainerCleanupSummary> {
        docker::remove_stale_local_validation_containers(self.docker).await
    }
}
