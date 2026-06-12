use super::*;

#[tokio::test]
async fn disabled_private_benchmark_bundle_does_not_require_directory() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-disabled-private-benchmark-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    create_bundle(&root, &spec);

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(result.is_ok());
}

/// Verifies that source backed run inputs must exist under bundle root.
#[tokio::test]
async fn source_backed_run_inputs_must_exist_under_bundle_root() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-source-input-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"public-1","interface":"file_system","stdin_json":null,"stdin_text":null,"input_files":[{"path":"input.txt","source_path":"public/input.txt"}],"output_files":["answer.txt"],"metadata":null}]}"#,
    )
    .expect("failed to write source-backed runs");

    let missing_result = validate_challenge_bundle(&root).await;
    std::fs::write(root.join("public/input.txt"), "payload\n")
        .expect("failed to write source input");
    let present_result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(missing_result.is_err());
    assert!(present_result.is_ok());
}

/// Verifies source-backed session inputs are validated under the selected source root.
#[tokio::test]
async fn source_backed_session_inputs_must_exist_under_bundle_root() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-source-session-input-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_piped_stdio_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.official_session = None;
    }
    create_piped_stdio_bundle(&root, &spec);
    std::fs::remove_file(root.join("public/prompt.txt")).expect("failed to remove source input");

    let missing_result = validate_challenge_bundle(&root).await;
    std::fs::write(root.join("public/prompt.txt"), "payload\n")
        .expect("failed to restore source input");
    let present_result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(missing_result.is_err());
    assert!(present_result.is_ok());
}

/// Verifies session manifests reject duplicate materialized input paths.
#[tokio::test]
async fn session_manifest_rejects_duplicate_input_paths() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-duplicate-session-input-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_piped_stdio_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.official_session = None;
    }
    create_piped_stdio_bundle(&root, &spec);
    std::fs::write(
        root.join("public/session.json"),
        r#"{"session_name":"public-1","input_files":[{"path":"prompt.txt","content":"a"},{"path":"prompt.txt","content":"b"}],"metadata":null}"#,
    )
    .expect("failed to write duplicate session inputs");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("duplicate session input paths should fail");
    assert!(error.to_string().contains("duplicate path"));
}

/// Verifies separated-evaluator run manifests reject duplicate materialized input paths.
#[tokio::test]
async fn run_manifest_rejects_duplicate_input_paths() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-duplicate-run-input-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"public-1","interface":"file_system","stdin_json":null,"stdin_text":null,"input_files":[{"path":"prompt.txt","content":"a"},{"path":"prompt.txt","content":"b"}],"output_files":null,"metadata":null}]}"#,
    )
    .expect("failed to write duplicate run inputs");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("duplicate run input paths should fail");
    assert!(error.to_string().contains("duplicate path"));
}

/// Verifies separated-evaluator run manifests require nullable fields to be present.
#[tokio::test]
async fn run_manifest_requires_nullable_fields_to_be_present() {
    for field in [
        "stdin_json",
        "stdin_text",
        "input_files",
        "output_files",
        "metadata",
    ] {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-missing-run-field-{field}-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        create_bundle(&root, &spec);
        let mut run = serde_json::json!({
            "run_name": "public-1",
            "interface": "stdio",
            "stdin_json": null,
            "stdin_text": "1",
            "input_files": null,
            "output_files": null,
            "metadata": null
        });
        run.as_object_mut()
            .expect("run should be an object")
            .remove(field);
        std::fs::write(
            root.join("public/runs.json"),
            serde_json::json!({ "runs": [run] }).to_string(),
        )
        .expect("failed to write incomplete run manifest");

        let result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        let error = result.expect_err("missing required nullable run field should fail");
        assert!(
            error.to_string().contains(field),
            "unexpected error for {field}: {error}"
        );
    }
}

/// Verifies run manifests use null, not empty arrays, for absent input or output declarations.
#[tokio::test]
async fn run_manifest_rejects_empty_nullable_arrays() {
    for (field, message) in [
        ("input_files", "empty arrays must use null"),
        ("output_files", "empty arrays must use null"),
    ] {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-empty-run-array-{field}-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        create_bundle(&root, &spec);
        let mut run = serde_json::json!({
            "run_name": "public-1",
            "interface": "stdio",
            "stdin_json": null,
            "stdin_text": "1",
            "input_files": null,
            "output_files": null,
            "metadata": null
        });
        run[field] = serde_json::json!([]);
        std::fs::write(
            root.join("public/runs.json"),
            serde_json::json!({ "runs": [run] }).to_string(),
        )
        .expect("failed to write run manifest with empty array");

        let result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        let error = result.expect_err("empty nullable run arrays should fail");
        assert!(
            error.to_string().contains(message) || error.to_string().contains("non-empty array"),
            "unexpected error for {field}: {error}"
        );
    }
}

