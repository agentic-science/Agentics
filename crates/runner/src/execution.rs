use super::topologies::{
    read_solution_manifest, run_coexecuted_benchmark, run_evaluator, run_piped_stdio_session,
    run_setup_and_build, run_solution_invocations,
};
use super::{
    ChallengeExecutionSpec, CoexecutedBenchmarkRequest, EVALUATION_LOG_BYTES_PER_RUN,
    EvaluationJobExecution, EvaluationLimitConfig, EvaluationLogs, EvaluatorRequest,
    EvaluatorRunResult, ExecutionResult, JobRequirement, OutputTreeLimits, PipedStdioRequest,
    Result, RetainedRunnerTree, RunPlanRequest, RunnerAttempt, RunnerContext, RunnerStorage,
    ScoringMode, ServiceError, SetupBuildRequest, SolutionRunRequest, append_named_logs,
    cleanup_paths, configure_run_count_limits, create_private_host_dir, evaluation_runner_log_key,
    extract_zip_safe, make_container_readable_tree, read_limited_result_json, resolve_run_plan,
    sanitize_runner_error, validate_evaluator_result, visible_log_content,
};

/// Execute one evaluation job in Docker and return the validated evaluator result.
pub async fn execute_evaluation_job(
    request: EvaluationJobExecution<'_>,
) -> Result<ExecutionResult> {
    let EvaluationJobExecution {
        docker,
        config,
        job_id,
        worker_id,
        attempt_count,
        container_scope,
        eval_type,
        payload,
        storage,
    } = request;
    let attempt = RunnerAttempt::new(job_id, worker_id, attempt_count);
    let runner_runtime_root = config
        .runner_runtime_root()
        .map_err(|error| ServiceError::Runner(error.to_string()))?;
    let artifact_root = runner_runtime_root.join("agentics-eval-artifacts");
    let working_root = artifact_root.join(&attempt.transient_name);
    let source_root = working_root.join("source");
    let build_root = working_root.join("build-workspace");
    let run_work_root = working_root.join("solution-run-work");
    let runs_root = working_root.join("solution-runs");
    let setup_root = working_root.join("setup");
    let session_root = working_root.join("session");
    let evaluator_output_root = working_root.join("evaluator-output");
    let challenge_bundle_root = working_root.join("challenge-bundle");
    let log_key = evaluation_runner_log_key(job_id, attempt_count)?;

    create_private_host_dir(&artifact_root).await?;
    cleanup_paths([working_root.clone()]).await?;
    create_private_host_dir(&working_root).await?;
    tokio::fs::create_dir_all(&source_root).await?;
    tokio::fs::create_dir_all(&build_root).await?;
    tokio::fs::create_dir_all(&run_work_root).await?;
    tokio::fs::create_dir_all(&runs_root).await?;
    tokio::fs::create_dir_all(&session_root).await?;
    tokio::fs::create_dir_all(&evaluator_output_root).await?;

    let limits = EvaluationLimitConfig {
        max_runs: config.runner.max_runs,
        max_result_json_bytes: config.runner.max_result_json_bytes,
        max_public_results: config.runner.max_public_results,
        max_result_log_bytes: config.runner.max_result_log_bytes,
    };
    let max_log_bytes = EVALUATION_LOG_BYTES_PER_RUN
        .checked_mul(limits.max_runs)
        .ok_or_else(|| ServiceError::Runner("evaluation log limit overflow".to_string()))?;
    let mut logs = EvaluationLogs::new(max_log_bytes);
    let docker_backend = super::DockerRunnerBackend::new(docker, &config.runner.namespace);
    let runner_storage = RunnerStorage::from_config(config)?;
    let output_limits = OutputTreeLimits {
        max_files: config.runner.max_output_files,
        max_dirs: config.runner.max_output_dirs,
        max_depth: config.runner.max_output_depth,
    };
    let runner_context = RunnerContext {
        docker,
        backend: &docker_backend,
        storage: &runner_storage,
        runner_namespace: &config.runner.namespace,
        job_id,
        attempt: &attempt,
        container_scope,
    };

    let execution = async {
        let bundle_key = match eval_type {
            ScoringMode::Validation => &payload.public_bundle_key,
            ScoringMode::Official => &payload.bundle_key,
        };
        let bundle_archive_path = working_root.join("challenge-bundle.tar");
        storage
            .get_to_file(
                bundle_key,
                &bundle_archive_path,
                agentics_storage::StorageWriteIntent::new(
                    "challenge bundle archive",
                    config.storage.max_bundle_archive_bytes,
                ),
            )
            .await?;
        agentics_storage::unpack_tar_to_directory(&bundle_archive_path, &challenge_bundle_root)
            .await?;
        make_container_readable_tree(&challenge_bundle_root).await?;
        let bundle_dir = challenge_bundle_root.as_path();
        let spec =
            agentics_contracts::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
        if config.requires_digest_pinned_images() {
            agentics_contracts::challenge_bundle::validate_digest_pinned_images(&spec)?;
        }
        let result_path =
            evaluator_output_root.join(spec.execution.trusted_evaluator().result_file.as_path());

        let target = spec.target(&payload.target).ok_or_else(|| {
            ServiceError::Runner(format!(
                "challenge contract does not declare target `{}`",
                payload.target
            ))
        })?;
        let job_requirement = JobRequirement::new(target.docker_platform, target.accelerator);
        let profile = &target.resource_profile;
        runner_context
            .backend
            .pre_pull_image(
                profile.solution_image.docker_reference(),
                job_requirement.docker_platform,
            )
            .await?;
        runner_context
            .backend
            .pre_pull_image(
                profile.evaluator_image.docker_reference(),
                job_requirement.docker_platform,
            )
            .await?;

        let artifact_bytes = storage
            .get(
                &payload.artifact_key,
                agentics_storage::StorageWriteIntent::new(
                    "solution artifact ZIP",
                    agentics_contracts::zip_project::MAX_ZIP_PROJECT_ARTIFACT_BYTES,
                ),
            )
            .await?;
        let artifact_path = working_root.join("solution.zip");
        tokio::fs::write(&artifact_path, artifact_bytes).await?;
        extract_zip_safe(&artifact_path, &source_root).await?;
        let manifest = read_solution_manifest(&source_root, &spec).await?;
        let build_workspace = run_setup_and_build(
            runner_context,
            SetupBuildRequest {
                eval_type,
                profile,
                docker_platform: job_requirement.docker_platform,
                accelerator: job_requirement.accelerator,
                manifest: &manifest,
                source_root: &source_root,
                build_root: &build_root,
            },
            &mut logs,
        )
        .await?;

        match &spec.execution {
            ChallengeExecutionSpec::SeparatedEvaluator(_) => {
                let run_plan = resolve_run_plan(
                    RunPlanRequest {
                        runner: runner_context,
                        spec: &spec,
                        profile,
                        docker_platform: job_requirement.docker_platform,
                        accelerator: job_requirement.accelerator,
                        target: target.name.as_str(),
                        eval_type,
                        bundle_dir,
                        setup_root: &setup_root,
                    },
                    &mut logs,
                )
                .await?;
                configure_run_count_limits(&run_plan.manifest, limits, &mut logs)?;
                let retained_run_trees = run_solution_invocations(
                    runner_context,
                    SolutionRunRequest {
                        eval_type,
                        profile,
                        docker_platform: job_requirement.docker_platform,
                        accelerator: job_requirement.accelerator,
                        manifest: &manifest,
                        run_manifest: &run_plan.manifest,
                        input_source_root: &run_plan.input_source_root,
                        build_root: &build_workspace,
                        run_work_root: &run_work_root,
                        runs_root: &runs_root,
                        output_limits,
                    },
                    &mut logs,
                )
                .await?;

                run_evaluator(
                    runner_context,
                    EvaluatorRequest {
                        eval_type,
                        spec: &spec,
                        profile,
                        docker_platform: job_requirement.docker_platform,
                        accelerator: job_requirement.accelerator,
                        run_manifest_container_path: &run_plan.run_manifest_container_path,
                        bundle_dir,
                        setup_root: run_plan.setup_root.as_ref().map(RetainedRunnerTree::path),
                        runs_root: &runs_root,
                        retained_run_trees: &retained_run_trees,
                        evaluator_output_root: &evaluator_output_root,
                    },
                    &mut logs,
                )
                .await?;
            }
            ChallengeExecutionSpec::PipedStdio(_) => {
                logs.set_limit(EVALUATION_LOG_BYTES_PER_RUN);
                run_piped_stdio_session(
                    runner_context,
                    PipedStdioRequest {
                        eval_type,
                        spec: &spec,
                        profile,
                        docker_platform: job_requirement.docker_platform,
                        accelerator: job_requirement.accelerator,
                        target: target.name.as_str(),
                        manifest: &manifest,
                        bundle_dir,
                        setup_root: &setup_root,
                        session_root: &session_root,
                        build_root: &build_workspace,
                        run_work_root: &run_work_root,
                        evaluator_output_root: &evaluator_output_root,
                        max_interaction_bytes_per_direction: config
                            .runner
                            .max_interaction_bytes_per_direction,
                        interaction_shutdown_grace_secs: config
                            .runner
                            .interaction_shutdown_grace_secs,
                    },
                    &mut logs,
                )
                .await?;
            }
            ChallengeExecutionSpec::CoexecutedBenchmark(_) => {
                logs.set_limit(EVALUATION_LOG_BYTES_PER_RUN);
                run_coexecuted_benchmark(
                    runner_context,
                    CoexecutedBenchmarkRequest {
                        eval_type,
                        spec: &spec,
                        profile,
                        docker_platform: job_requirement.docker_platform,
                        accelerator: job_requirement.accelerator,
                        target: target.name.as_str(),
                        bundle_dir,
                        setup_root: &setup_root,
                        build_root: &build_workspace,
                        evaluator_output_root: &evaluator_output_root,
                    },
                    &mut logs,
                )
                .await?;
            }
        }

        let result_raw =
            read_limited_result_json(&result_path, limits.max_result_json_bytes).await?;
        let mut result: EvaluatorRunResult = serde_json::from_str(&result_raw)
            .map_err(|e| ServiceError::Runner(format!("invalid result.json: {e}")))?;
        validate_evaluator_result(&mut result, eval_type, &spec.metric_schema, limits)?;
        if !result.logs.is_empty() {
            let result_logs = result.logs.join("\n");
            append_named_logs(
                &mut logs,
                "evaluator:result.logs",
                visible_log_content(eval_type, &result_logs),
            );
        }

        Ok(ExecutionResult {
            result,
            log_key: log_key.clone(),
        })
    }
    .await;

    let log_write = storage
        .put(
            &log_key,
            logs.as_bytes(),
            agentics_storage::StorageWriteIntent::new("runner log", max_log_bytes),
        )
        .await;
    let cleanup = cleanup_paths([working_root]).await;
    match (execution, log_write, cleanup) {
        (Ok(result), Ok(_), Ok(())) => Ok(result),
        (Ok(_), Err(log_err), Ok(())) => Err(log_err.into()),
        (Ok(_), Ok(_), Err(cleanup_err)) => Err(cleanup_err),
        (Ok(_), Err(log_err), Err(cleanup_err)) => Err(ServiceError::Runner(format!(
            "{log_err}; additionally failed to clean runner workspace: {cleanup_err}"
        ))),
        (Err(run_err), _, Ok(())) => Err(sanitize_runner_error(eval_type, run_err)),
        (Err(run_err), _, Err(cleanup_err)) => Err(ServiceError::Runner(format!(
            "{}; additionally failed to clean runner workspace: {cleanup_err}",
            sanitize_runner_error(eval_type, run_err)
        ))),
    }
}
