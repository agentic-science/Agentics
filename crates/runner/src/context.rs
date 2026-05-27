use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bollard::Docker;

use agentics_config::RunnerNamespace;
use agentics_domain::models::challenge::{DockerPlatform, TargetAccelerator};

use super::backend::RunnerBackend;
use super::storage::{RunnerStorage, WritableMountLease};
use super::{
    RUNNER_ATTEMPT_COUNT_LABEL, RUNNER_JOB_ID_LABEL, RUNNER_NAMESPACE_LABEL, RUNNER_PHASE_LABEL,
    RUNNER_SCOPE_HOSTED_WORKER, RUNNER_SCOPE_LABEL, RUNNER_SCOPE_LOCAL_VALIDATION,
    RUNNER_WORKER_ID_LABEL,
};

#[derive(Clone, Copy)]
/// Carries runner context data across this module boundary.
pub(super) struct RunnerContext<'a> {
    pub(super) docker: &'a Docker,
    pub(super) backend: &'a dyn RunnerBackend,
    pub(super) storage: &'a RunnerStorage,
    pub(super) runner_namespace: &'a RunnerNamespace,
    pub(super) job_id: &'a str,
    pub(super) attempt: &'a RunnerAttempt,
    pub(super) container_scope: RunnerContainerScope,
}

impl RunnerContext<'_> {
    /// Build Docker labels that identify one runner container and its slot owner.
    pub(super) fn container_labels(
        self,
        phase: &str,
        writable_mount: Option<&WritableMountLease>,
    ) -> HashMap<String, String> {
        let mut labels = HashMap::from([
            (RUNNER_JOB_ID_LABEL.to_string(), self.job_id.to_string()),
            (
                RUNNER_WORKER_ID_LABEL.to_string(),
                self.attempt.worker_id.clone(),
            ),
            (
                RUNNER_ATTEMPT_COUNT_LABEL.to_string(),
                self.attempt.attempt_count.to_string(),
            ),
            (
                RUNNER_NAMESPACE_LABEL.to_string(),
                self.runner_namespace.as_str().to_string(),
            ),
            (
                RUNNER_SCOPE_LABEL.to_string(),
                self.container_scope.as_label().to_string(),
            ),
            (RUNNER_PHASE_LABEL.to_string(), phase.to_string()),
        ]);
        if let Some(writable_mount) = writable_mount {
            labels.extend(writable_mount.docker_labels());
        }
        labels
    }
}

/// Product-level execution requirements resolved before backend calls.
#[derive(Clone, Copy)]
pub(super) struct JobRequirement {
    pub(super) docker_platform: DockerPlatform,
    pub(super) accelerator: TargetAccelerator,
}

impl JobRequirement {
    pub(super) const fn new(
        docker_platform: DockerPlatform,
        accelerator: TargetAccelerator,
    ) -> Self {
        Self {
            docker_platform,
            accelerator,
        }
    }
}

/// Identifies one concrete execution attempt for transient runner resources.
pub(super) struct RunnerAttempt {
    pub(super) worker_id: String,
    pub(super) attempt_count: i32,
    pub(super) transient_name: String,
}

impl RunnerAttempt {
    /// Build an attempt identity safe for Docker names and temporary paths.
    pub(super) fn new(job_id: &str, worker_id: &str, attempt_count: i32) -> Self {
        let nonce = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>();
        Self {
            worker_id: sanitize_name_component(worker_id),
            attempt_count,
            transient_name: format!(
                "{}-attempt-{}-{}",
                sanitize_name_component(job_id),
                attempt_count,
                nonce
            ),
        }
    }
}

/// Keeps a retained runner tree alive when it is backed by a bounded slot lease.
pub(super) struct RetainedRunnerTree {
    path: PathBuf,
    _lease: Option<WritableMountLease>,
}

impl RetainedRunnerTree {
    /// Return the host path used for subsequent read-only mounts.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Build a retained tree from an existing runtime path.
    pub(super) fn runtime_path(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            _lease: None,
        }
    }

    /// Build a retained tree that keeps its writable mount lease alive.
    pub(super) fn leased(lease: WritableMountLease) -> Self {
        let path = lease.path().to_path_buf();
        Self {
            path,
            _lease: Some(lease),
        }
    }
}

/// Keeps one evaluator-visible run tree alive until the evaluator finishes.
pub(super) struct RetainedRunTree {
    pub(super) run_name: String,
    pub(super) tree: RetainedRunnerTree,
}

/// Docker label scope separating hosted worker containers from CLI local validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerContainerScope {
    HostedWorker,
    LocalValidation,
}

impl RunnerContainerScope {
    /// Stable Docker label value for this runner container scope.
    pub(super) fn as_label(self) -> &'static str {
        match self {
            Self::HostedWorker => RUNNER_SCOPE_HOSTED_WORKER,
            Self::LocalValidation => RUNNER_SCOPE_LOCAL_VALIDATION,
        }
    }
}

/// Build a Docker container name for one attempt-local phase.
pub(super) fn container_name(attempt: &RunnerAttempt, suffix: &str) -> String {
    let safe_suffix = sanitize_name_component(suffix);
    format!("agentics-{}-{safe_suffix}", attempt.transient_name)
}

/// Convert arbitrary identifiers into Docker-name-safe components.
fn sanitize_name_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
}
