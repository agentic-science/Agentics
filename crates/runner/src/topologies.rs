use super::{
    ChallengeBundleSpec, CoexecutedBenchmarkRequest, CoexecutedBenchmarkSetupRequest,
    ContainerRequest, EvaluationLogs, EvaluatorRequest, Path, PipedStdioRequest, Result,
    RetainedRunTree, RetainedRunnerTree, RunnerContext, ServiceError, SessionPlanRequest,
    SetupBuildRequest, SolutionRunRequest, WritablePhase, ZIP_PROJECT_MANIFEST_FILE,
    ZipProjectManifest, ZipProjectPhaseName, append_named_logs, append_phase_logs, append_run_logs,
    bind_mount, cleanup_paths, coexecuted_benchmark_setup, container_name, copy_dir_all,
    copy_evaluator_visible_run_tree, effective_accelerator_count, effective_phase_limits,
    ensure_container_succeeded, ensure_declared_outputs_exist, ensure_disk_limit,
    ensure_setup_disk_limit, evaluator_limits, include_log_excerpts, make_container_readable_tree,
    make_container_writable_tree, materialize_input_files, materialize_run_io, phase_name,
    replace_dir_all, replace_dir_all_if_separate, resolve_piped_stdio_session_plan, run_alias,
    run_coexecuted_benchmark_setup_phase, run_interface, validate_evaluator_visible_output_tree,
    visible_log_content, writable_phase_for_solution_phase, write_run_metadata,
};

/// Reads solution manifest from disk or storage.
pub(super) async fn read_solution_manifest(
    source_root: &Path,
    spec: &ChallengeBundleSpec,
) -> Result<ZipProjectManifest> {
    let manifest_path = source_root.join(spec.solution.manifest_file.as_path());
    let raw = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| {
            ServiceError::Validation(format!(
                "missing {ZIP_PROJECT_MANIFEST_FILE} in solution submission: {e}"
            ))
        })?;
    ZipProjectManifest::parse_json(&raw)
}

