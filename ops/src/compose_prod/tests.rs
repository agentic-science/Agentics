use super::{
    Cli, ComposeContext, ComposeProdError, DEFAULT_PROJECT, ProdCommand, REHEARSAL_PROJECT,
    RawComposeProdEnv, RunnerDownPolicy, build_rehearsal_purge_plan, down, env_value,
    private_bundle_restore_args, resolve_project, run, unavailable_runner_cleanup_reports,
};
use agentics_config::RunnerNamespace;
use clap::ValueEnum;
use std::path::PathBuf;

/// Verifies production down cannot silently choose a runner policy.
#[tokio::test]
async fn down_requires_runner_policy() {
    let context = fake_context();
    let error = down(&context, None, true)
        .await
        .expect_err("missing policy should fail");
    assert!(matches!(error, ComposeProdError::MissingRunnerPolicy));
}

/// Verifies the public down command reports the missing runner policy before env-file setup.
#[tokio::test]
async fn run_down_requires_runner_before_env_file() {
    let error = run(Cli {
        env_file: Some(PathBuf::from("/tmp/agentics-missing-prod.env")),
        project: None,
        command: ProdCommand::Down {
            runner: None,
            dry_run: true,
        },
    })
    .await
    .expect_err("missing policy should fail before env lookup");
    assert!(matches!(error, ComposeProdError::MissingRunnerPolicy));
}

/// Verifies dry-run policy selection is explicit and non-mutating.
#[test]
fn runner_down_policy_parses_only_named_values() {
    assert_eq!(
        RunnerDownPolicy::from_str("keep", true).expect("keep parses"),
        RunnerDownPolicy::Keep
    );
    assert_eq!(
        RunnerDownPolicy::from_str("clean", true).expect("clean parses"),
        RunnerDownPolicy::Clean
    );
    assert!(RunnerDownPolicy::from_str("delete", true).is_err());
}

/// Verifies env file values can provide the project default.
#[test]
fn project_resolves_from_env_file_or_default() {
    let process_env = RawComposeProdEnv::default();
    let mut file_env = RawComposeProdEnv::default();
    assert_eq!(
        resolve_project(None, &process_env, &file_env),
        DEFAULT_PROJECT
    );
    file_env.compose_prod_project = Some("custom-prod".to_string());
    assert_eq!(
        resolve_project(None, &process_env, &file_env),
        "custom-prod"
    );
    assert_eq!(
        resolve_project(Some("cli-prod"), &process_env, &file_env),
        "cli-prod"
    );
    assert_eq!(
        env_value(None, file_env.compose_prod_project.as_ref()).as_deref(),
        Some("custom-prod")
    );
}

/// Verifies rehearsal purge dry-run still requires the explicit rehearsal stage marker.
#[test]
fn rehearsal_purge_refuses_missing_env_marker() {
    let mut context = fake_context();
    context.project = REHEARSAL_PROJECT.to_string();
    context.file_env.runner_namespace = Some(REHEARSAL_PROJECT.to_string());
    context.file_env.dgx_state_root = Some("/srv/agentics-rehearsal".to_string());
    let error =
        build_rehearsal_purge_plan(&context, false, true).expect_err("missing marker should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("AGENTICS_DEPLOYMENT_STAGE"))
    );
}

/// Verifies rehearsal purge never accepts the production project.
#[test]
fn rehearsal_purge_refuses_production_project() {
    let mut context = fake_context();
    context.file_env.deployment_stage = Some("rehearsal".to_string());
    context.file_env.runner_namespace = Some(REHEARSAL_PROJECT.to_string());
    context.file_env.dgx_state_root = Some("/srv/agentics-rehearsal".to_string());
    let error = build_rehearsal_purge_plan(&context, true, false)
        .expect_err("production project should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("refusing to purge production"))
    );
}

/// Verifies destructive rehearsal purge requires an explicit confirmation flag.
#[test]
fn rehearsal_purge_requires_confirm_for_destructive_run() {
    let context = rehearsal_context();
    let error = build_rehearsal_purge_plan(&context, false, false)
        .expect_err("missing confirmation should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("--confirm-rehearsal-purge"))
    );
}

/// Verifies purge guardrails reject even one production-rooted path.
#[test]
fn rehearsal_purge_refuses_paths_outside_rehearsal_root() {
    let mut context = rehearsal_context();
    context.file_env.runner_runtime_root = Some("/srv/agentics/runtime".to_string());
    let error =
        build_rehearsal_purge_plan(&context, true, false).expect_err("production path should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("outside /srv/agentics-rehearsal"))
    );
}

/// Verifies dry-run plans are complete and non-mutating.
#[test]
fn rehearsal_purge_dry_run_reports_resources_and_paths() {
    let context = rehearsal_context();
    let plan = build_rehearsal_purge_plan(&context, false, true).expect("dry-run plan");
    assert_eq!(plan.namespace.as_str(), REHEARSAL_PROJECT);
    assert!(
        plan.reported_paths
            .iter()
            .any(|path| path == &PathBuf::from("/srv/agentics-rehearsal/docker.sock"))
    );
    let reports = plan.dry_run_reports();
    assert!(
        reports
            .iter()
            .any(|report| format!("{report:?}").contains("Compose project"))
    );
}

/// Verifies a partially completed purge can be retried after the runner daemon is gone.
#[test]
fn rehearsal_purge_skips_runner_cleanup_when_socket_is_missing() {
    let mut context = rehearsal_context();
    let missing_socket = std::env::temp_dir().join(format!(
        "agentics-missing-rehearsal-runner-{}-{}.sock",
        std::process::id(),
        line!()
    ));
    let _ignored = std::fs::remove_file(&missing_socket);
    context.file_env.docker_socket_path = Some(missing_socket.display().to_string());

    let reports = unavailable_runner_cleanup_reports(
        &context,
        &RunnerNamespace::try_new(REHEARSAL_PROJECT.to_string()).expect("valid namespace"),
    )
    .expect("missing socket should skip runner cleanup");

    assert!(
        reports
            .iter()
            .any(|report| format!("{report:?}").contains("assuming daemon already stopped"))
    );
}

