//! Disposable production rehearsal purge planning and execution.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use agentics_config::{DeploymentStage, RunnerNamespace};

use super::{
    ComposeContext, ComposeProdError, DEFAULT_PROJECT, PREFIX, REHEARSAL_PROJECT, REHEARSAL_ROOT,
    RunnerCleanupScope, clean_runners, runner_docker_down, stop_running_workers,
};
use crate::support::{ReportLine, print_reports};

pub(super) async fn purge_rehearsal_data(
    context: &ComposeContext,
    confirm_rehearsal_purge: bool,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let plan = build_rehearsal_purge_plan(context, confirm_rehearsal_purge, dry_run)?;
    if dry_run {
        return Ok(print_reports(PREFIX, &plan.dry_run_reports()));
    }

    stop_running_workers(context).await?;
    let runner_reports = match unavailable_runner_cleanup_reports(context, &plan.namespace) {
        Some(reports) => reports,
        None => {
            clean_runners(
                context,
                &plan.namespace,
                RunnerCleanupScope::HostedWorker,
                false,
            )
            .await?
        }
    };
    let runner_code = print_reports(PREFIX, &runner_reports);
    if runner_code != ExitCode::SUCCESS {
        return Ok(runner_code);
    }

    let runner_stop_code = runner_docker_down(context, false).await?;
    if runner_stop_code != ExitCode::SUCCESS {
        return Ok(runner_stop_code);
    }

    let compose_code = context
        .run_compose_passthrough(["down", "-v", "--remove-orphans"])
        .await?;
    if compose_code != ExitCode::SUCCESS {
        return Ok(compose_code);
    }

    let mut reports = vec![ReportLine::pass(
        "rehearsal purge",
        format!(
            "removed Compose project {} and runner namespace {}",
            context.project,
            plan.namespace.as_str()
        ),
    )];
    for path in &plan.cleanup_paths {
        reports.push(remove_rehearsal_path(path).await?);
    }
    Ok(print_reports(PREFIX, &reports))
}

pub(super) fn unavailable_runner_cleanup_reports(
    context: &ComposeContext,
    namespace: &RunnerNamespace,
) -> Option<Vec<ReportLine>> {
    let socket_path = context.docker_socket_path().map(PathBuf::from)?;
    if socket_path.exists() {
        return None;
    }
    Some(vec![ReportLine::skip(
        "runner cleanup",
        format!(
            "runner Docker socket {} did not exist; assuming daemon already stopped for namespace {}",
            socket_path.display(),
            namespace.as_str()
        ),
    )])
}

#[derive(Debug, Clone)]
pub(super) struct RehearsalPurgePlan {
    pub(super) namespace: RunnerNamespace,
    pub(super) reported_paths: Vec<PathBuf>,
    cleanup_paths: Vec<PathBuf>,
}

impl RehearsalPurgePlan {
    pub(super) fn dry_run_reports(&self) -> Vec<ReportLine> {
        let mut reports = vec![
            ReportLine::pass(
                "rehearsal purge",
                format!("would remove Compose project {REHEARSAL_PROJECT}"),
            ),
            ReportLine::pass(
                "rehearsal purge",
                format!(
                    "would remove runner containers in namespace {}",
                    self.namespace.as_str()
                ),
            ),
            ReportLine::pass(
                "rehearsal purge",
                "would stop the dedicated rehearsal runner Docker daemon",
            ),
            ReportLine::pass(
                "rehearsal purge",
                "would remove rehearsal Compose volumes with docker compose down -v",
            ),
        ];
        reports.extend(self.reported_paths.iter().map(|path| {
            ReportLine::pass(
                "rehearsal purge path",
                format!("would remove or verify {}", path.display()),
            )
        }));
        reports
    }
}

