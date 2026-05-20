//! Integration tests for runner quota and output-resource behavior.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, examples_challenges_root, run_worker_once, sample_sum_solution,
    solution_zip_base64_with_scripts, spawn_app_with_config, test_config, zip_project_zip_base64,
};

const QUOTA_TEST_STORAGE_MODE_ENV: &str = "AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE";
const QUOTA_TEST_RUNTIME_ROOT_ENV: &str = "AGENTICS_TEST_RUNNER_RUNTIME_ROOT";
const QUOTA_TEST_PHASE_MOUNT_ROOT_ENV: &str = "AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT";
const QUOTA_TEST_SLOT_CLASSES_ENV: &str = "AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB";
const QUOTA_TEST_SETUP_SCRIPT: &str = "scripts/ops/prepare-dgx-spark-test-storage.sh";
const QUOTA_TEST_REQUIRED_SLOT_CLASS_MB: u64 = 64;

/// Returns true when the local environment can run Linux quota-sensitive tests.
fn quota_sensitive_runner_env_configured() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    validate_quota_sensitive_runner_env().unwrap_or_else(|message| panic!("{message}"));
    true
}

/// Build a test config that uses the validated Linux quota test root.
fn quota_sensitive_runner_config(
    storage_root: &Path,
    challenges_root: &Path,
) -> shared::config::Config {
    validate_quota_sensitive_runner_env().expect("quota env should be validated before use");
    let mut config = test_config(storage_root, challenges_root);
    config.runner_writable_storage_mode =
        std::env::var(QUOTA_TEST_STORAGE_MODE_ENV).expect("validated quota storage mode");
    config.runner_runtime_root =
        Some(std::env::var(QUOTA_TEST_RUNTIME_ROOT_ENV).expect("validated quota runtime root"));
    config.runner_phase_mount_root =
        Some(std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).expect("validated quota mount root"));
    config.runner_writable_slot_classes_mb =
        std::env::var(QUOTA_TEST_SLOT_CLASSES_ENV).expect("validated quota slot classes");
    config.runner_docker_layer_quota =
        std::env::var("AGENTICS_TEST_RUNNER_DOCKER_LAYER_QUOTA").is_ok_and(|value| value == "true");
    config
}

/// Validate Linux-only quota-sensitive test environment variables.
fn validate_quota_sensitive_runner_env() -> std::result::Result<(), String> {
    let storage_mode = std::env::var(QUOTA_TEST_STORAGE_MODE_ENV).ok();
    let runtime_root = std::env::var(QUOTA_TEST_RUNTIME_ROOT_ENV).ok();
    let phase_mount_root = std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).ok();
    let slot_classes = std::env::var(QUOTA_TEST_SLOT_CLASSES_ENV).ok();

    let missing: Vec<&str> = [
        (storage_mode.is_none(), QUOTA_TEST_STORAGE_MODE_ENV),
        (runtime_root.is_none(), QUOTA_TEST_RUNTIME_ROOT_ENV),
        (phase_mount_root.is_none(), QUOTA_TEST_PHASE_MOUNT_ROOT_ENV),
        (slot_classes.is_none(), QUOTA_TEST_SLOT_CLASSES_ENV),
    ]
    .into_iter()
    .filter_map(|(is_missing, name)| is_missing.then_some(name))
    .collect();
    if !missing.is_empty() {
        return Err(quota_test_setup_error(format!(
            "missing environment variable(s): {}",
            missing.join(", ")
        )));
    }

    let storage_mode = storage_mode.expect("checked above");
    if storage_mode != "xfs-project-quota-slots" {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_STORAGE_MODE_ENV} must be xfs-project-quota-slots, got `{storage_mode}`"
        )));
    }

    let runtime_root = runtime_root.expect("checked above");
    let root = Path::new(&runtime_root);
    if !root.is_absolute() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_RUNTIME_ROOT_ENV} must be an absolute path, got `{runtime_root}`"
        )));
    }
    if !root.is_dir() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_RUNTIME_ROOT_ENV} must point to a prepared test runtime root, got `{runtime_root}`"
        )));
    }

    let phase_mount_root = phase_mount_root.expect("checked above");
    let root = Path::new(&phase_mount_root);
    if !root.is_absolute() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} must be an absolute path, got `{phase_mount_root}`"
        )));
    }
    if !root.is_dir() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} must point to a prepared test quota root, got `{phase_mount_root}`"
        )));
    }

    let slot_classes = parse_quota_test_slot_classes(&slot_classes.expect("checked above"))?;
    if !slot_classes.contains(&QUOTA_TEST_REQUIRED_SLOT_CLASS_MB) {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_SLOT_CLASSES_ENV} must include {QUOTA_TEST_REQUIRED_SLOT_CLASS_MB} MiB because these tests lower the challenge disk limit to that class"
        )));
    }
    let slot_metadata = root
        .join("solution-run")
        .join("slots")
        .join(format!("{QUOTA_TEST_REQUIRED_SLOT_CLASS_MB}mb"))
        .join("slot-001")
        .join(".agentics-slot.json");
    if !slot_metadata.is_file() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} does not look like a prepared bounded quota root; missing {}",
            slot_metadata.display()
        )));
    }

    Ok(())
}