/// Verifies run metadata cannot shadow evaluator-visible reserved run fields.
#[tokio::test]
async fn run_manifest_metadata_cannot_shadow_reserved_fields() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-reserved-run-metadata-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"public-1","interface":"stdio","stdin_json":null,"stdin_text":"1","input_files":null,"output_files":null,"metadata":{"run_name":"shadow"}}]}"#,
    )
    .expect("failed to write reserved run metadata");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("reserved metadata key should fail");
    assert!(error.to_string().contains("run_name"));
}

/// Verifies run names cannot collide with the runner-generated evaluator manifest.
#[tokio::test]
async fn run_manifest_rejects_generated_evaluator_manifest_name() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-reserved-run-name-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"agentics-runs.json","interface":"stdio","stdin_json":null,"stdin_text":"1","input_files":null,"output_files":null,"metadata":null}]}"#,
    )
    .expect("failed to write reserved run name");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("reserved run_name should fail");
    assert!(error.to_string().contains("runner metadata"));
}

/// Verifies session metadata must be an object when present.
#[tokio::test]
async fn session_manifest_rejects_non_object_metadata() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-session-metadata-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_piped_stdio_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.official_session = None;
    }
    create_piped_stdio_bundle(&root, &spec);
    std::fs::write(
        root.join("public/session.json"),
        r#"{"session_name":"public-1","input_files":null,"metadata":["not","object"]}"#,
    )
    .expect("failed to write invalid session metadata");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("non-object metadata should fail");
    assert!(error.to_string().contains("invalid session manifest"));
}

/// Verifies piped-stdio session manifests require nullable fields to be present.
#[tokio::test]
async fn session_manifest_requires_nullable_fields_to_be_present() {
    for field in ["input_files", "metadata"] {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-missing-session-field-{field}-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_piped_stdio_spec();
        spec.datasets.private_benchmark_enabled = false;
        spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
        if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
            execution.official_session = None;
        }
        create_piped_stdio_bundle(&root, &spec);
        let mut session = serde_json::json!({
            "session_name": "public-1",
            "input_files": null,
            "metadata": null
        });
        session
            .as_object_mut()
            .expect("session should be an object")
            .remove(field);
        std::fs::write(root.join("public/session.json"), session.to_string())
            .expect("failed to write incomplete session manifest");

        let result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        let error = result.expect_err("missing required nullable session field should fail");
        assert!(
            error.to_string().contains("invalid session manifest"),
            "unexpected error for {field}: {error}"
        );
    }
}

/// Verifies session manifests use null, not an empty array, for absent input files.
#[tokio::test]
async fn session_manifest_rejects_empty_input_file_array() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-empty-session-inputs-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_piped_stdio_spec();
    spec.datasets.private_benchmark_enabled = false;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    if let ChallengeExecutionSpec::PipedStdio(execution) = &mut spec.execution {
        execution.official_session = None;
    }
    create_piped_stdio_bundle(&root, &spec);
    std::fs::write(
        root.join("public/session.json"),
        r#"{"session_name":"public-1","input_files":[],"metadata":null}"#,
    )
    .expect("failed to write empty session inputs");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("empty session input files should fail");
    assert!(error.to_string().contains("non-empty array"));
}

/// Verifies that run manifests cannot declare more than the platform run cap.
#[tokio::test]
async fn run_manifest_rejects_too_many_runs() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-too-many-runs-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    let runs = (0..=crate::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION)
        .map(|index| {
            serde_json::json!({
                "run_name": format!("public-{index}"),
                "interface": "stdio",
                "stdin_json": null,
                "stdin_text": "1",
                "input_files": null,
                "output_files": null,
                "metadata": null
            })
        })
        .collect::<Vec<_>>();
    std::fs::write(
        root.join("public/runs.json"),
        serde_json::json!({ "runs": runs }).to_string(),
    )
    .expect("failed to write too-large run manifest");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("too many runs should be rejected");
    assert!(error.to_string().contains("at most 100 runs"));
}

/// Verifies that run names cannot escape evaluator-visible filesystem paths.
#[tokio::test]
async fn run_manifest_rejects_parent_directory_run_name() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-unsafe-run-name-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = false;
    create_bundle(&root, &spec);
    std::fs::write(
        root.join("public/runs.json"),
        r#"{"runs":[{"run_name":"..","interface":"stdio","stdin_json":null,"stdin_text":"1","input_files":null,"output_files":null,"metadata":null}]}"#,
    )
    .expect("failed to write unsafe run manifest");

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    let error = result.expect_err("parent-directory run names should be rejected");
    assert!(error.to_string().contains("run_name"));
}

/// Verifies that enabled private benchmark bundle requires directory.
#[tokio::test]
async fn enabled_private_benchmark_bundle_requires_directory() {
    let root = std::env::temp_dir().join(format!(
        "agentics-bundle-enabled-private-benchmark-{}",
        uuid::Uuid::new_v4()
    ));
    let mut spec = base_spec();
    spec.datasets.private_benchmark_enabled = true;
    spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
    create_bundle(&root, &spec);

    let result = validate_challenge_bundle(&root).await;
    drop(std::fs::remove_dir_all(root));

    assert!(result.is_err());
}
