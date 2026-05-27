//! Hosted runner cleanup for production Compose operations.

use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroI32;

use agentics_config::RunnerNamespace;
use agentics_domain::models::ids::EvaluationJobId;
use agentics_runner::{
    RUNNER_ATTEMPT_COUNT_LABEL, RUNNER_JOB_ID_LABEL, RUNNER_KIND_LABEL, RUNNER_KIND_ZIP_PROJECT,
    RUNNER_NAMESPACE_LABEL, RUNNER_PHASE_LABEL, RUNNER_SCOPE_LABEL, RUNNER_WORKER_ID_LABEL,
};
use bollard::Docker;
use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder};
use sqlx::Row;

use crate::support::{DEFAULT_DOCKER_SOCKET_PATH, ReportLine};

use super::{ComposeContext, ComposeProdError, RunnerCleanupScope};

pub(super) async fn clean_runners(
    context: &ComposeContext,
    namespace: &RunnerNamespace,
    scope: RunnerCleanupScope,
    dry_run: bool,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    let docker = connect_docker(context)?;
    let mut runners = list_runner_containers(&docker, namespace, scope).await?;
    runners.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));

    let db = RunnerClaimLookup::from_context(context).await;
    let mut reports = Vec::new();
    if runners.is_empty() {
        reports.push(ReportLine::pass(
            "runner cleanup",
            format!(
                "no matching runner containers for namespace {}",
                namespace.as_str()
            ),
        ));
        return Ok(reports);
    }

    for runner in runners {
        let claim_status = db.describe_claim(&runner).await;
        let message = format!(
            "{} {} job={} worker={} attempt={} phase={} {claim_status}",
            if dry_run { "would remove" } else { "removed" },
            runner.short_id(),
            runner.job_id,
            runner.worker_id,
            runner.attempt_count.get(),
            runner.phase
        );
        if !dry_run {
            docker
                .remove_container(
                    &runner.id,
                    Some(RemoveContainerOptionsBuilder::default().force(true).build()),
                )
                .await?;
        }
        reports.push(ReportLine::pass("runner cleanup", message));
    }

    Ok(reports)
}

async fn list_runner_containers(
    docker: &Docker,
    namespace: &RunnerNamespace,
    scope: RunnerCleanupScope,
) -> Result<Vec<RunnerContainer>, ComposeProdError> {
    let mut filters = HashMap::new();
    filters.insert(
        "label",
        vec![
            format!("{RUNNER_KIND_LABEL}={RUNNER_KIND_ZIP_PROJECT}"),
            format!("{}={}", RUNNER_NAMESPACE_LABEL, namespace.as_str()),
            format!("{}={}", RUNNER_SCOPE_LABEL, scope.as_label()),
        ],
    );
    let options = ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&filters)
        .build();
    let containers = docker.list_containers(Some(options)).await?;
    Ok(containers
        .into_iter()
        .filter_map(|container| RunnerContainer::from_summary(container, namespace, scope))
        .collect())
}