/// Parse comma or whitespace separated quota slot classes.
fn parse_quota_test_slot_classes(raw: &str) -> std::result::Result<Vec<u64>, String> {
    let mut parsed = Vec::new();
    for value in raw.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let class_mb = value.parse::<u64>().map_err(|error| {
            quota_test_setup_error(format!(
                "{QUOTA_TEST_SLOT_CLASSES_ENV} contains invalid slot class `{value}`: {error}"
            ))
        })?;
        if class_mb == 0 {
            return Err(quota_test_setup_error(format!(
                "{QUOTA_TEST_SLOT_CLASSES_ENV} entries must be positive"
            )));
        }
        parsed.push(class_mb);
    }
    parsed.sort_unstable();
    parsed.dedup();
    if parsed.is_empty() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_SLOT_CLASSES_ENV} must not be empty"
        )));
    }
    Ok(parsed)
}

/// Build the actionable Linux quota-test setup failure.
fn quota_test_setup_error(reason: String) -> String {
    format!(
        "Linux quota-sensitive runner tests require a prepared bounded test quota root: {reason}. Run `{QUOTA_TEST_SETUP_SCRIPT}` and export {QUOTA_TEST_STORAGE_MODE_ENV}=xfs-project-quota-slots, {QUOTA_TEST_RUNTIME_ROOT_ENV}=/srv/agentics-test/runtime, {QUOTA_TEST_PHASE_MOUNT_ROOT_ENV}=/srv/agentics-test/phase-mounts, and {QUOTA_TEST_SLOT_CLASSES_ENV}=64,256,1024,4096 before running these tests on Linux."
    )
}

/// Verifies that worker enforces run writable disk limit.
#[sqlx::test(migrations = "../migrations")]
async fn worker_enforces_run_writable_disk_limit(pool: sqlx::PgPool) {
    if !quota_sensitive_runner_env_configured() {
        return;
    }

    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = quota_sensitive_runner_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    sqlx::query(
        r#"
        UPDATE challenges
        SET spec_json = jsonb_set(spec_json, '{targets,0,resource_profile,disk_limit_mb}', '64'::jsonb)
        WHERE name = 'sample-sum'
        "#,
    )
    .execute(&pool)
    .await
    .expect("failed to lower test resource profile disk limit");

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "run-disk-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
dd if=/dev/zero of="$AGENTICS_OUTPUT_DIR/quota.bin" bs=1M count=80
python main.py
"#;
    let artifact_base64 = zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "disk limit probe",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        ("run.sh", run_sh.to_string()),
        (
            "main.py",
            sample_sum_solution("payload['a'] + payload['b']"),
        ),
    ]);

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run writable disk limit probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(
        last_error.contains("\"phase\":\"run\""),
        "expected run phase failure, got: {last_error}"
    );

    assert!(last_error.contains("\"reason\":\"non_zero_exit\""));
}

/// Verifies that worker enforces platform-owned output file count limits.
#[sqlx::test(migrations = "../migrations")]
async fn worker_rejects_excessive_run_output_files(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "run-output-file-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
i=0
while [ "$i" -lt 8193 ]; do
  : > "$AGENTICS_OUTPUT_DIR/file-$i.txt"
  i=$((i + 1))
done
python main.py
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n",
        run_sh,
    );

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run output file limit probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(
        last_error.contains("output file limit"),
        "unexpected last_error: {last_error}"
    );
    assert!(
        last_error.contains("resource_limit"),
        "unexpected last_error: {last_error}"
    );
}

/// Verifies quota-backed Linux runner slots enforce inode limits when configured.
#[test]
fn test_quota_root_enforces_inode_limit_when_configured() {
    if !quota_sensitive_runner_env_configured() {
        return;
    }

    let phase_mount_root =
        std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).expect("validated quota env");
    let slot_root = Path::new(&phase_mount_root)
        .join("solution-run")
        .join("slots")
        .join(format!("{QUOTA_TEST_REQUIRED_SLOT_CLASS_MB}mb"))
        .join("slot-004");
    let probe_root = slot_root.join(format!("direct-inode-probe-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&probe_root).expect("failed to create inode probe root");

    let mut quota_hit = None;
    for index in 0..17_000 {
        let path = probe_root.join(format!("inode-{index}"));
        match std::fs::File::create(&path) {
            Ok(_) => {}
            Err(error) if is_quota_exhaustion(&error) => {
                quota_hit = Some((index, error));
                break;
            }
            Err(error) => panic!("unexpected inode probe write failure at {index}: {error}"),
        }
    }
    drop(std::fs::remove_dir_all(&probe_root));

    let Some((index, error)) = quota_hit else {
        panic!(
            "expected the prepared {QUOTA_TEST_REQUIRED_SLOT_CLASS_MB} MiB test quota slot to exhaust before 17000 files; verify {QUOTA_TEST_SETUP_SCRIPT} prepared inode quotas"
        );
    };
    assert!(
        index < 17_000,
        "quota exhaustion must happen before the full probe count: {error}"
    );
}

/// Return whether a filesystem error represents byte or inode quota exhaustion.
fn is_quota_exhaustion(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(28) | Some(122))
        || error.to_string().contains("No space left on device")
        || error.to_string().contains("Disk quota exceeded")
}

/// Verifies setup/build dependency trees are not capped by scorer-visible output limits.
#[sqlx::test(migrations = "../migrations")]
async fn setup_build_file_count_is_not_limited_by_output_cap(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.runner_max_output_files = 8;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "dependency-file-count-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let build_sh = r#"#!/usr/bin/env sh
set -eu
mkdir -p build/deps
i=0
while [ "$i" -lt 16 ]; do
  : > "build/deps/file-$i.txt"
  i=$((i + 1))
done
printf built > build/generated.txt
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        build_sh,
        "#!/usr/bin/env sh\nset -eu\ntest -f build/generated.txt\npython main.py\n",
    );

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "dependency tree should not hit output file cap"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);
}
