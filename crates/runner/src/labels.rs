//! Docker label vocabulary for Agentics runner containers.

/// Docker label marking an Agentics-owned runner container.
pub const RUNNER_KIND_LABEL: &str = "agentics.runner";
/// Docker label value for `zip_project` runner containers.
pub const RUNNER_KIND_ZIP_PROJECT: &str = "zip_project";
/// Docker label storing the runner namespace.
pub const RUNNER_NAMESPACE_LABEL: &str = "agentics.runner_namespace";
/// Docker label storing the runner ownership scope.
pub const RUNNER_SCOPE_LABEL: &str = "agentics.runner_scope";
/// Docker label value for hosted worker runner containers.
pub const RUNNER_SCOPE_HOSTED_WORKER: &str = "hosted-worker";
/// Docker label value for local validation runner containers.
pub const RUNNER_SCOPE_LOCAL_VALIDATION: &str = "local-validation";
/// Docker label storing the evaluation job id.
pub const RUNNER_JOB_ID_LABEL: &str = "agentics.job_id";
/// Docker label storing the worker id that created a runner container.
pub const RUNNER_WORKER_ID_LABEL: &str = "agentics.worker_id";
/// Docker label storing the evaluation attempt count.
pub const RUNNER_ATTEMPT_COUNT_LABEL: &str = "agentics.attempt_count";
/// Docker label storing the execution phase.
pub const RUNNER_PHASE_LABEL: &str = "agentics.phase";