/// Verifies only the committed rehearsal env file marker adds the rehearsal Compose override.
#[test]
fn rehearsal_override_comes_from_env_file_marker() {
    let mut context = fake_context();
    context.process_env.deployment_stage = Some("rehearsal".to_string());
    assert!(
        !compose_args_text(&context).contains("compose.rehearsal.yml"),
        "process env alone must not turn production commands into rehearsal commands"
    );

    context.file_env.deployment_stage = Some("rehearsal".to_string());
    assert!(compose_args_text(&context).contains("compose.rehearsal.yml"));
}

/// Verifies production PostgreSQL 18 adds the explicit cutover override file.
#[test]
fn production_postgres18_adds_explicit_override() {
    let mut context = fake_context();
    context.file_env.deployment_stage = Some("production".to_string());
    context.file_env.postgres_image = Some("postgres:18-alpine".to_string());
    context.file_env.postgres_volume = Some("postgres_data_pg18".to_string());
    context.file_env.postgres_data_mount = Some("/var/lib/postgresql".to_string());

    assert!(compose_args_text(&context).contains("compose.prod-postgres18.yml"));
    context
        .validate_postgres_cutover_guardrails()
        .expect("fresh PG18 volume should be accepted");
}

/// Verifies production PostgreSQL 18 refuses to reuse the old PG16 volume.
#[test]
fn production_postgres18_refuses_default_volume() {
    let mut context = fake_context();
    context.file_env.deployment_stage = Some("production".to_string());
    context.file_env.postgres_image = Some("postgres:18-alpine".to_string());
    context.file_env.postgres_volume = Some("postgres_data".to_string());
    context.file_env.postgres_data_mount = Some("/var/lib/postgresql".to_string());

    let error = context
        .validate_postgres_cutover_guardrails()
        .expect_err("old volume should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("must not use old default"))
    );
}

/// Verifies production PostgreSQL 18 requires the PG18 Docker data mount root.
#[test]
fn production_postgres18_requires_pg18_mount_root() {
    let mut context = fake_context();
    context.file_env.deployment_stage = Some("production".to_string());
    context.file_env.postgres_image = Some("postgres:18-alpine".to_string());
    context.file_env.postgres_volume = Some("postgres_data_pg18".to_string());
    context.file_env.postgres_data_mount = Some("/var/lib/postgresql/data".to_string());

    let error = context
        .validate_postgres_cutover_guardrails()
        .expect_err("old mount should fail");
    assert!(
        matches!(error, ComposeProdError::InvalidConfig(message) if message.contains("AGENTICS_POSTGRES_DATA_MOUNT"))
    );
}

/// Verifies restore-private-bundles passes explicit refresh flags to the copy tool.
#[test]
fn private_bundle_restore_args_forward_overwrite_and_dry_run() {
    let context = rehearsal_context();
    let args = private_bundle_restore_args(&context, true, true)
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        vec![
            "run",
            "--rm",
            "private-bundle-restore",
            "/usr/local/bin/agentics-copy-private-bundle-backups",
            "--work-dir",
            "/srv/agentics-rehearsal/storage-work/private-bundle-backup-copy",
            "--overwrite",
            "--dry-run"
        ]
    );
}

fn rehearsal_context() -> ComposeContext {
    let mut context = fake_context();
    context.env_file = PathBuf::from("/tmp/agentics-test/rehearsal.env");
    context.project = REHEARSAL_PROJECT.to_string();
    context.file_env.deployment_stage = Some("rehearsal".to_string());
    context.file_env.runner_namespace = Some(REHEARSAL_PROJECT.to_string());
    context.file_env.dgx_state_root = Some("/srv/agentics-rehearsal".to_string());
    context.file_env.storage_work_root = Some("/srv/agentics-rehearsal/storage-work".to_string());
    context.file_env.challenge_review_repository_host_root =
        Some("/srv/agentics-rehearsal/review-checkouts/agentics-challenges".to_string());
    context.file_env.runner_runtime_root = Some("/srv/agentics-rehearsal/runtime".to_string());
    context.file_env.runner_phase_mount_root =
        Some("/srv/agentics-rehearsal/phase-mounts".to_string());
    context.file_env.dgx_phase_mount_root =
        Some("/srv/agentics-rehearsal/phase-mounts".to_string());
    context.file_env.dgx_docker_data_root =
        Some("/srv/agentics-rehearsal/docker-data-root".to_string());
    context.file_env.dgx_runner_docker_exec_root =
        Some("/srv/agentics-rehearsal/docker-exec".to_string());
    context.file_env.dgx_runner_docker_pidfile =
        Some("/srv/agentics-rehearsal/docker.pid".to_string());
    context.file_env.dgx_runner_docker_log =
        Some("/srv/agentics-rehearsal/dockerd.log".to_string());
    context.file_env.docker_socket_path = Some("/srv/agentics-rehearsal/docker.sock".to_string());
    context
}

fn compose_args_text(context: &ComposeContext) -> String {
    context
        .compose_args(["ps"])
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn fake_context() -> ComposeContext {
    ComposeContext {
        repo_root: PathBuf::from("/tmp/agentics-test"),
        env_file: PathBuf::from("/tmp/agentics-test/prod.env"),
        process_env: RawComposeProdEnv::default(),
        file_env: RawComposeProdEnv::default(),
        project: DEFAULT_PROJECT.to_string(),
    }
}