fn connect_docker(context: &ComposeContext) -> Result<Docker, ComposeProdError> {
    if let Some(host) = context.docker_host() {
        return Docker::connect_with_host(&host).map_err(ComposeProdError::Docker);
    }
    if let Some(socket_path) = context.docker_socket_path() {
        return Docker::connect_with_host(&format!("unix://{socket_path}"))
            .map_err(ComposeProdError::Docker);
    }
    Docker::connect_with_host(&format!("unix://{DEFAULT_DOCKER_SOCKET_PATH}"))
        .or_else(|_| Docker::connect_with_defaults())
        .map_err(ComposeProdError::Docker)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerContainer {
    id: String,
    job_id: EvaluationJobId,
    worker_id: RunnerWorkerLabel,
    attempt_count: NonZeroI32,
    phase: RunnerPhaseLabel,
}

impl RunnerContainer {
    fn from_summary(
        container: bollard::models::ContainerSummary,
        namespace: &RunnerNamespace,
        scope: RunnerCleanupScope,
    ) -> Option<Self> {
        let labels = container.labels.as_ref()?;
        if labels.get(RUNNER_KIND_LABEL).map(String::as_str) != Some(RUNNER_KIND_ZIP_PROJECT)
            || labels.get(RUNNER_NAMESPACE_LABEL).map(String::as_str) != Some(namespace.as_str())
            || labels.get(RUNNER_SCOPE_LABEL).map(String::as_str) != Some(scope.as_label())
        {
            return None;
        }
        Some(Self {
            id: container.id?,
            job_id: required_label(labels, RUNNER_JOB_ID_LABEL)
                .and_then(|value| EvaluationJobId::try_new(value).ok())?,
            worker_id: required_label(labels, RUNNER_WORKER_ID_LABEL)
                .and_then(RunnerWorkerLabel::try_new)?,
            attempt_count: required_label(labels, RUNNER_ATTEMPT_COUNT_LABEL)
                .and_then(parse_positive_attempt_count)?,
            phase: required_label(labels, RUNNER_PHASE_LABEL)
                .and_then(RunnerPhaseLabel::try_new)?,
        })
    }

    fn short_id(&self) -> &str {
        self.id.get(..12).unwrap_or(&self.id)
    }

    fn sort_key(&self) -> (&str, &str, i32, &str, &str) {
        (
            self.job_id.as_str(),
            self.worker_id.as_str(),
            self.attempt_count.get(),
            self.phase.as_str(),
            self.id.as_str(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerWorkerLabel(String);

impl RunnerWorkerLabel {
    fn try_new(value: String) -> Option<Self> {
        non_empty_label(value).map(Self)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunnerWorkerLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerPhaseLabel(String);

impl RunnerPhaseLabel {
    fn try_new(value: String) -> Option<Self> {
        non_empty_label(value).map(Self)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunnerPhaseLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn required_label(labels: &HashMap<String, String>, name: &str) -> Option<String> {
    labels
        .get(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn non_empty_label(value: String) -> Option<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn parse_positive_attempt_count(value: String) -> Option<NonZeroI32> {
    let attempt = value.parse::<NonZeroI32>().ok()?;
    (attempt.get() > 0).then_some(attempt)
}

#[derive(Debug)]
enum RunnerClaimLookup {
    Connected(sqlx::PgPool),
    Unavailable(String),
}

impl RunnerClaimLookup {
    async fn from_context(context: &ComposeContext) -> Self {
        let Some(database_url) = context.database_url().filter(|value| !value.contains("${"))
        else {
            return Self::Unavailable("db=not-configured-for-host-check".to_string());
        };
        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
        {
            Ok(pool) => Self::Connected(pool),
            Err(_) => Self::Unavailable("db=unreachable".to_string()),
        }
    }

    async fn describe_claim(&self, runner: &RunnerContainer) -> String {
        let pool = match self {
            Self::Connected(pool) => pool,
            Self::Unavailable(message) => return message.clone(),
        };
        match sqlx::query(
            r#"
            SELECT status, worker_id, attempt_count
            FROM evaluation_jobs
            WHERE id = $1::uuid
            "#,
        )
        .bind(runner.job_id.as_str())
        .fetch_optional(pool)
        .await
        {
            Ok(Some(row)) => {
                let status = row
                    .try_get::<String, _>("status")
                    .unwrap_or_else(|_| "unknown".to_string());
                let worker_id = row
                    .try_get::<Option<String>, _>("worker_id")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "none".to_string());
                let attempt_count = row.try_get::<i32, _>("attempt_count").unwrap_or_default();
                let matches = status == "running"
                    && worker_id == runner.worker_id.as_str()
                    && attempt_count == runner.attempt_count.get();
                format!(
                    "db=status:{status},worker:{worker_id},attempt:{attempt_count},matches:{matches}"
                )
            }
            Ok(None) => "db=missing-job".to_string(),
            Err(_) => "db=query-failed".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RunnerContainer;
    use agentics_config::RunnerNamespace;
    use agentics_runner::{
        RUNNER_ATTEMPT_COUNT_LABEL, RUNNER_JOB_ID_LABEL, RUNNER_KIND_LABEL,
        RUNNER_KIND_ZIP_PROJECT, RUNNER_NAMESPACE_LABEL, RUNNER_PHASE_LABEL,
        RUNNER_SCOPE_HOSTED_WORKER, RUNNER_SCOPE_LABEL, RUNNER_WORKER_ID_LABEL,
    };
    use std::collections::HashMap;

    /// Verifies runner cleanup accepts only exact Agentics production labels.
    #[test]
    fn runner_container_requires_exact_labels() {
        let namespace = RunnerNamespace::try_new("agentics-prod").expect("valid namespace");
        let labels = runner_labels("agentics-prod", RUNNER_SCOPE_HOSTED_WORKER);
        let container = bollard::models::ContainerSummary {
            id: Some("abcdef1234567890".to_string()),
            labels: Some(labels),
            ..Default::default()
        };
        let parsed = RunnerContainer::from_summary(
            container,
            &namespace,
            super::super::RunnerCleanupScope::HostedWorker,
        )
        .expect("labels should parse");
        assert_eq!(
            parsed.job_id.as_str(),
            "20000000-0000-4000-8000-000000000001"
        );
        assert_eq!(parsed.attempt_count.get(), 2);

        let wrong_namespace = RunnerNamespace::try_new("other").expect("valid namespace");
        let container = bollard::models::ContainerSummary {
            id: Some("abcdef1234567890".to_string()),
            labels: Some(runner_labels("agentics-prod", RUNNER_SCOPE_HOSTED_WORKER)),
            ..Default::default()
        };
        assert!(
            RunnerContainer::from_summary(
                container,
                &wrong_namespace,
                super::super::RunnerCleanupScope::HostedWorker
            )
            .is_none()
        );
    }

    fn runner_labels(namespace: &str, scope: &str) -> HashMap<String, String> {
        HashMap::from([
            (
                RUNNER_KIND_LABEL.to_string(),
                RUNNER_KIND_ZIP_PROJECT.to_string(),
            ),
            (RUNNER_NAMESPACE_LABEL.to_string(), namespace.to_string()),
            (RUNNER_SCOPE_LABEL.to_string(), scope.to_string()),
            (
                RUNNER_JOB_ID_LABEL.to_string(),
                "20000000-0000-4000-8000-000000000001".to_string(),
            ),
            (RUNNER_WORKER_ID_LABEL.to_string(), "worker-a".to_string()),
            (RUNNER_ATTEMPT_COUNT_LABEL.to_string(), "2".to_string()),
            (RUNNER_PHASE_LABEL.to_string(), "run".to_string()),
        ])
    }
}