/// Handles run setup and build for this module.
pub(super) async fn run_setup_and_build(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    if runner.storage.uses_bounded_slots() {
        return run_setup_and_build_bounded(runner, request, logs).await;
    }

    cleanup_paths([request.build_root.to_path_buf()]).await?;
    copy_dir_all(request.source_root, request.build_root).await?;
    make_container_writable_tree(request.build_root).await?;

    for phase in request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
    {
        let limits = effective_phase_limits(request.profile, &phase)?;
        let cmd = vec!["sh".to_string(), format!("/workspace/{}", phase.command)];
        let outcome = runner
            .backend
            .run_container(ContainerRequest {
                name: container_name(runner.attempt, &format!("{:?}", phase.name).to_lowercase()),
                image: request
                    .profile
                    .solution_image
                    .docker_reference()
                    .to_string(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(request.build_root, "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels(phase_name(&phase.name), None),
            })
            .await?;
        append_phase_logs(
            logs,
            phase.name,
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            phase.name,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        ensure_disk_limit(request.build_root, limits.disk_limit_mb, phase.name).await?;
    }

    Ok(RetainedRunnerTree::runtime_path(request.build_root))
}

/// Handles run setup and build bounded for this module.
async fn run_setup_and_build_bounded(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let phases = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
        .collect::<Vec<_>>();

    if phases.is_empty() {
        replace_dir_all(request.source_root, request.build_root).await?;
        return Ok(RetainedRunnerTree::runtime_path(request.build_root));
    }

    let mut retained_workspace: Option<RetainedRunnerTree> = None;
    for phase in phases {
        let limits = effective_phase_limits(request.profile, &phase)?;
        let workspace = runner
            .storage
            .writable_mount(
                runner.docker,
                request.build_root,
                writable_phase_for_solution_phase(phase.name),
                limits.disk_limit_mb,
            )
            .await?;
        let source_workspace = retained_workspace
            .as_ref()
            .map(RetainedRunnerTree::path)
            .unwrap_or(request.source_root);
        copy_dir_all(source_workspace, workspace.path()).await?;
        make_container_writable_tree(workspace.path()).await?;

        let cmd = vec!["sh".to_string(), format!("/workspace/{}", phase.command)];
        let outcome = runner
            .backend
            .run_container(ContainerRequest {
                name: container_name(runner.attempt, &format!("{:?}", phase.name).to_lowercase()),
                image: request
                    .profile
                    .solution_image
                    .docker_reference()
                    .to_string(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(workspace.path(), "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels(phase_name(&phase.name), Some(&workspace)),
            })
            .await?;
        append_phase_logs(
            logs,
            phase.name,
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            phase.name,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        ensure_disk_limit(workspace.path(), limits.disk_limit_mb, phase.name).await?;
        retained_workspace = Some(RetainedRunnerTree::leased(workspace));
    }

    retained_workspace.ok_or_else(|| {
        ServiceError::Internal("setup/build phase list unexpectedly ended empty".to_string())
    })
}

/// Handles run solution invocations for this module.
pub(super) async fn run_solution_invocations(
    runner: RunnerContext<'_>,
    request: SolutionRunRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<Vec<RetainedRunTree>> {
    let run_phase = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .find(|phase| phase.name == ZipProjectPhaseName::Run)
        .ok_or_else(|| ServiceError::Runner("zip_project manifest has no run phase".to_string()))?;

    let mut retained_run_trees = Vec::with_capacity(request.run_manifest.runs.len());
    for (run_index, run) in request.run_manifest.runs.iter().enumerate() {
        let run_alias = run_alias(run_index)?;
        let solution_io_root = request.run_work_root.join(run_alias.as_str());
        let evaluator_run_root = request.runs_root.join(run.run_name.as_str());
        cleanup_paths([solution_io_root.clone(), evaluator_run_root.clone()]).await?;
        let limits = effective_phase_limits(request.profile, &run_phase)?;
        let io_mount = runner
            .storage
            .writable_mount(
                runner.docker,
                &solution_io_root,
                WritablePhase::SolutionRun,
                limits.disk_limit_mb,
            )
            .await?;
        let io_root = io_mount.path().to_path_buf();
        let input_dir = io_root.join("input");
        let output_dir = io_root.join("output");
        let tmp_dir = io_root.join("tmp");
        tokio::fs::create_dir_all(&input_dir).await?;
        tokio::fs::create_dir_all(&output_dir).await?;
        tokio::fs::create_dir_all(&tmp_dir).await?;
        materialize_run_io(
            run,
            run_alias.as_str(),
            request.eval_type,
            request.input_source_root,
            &io_root,
            &input_dir,
        )
        .await?;
        make_container_writable_tree(&io_root).await?;

        let outcome = runner.backend.run_container(
            ContainerRequest {
                name: container_name(runner.attempt, &format!("run-{run_alias}")),
                image: request.profile.solution_image.docker_reference().to_string(),
                cmd: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "mkdir -p /io/output /io/tmp; if [ -f /io/stdin.txt ]; then sh \"$1\" < /io/stdin.txt > /io/stdout.txt 2> /io/stderr.txt; else sh \"$1\" > /io/stdout.txt 2> /io/stderr.txt; fi"
                        .to_string(),
                    "agentics-run".to_string(),
                    format!("/workspace/{}", run_phase.command),
                ],
                env: vec![
                    "AGENTICS_PHASE=run".to_string(),
                    format!("AGENTICS_RUN_NAME={run_alias}"),
                    format!("AGENTICS_INTERFACE={}", run_interface(run.interface)),
                    "AGENTICS_INPUT_DIR=/io/input".to_string(),
                    "AGENTICS_OUTPUT_DIR=/io/output".to_string(),
                    "HOME=/io".to_string(),
                    "TMPDIR=/io/tmp".to_string(),
                    "PYTHONDONTWRITEBYTECODE=1".to_string(),
                ],
                mounts: vec![
                    bind_mount(request.build_root.path(), "/workspace", true),
                    bind_mount(&io_root, "/io", false),
                    bind_mount(&input_dir, "/io/input", true),
                ],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels("run", Some(&io_mount)),
            },
        )
        .await?;
        append_run_logs(
            logs,
            run_alias.as_str(),
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            ZipProjectPhaseName::Run,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        write_run_metadata(&io_root, run, run_alias.as_str(), &outcome).await?;
        ensure_disk_limit(&io_root, limits.disk_limit_mb, ZipProjectPhaseName::Run).await?;
        ensure_declared_outputs_exist(run, run_alias.as_str(), &output_dir).await?;
        if runner.storage.uses_bounded_slots() {
            validate_evaluator_visible_output_tree(
                &io_root,
                run_alias.as_str(),
                request.output_limits,
            )?;
            make_container_readable_tree(&io_root).await?;
            tokio::fs::create_dir_all(&evaluator_run_root).await?;
            retained_run_trees.push(RetainedRunTree {
                run_name: run.run_name.as_str().to_string(),
                tree: RetainedRunnerTree::leased(io_mount),
            });
        } else {
            copy_evaluator_visible_run_tree(
                &io_root,
                &evaluator_run_root,
                run_alias.as_str(),
                request.output_limits,
            )
            .await?;
            make_container_readable_tree(&evaluator_run_root).await?;
            cleanup_paths([solution_io_root]).await?;
        }
    }

    Ok(retained_run_trees)
}

/// Handles run evaluator for this module.
pub(super) async fn run_evaluator(
    runner: RunnerContext<'_>,
    request: EvaluatorRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    make_container_readable_tree(request.bundle_dir).await?;
    make_container_readable_tree(request.runs_root).await?;
    let limits = evaluator_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.evaluator_output_root,
            WritablePhase::EvaluatorScore,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(output_mount.path()).await?;

    let mut cmd = request.spec.execution.trusted_evaluator().command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--solution-runs-dir".to_string(),
        "/solution-runs".to_string(),
        "--output-path".to_string(),
        format!(
            "/output/{}",
            request.spec.execution.trusted_evaluator().result_file
        ),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--runs-file".to_string(),
        request.run_manifest_container_path.to_string(),
    ]);

    let mut mounts = vec![
        bind_mount(request.bundle_dir, "/challenge", true),
        bind_mount(request.runs_root, "/solution-runs", true),
        bind_mount(output_mount.path(), "/output", false),
    ];
    for run_tree in request.retained_run_trees {
        mounts.push(bind_mount(
            run_tree.tree.path(),
            &format!("/solution-runs/{}", run_tree.run_name),
            true,
        ));
    }
    if let Some(setup_root) = request.setup_root {
        mounts.push(bind_mount(setup_root, "/setup", true));
    }
    let outcome = runner
        .backend
        .run_container(ContainerRequest {
            name: container_name(runner.attempt, "separated-evaluator"),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec!["AGENTICS_PHASE=separated-evaluator".to_string()],
            mounts,
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            labels: runner.container_labels("separated-evaluator", Some(&output_mount)),
        })
        .await?;
    append_named_logs(
        logs,
        "separated-evaluator",
        visible_log_content(request.eval_type, &outcome.logs),
    );
    if outcome.timed_out || outcome.exit_code != 0 {
        return Err(ServiceError::Runner(format!(
            "separated-evaluator container failed: exit_code={}, timed_out={}",
            outcome.exit_code, outcome.timed_out
        )));
    }
    replace_dir_all_if_separate(output_mount.path(), request.evaluator_output_root).await?;

    Ok(())
}

/// Run the current single-session piped-stdio topology.
pub(super) async fn run_piped_stdio_session(
    runner: RunnerContext<'_>,
    request: PipedStdioRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    let run_phase = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .find(|phase| phase.name == ZipProjectPhaseName::Run)
        .ok_or_else(|| ServiceError::Runner("zip_project manifest has no run phase".to_string()))?;
    let session_plan = resolve_piped_stdio_session_plan(
        SessionPlanRequest {
            runner,
            spec: request.spec,
            profile: request.profile,
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            target: request.target,
            eval_type: request.eval_type,
            bundle_dir: request.bundle_dir,
            setup_root: request.setup_root,
        },
        logs,
    )
    .await?;

    cleanup_paths([request.session_root.to_path_buf()]).await?;
    let session_input_dir = request.session_root.join("input");
    tokio::fs::create_dir_all(&session_input_dir).await?;
    tokio::fs::write(
        request.session_root.join("session.json"),
        serde_json::to_vec_pretty(&session_plan.manifest).map_err(|e| {
            ServiceError::Internal(format!("serialize session manifest failed: {e}"))
        })?,
    )
    .await?;
    materialize_input_files(
        &session_plan.manifest.input_files,
        session_plan.manifest.session_name.as_str(),
        request.eval_type,
        &session_plan.input_source_root,
        &session_input_dir,
    )
    .await?;
    make_container_readable_tree(request.session_root).await?;
    make_container_readable_tree(request.bundle_dir).await?;

    let run_limits = effective_phase_limits(request.profile, &run_phase)?;
    cleanup_paths([request.run_work_root.to_path_buf()]).await?;
    let io_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.run_work_root,
            WritablePhase::SolutionRun,
            run_limits.disk_limit_mb,
        )
        .await?;
    let io_root = io_mount.path().to_path_buf();
    tokio::fs::create_dir_all(io_root.join("output")).await?;
    tokio::fs::create_dir_all(io_root.join("tmp")).await?;
    make_container_writable_tree(&io_root).await?;

    let evaluator_limits = evaluator_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.evaluator_output_root,
            WritablePhase::EvaluatorScore,
            evaluator_limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(output_mount.path()).await?;

    let mut interactive_evaluator_cmd = request.spec.execution.trusted_evaluator().command.clone();
    interactive_evaluator_cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--session-file".to_string(),
        "/session/session.json".to_string(),
        "--session-input-dir".to_string(),
        "/session/input".to_string(),
        "--output-path".to_string(),
        format!(
            "/output/{}",
            request.spec.execution.trusted_evaluator().result_file
        ),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
    ]);

    let mut interactive_evaluator_mounts = vec![
        bind_mount(request.bundle_dir, "/challenge", true),
        bind_mount(request.session_root, "/session", true),
        bind_mount(output_mount.path(), "/output", false),
    ];
    if let Some(setup_root) = session_plan
        .setup_root
        .as_ref()
        .map(RetainedRunnerTree::path)
    {
        interactive_evaluator_mounts.push(bind_mount(setup_root, "/setup", true));
    }

    let outcome = runner
        .backend
        .run_interactive_stdio_session(
            ContainerRequest {
                name: container_name(runner.attempt, "piped-participant"),
                image: request
                    .profile
                    .solution_image
                    .docker_reference()
                    .to_string(),
                cmd: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "mkdir -p /io/output /io/tmp; exec sh \"$1\"".to_string(),
                    "agentics-piped-run".to_string(),
                    format!("/workspace/{}", run_phase.command),
                ],
                env: vec![
                    "AGENTICS_PHASE=run".to_string(),
                    format!(
                        "AGENTICS_SESSION_NAME={}",
                        session_plan.manifest.session_name
                    ),
                    "AGENTICS_INTERFACE=piped_stdio".to_string(),
                    "AGENTICS_OUTPUT_DIR=/io/output".to_string(),
                    "HOME=/io".to_string(),
                    "TMPDIR=/io/tmp".to_string(),
                    "PYTHONDONTWRITEBYTECODE=1".to_string(),
                ],
                mounts: vec![
                    bind_mount(request.build_root.path(), "/workspace", true),
                    bind_mount(&io_root, "/io", false),
                ],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: run_limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&run_limits),
                labels: runner.container_labels("run", Some(&io_mount)),
            },
            ContainerRequest {
                name: container_name(runner.attempt, "interactive-evaluator"),
                image: request
                    .profile
                    .evaluator_image
                    .docker_reference()
                    .to_string(),
                cmd: interactive_evaluator_cmd,
                env: vec![
                    "AGENTICS_PHASE=interactive-evaluator".to_string(),
                    format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
                ],
                mounts: interactive_evaluator_mounts,
                working_dir: "/challenge".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: evaluator_limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&evaluator_limits),
                labels: runner.container_labels("interactive-evaluator", Some(&output_mount)),
            },
            request.max_interaction_bytes_per_direction,
            request.interaction_shutdown_grace_secs,
        )
        .await?;

    append_named_logs(
        logs,
        "participant",
        visible_log_content(request.eval_type, &outcome.participant.logs),
    );
    append_named_logs(
        logs,
        "interactive-evaluator",
        visible_log_content(request.eval_type, &outcome.interactive_evaluator.logs),
    );
    if outcome.participant.timed_out || outcome.participant.exit_code != 0 {
        return Err(ServiceError::Runner(format!(
            "participant container failed: exit_code={}, timed_out={}",
            outcome.participant.exit_code, outcome.participant.timed_out
        )));
    }
    if outcome.interactive_evaluator.timed_out || outcome.interactive_evaluator.exit_code != 0 {
        return Err(ServiceError::Runner(format!(
            "interactive-evaluator container failed: exit_code={}, timed_out={}",
            outcome.interactive_evaluator.exit_code, outcome.interactive_evaluator.timed_out
        )));
    }
    ensure_disk_limit(&io_root, run_limits.disk_limit_mb, ZipProjectPhaseName::Run).await?;
    ensure_setup_disk_limit(output_mount.path(), evaluator_limits.disk_limit_mb).await?;
    replace_dir_all_if_separate(output_mount.path(), request.evaluator_output_root).await?;

    Ok(())
}