pub(super) fn build_rehearsal_purge_plan(
    context: &ComposeContext,
    confirm_rehearsal_purge: bool,
    dry_run: bool,
) -> Result<RehearsalPurgePlan, ComposeProdError> {
    if !dry_run && !confirm_rehearsal_purge {
        return Err(ComposeProdError::InvalidConfig(
            "destructive rehearsal purge requires --confirm-rehearsal-purge".to_string(),
        ));
    }
    if context.project == DEFAULT_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "refusing to purge production Compose project {DEFAULT_PROJECT}"
        )));
    }
    if context.project != REHEARSAL_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge requires --project {REHEARSAL_PROJECT}, got {}",
            context.project
        )));
    }
    if context.file_deployment_stage() != Some(DeploymentStage::Rehearsal) {
        return Err(ComposeProdError::InvalidConfig(
            "rehearsal purge requires AGENTICS_DEPLOYMENT_STAGE=rehearsal in the env file"
                .to_string(),
        ));
    }
    let namespace = context.resolve_namespace(None)?;
    if namespace.as_str() != REHEARSAL_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge requires AGENTICS_RUNNER_NAMESPACE={REHEARSAL_PROJECT}, got {}",
            namespace.as_str()
        )));
    }

    let reported_paths = collect_rehearsal_paths(context)?;
    for path in &reported_paths {
        require_rehearsal_path(path)?;
    }
    let cleanup_paths = collapse_cleanup_paths(&reported_paths);
    Ok(RehearsalPurgePlan {
        namespace,
        reported_paths,
        cleanup_paths,
    })
}

fn collect_rehearsal_paths(context: &ComposeContext) -> Result<Vec<PathBuf>, ComposeProdError> {
    let mut paths = BTreeSet::new();
    paths.insert(context.path_value(|env| env.dgx_state_root.as_ref(), "/srv/agentics"));
    paths.insert(context.path_value(
        |env| env.storage_work_root.as_ref(),
        "/srv/agentics/storage-work",
    ));
    paths.insert(context.path_value(
        |env| env.challenge_review_repository_host_root.as_ref(),
        "/srv/agentics/review-checkouts/agentics-challenges",
    ));
    paths.insert(context.path_value(
        |env| env.runner_runtime_root.as_ref(),
        "/srv/agentics/runtime",
    ));
    paths.insert(context.path_value(
        |env| env.runner_phase_mount_root.as_ref(),
        "/srv/agentics/phase-mounts",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_phase_mount_root.as_ref(),
        "/srv/agentics/phase-mounts",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_docker_data_root.as_ref(),
        "/srv/agentics/docker-data-root",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_exec_root.as_ref(),
        "/srv/agentics/docker-exec",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_pidfile.as_ref(),
        "/srv/agentics/docker.pid",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_log.as_ref(),
        "/srv/agentics/dockerd.log",
    ));
    if let Some(socket_path) = context.docker_socket_path() {
        paths.insert(PathBuf::from(socket_path));
    } else {
        paths.insert(PathBuf::from("/srv/agentics/docker.sock"));
    }

    let paths = paths.into_iter().collect::<Vec<_>>();
    if paths.is_empty() {
        return Err(ComposeProdError::InvalidConfig(
            "rehearsal purge resolved no cleanup paths".to_string(),
        ));
    }
    Ok(paths)
}

fn require_rehearsal_path(path: &Path) -> Result<(), ComposeProdError> {
    let rehearsal_root = Path::new(REHEARSAL_ROOT);
    if !path.is_absolute() {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path must be absolute, got {}",
            path.display()
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path must not contain parent traversal, got {}",
            path.display()
        )));
    }
    if !path.starts_with(rehearsal_root) {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path {} is outside {REHEARSAL_ROOT}",
            path.display()
        )));
    }
    Ok(())
}

fn collapse_cleanup_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = paths.to_vec();
    sorted.sort_by_key(|path| path.components().count());
    let mut collapsed = Vec::new();
    'path: for path in sorted {
        for parent in &collapsed {
            if path == *parent || path.starts_with(parent) {
                continue 'path;
            }
        }
        collapsed.push(path);
    }
    collapsed
}

async fn remove_rehearsal_path(path: &Path) -> Result<ReportLine, ComposeProdError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReportLine::skip(
                "rehearsal purge path",
                format!("{} did not exist", path.display()),
            ));
        }
        Err(error) => return Err(ComposeProdError::Process(error.to_string())),
    };
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    }
    Ok(ReportLine::pass(
        "rehearsal purge path",
        format!("removed {}", path.display()),
    ))
}