/// Run the coexecuted-evaluator topology.
pub(super) async fn run_coexecuted_benchmark(
    runner: RunnerContext<'_>,
    request: CoexecutedBenchmarkRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    let execution = request
        .spec
        .execution
        .coexecuted_benchmark()
        .ok_or_else(|| {
            ServiceError::Runner("challenge execution is not coexecuted_benchmark".to_string())
        })?;
    let retained_setup_root =
        if let Some(setup) = coexecuted_benchmark_setup(execution, request.eval_type) {
            Some(
                run_coexecuted_benchmark_setup_phase(
                    CoexecutedBenchmarkSetupRequest {
                        runner,
                        profile: request.profile,
                        docker_platform: request.docker_platform,
                        accelerator: request.accelerator,
                        target: request.target,
                        eval_type: request.eval_type,
                        setup,
                        bundle_dir: request.bundle_dir,
                        setup_root: request.setup_root,
                    },
                    logs,
                )
                .await?,
            )
        } else {
            None
        };

    make_container_readable_tree(request.bundle_dir).await?;
    make_container_readable_tree(request.build_root.path()).await?;
    let limits = evaluator_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.evaluator_output_root,
            WritablePhase::EvaluatorScore,
            limits.disk_limit_mb,
        )
        .await?;
    tokio::fs::create_dir_all(output_mount.path().join("tmp")).await?;
    make_container_writable_tree(output_mount.path()).await?;

    let mut cmd = execution.coexecuted_evaluator.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--workspace-dir".to_string(),
        "/workspace".to_string(),
        "--output-path".to_string(),
        format!("/output/{}", execution.coexecuted_evaluator.result_file),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
    ]);
    if retained_setup_root.is_some() {
        cmd.extend(["--setup-dir".to_string(), "/setup".to_string()]);
    }

    let mut mounts = vec![
        bind_mount(request.bundle_dir, "/challenge", true),
        bind_mount(request.build_root.path(), "/workspace", true),
        bind_mount(output_mount.path(), "/output", false),
    ];
    if let Some(setup_root) = retained_setup_root.as_ref().map(RetainedRunnerTree::path) {
        mounts.push(bind_mount(setup_root, "/setup", true));
    }

    let outcome = runner
        .backend
        .run_container(ContainerRequest {
            name: container_name(runner.attempt, "coexecuted-evaluator"),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=coexecuted-evaluator".to_string(),
                "AGENTICS_EXECUTION_MODE=coexecuted_benchmark".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
                "AGENTICS_OUTPUT_DIR=/output".to_string(),
                "HOME=/output".to_string(),
                "TMPDIR=/output/tmp".to_string(),
                "PYTHONDONTWRITEBYTECODE=1".to_string(),
            ],
            mounts,
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            labels: runner.container_labels("coexecuted-evaluator", Some(&output_mount)),
        })
        .await?;
    append_named_logs(
        logs,
        "coexecuted-evaluator",
        visible_log_content(request.eval_type, &outcome.logs),
    );
    if outcome.timed_out || outcome.exit_code != 0 {
        return Err(ServiceError::Runner(format!(
            "coexecuted-evaluator container failed: exit_code={}, timed_out={}",
            outcome.exit_code, outcome.timed_out
        )));
    }
    ensure_setup_disk_limit(output_mount.path(), limits.disk_limit_mb).await?;
    replace_dir_all_if_separate(output_mount.path(), request.evaluator_output_root).await?;

    Ok(())
}
