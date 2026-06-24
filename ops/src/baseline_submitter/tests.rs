use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use super::{
    BaselineStateRecord, TargetSelection, build_deferlist, name_set_from_args,
    resumable_submission_id, select_declared_targets, solution_defer_marker, validate_api_base_url,
};

#[test]
fn default_deferlist_is_disabled_when_requested() {
    let deferlist = build_deferlist(true, None).expect("deferlist");

    assert!(deferlist.is_empty());
}

#[test]
fn default_deferlist_keeps_no_challenge_name_deferrals() {
    let deferlist = build_deferlist(false, None).expect("deferlist");

    assert!(deferlist.is_empty());
}

#[test]
fn default_deferlist_allows_upgraded_baselines() {
    let deferlist = build_deferlist(false, None).expect("deferlist");

    assert!(
        !deferlist.contains(
            &"colored-ball-pole-sorting-frontier-cs-algorithmic-142"
                .parse()
                .expect("name")
        )
    );
    assert!(
        !deferlist.contains(
            &"substring-ab-program-frontier-cs-algorithmic-23"
                .parse()
                .expect("name")
        )
    );
    assert!(
        !deferlist.contains(
            &"uniform-cave-explorer-frontier-cs-algorithmic-80"
                .parse()
                .expect("name")
        )
    );
    assert!(!deferlist.contains(&"imagenet-1m-frontier-cs-imagenet-1m".parse().expect("name")));
    assert!(
        !deferlist.contains(
            &"symreg-mccormick-frontier-cs-symreg-mccormick"
                .parse()
                .expect("name")
        )
    );
    assert!(!deferlist.contains(&"world-map-frontier-cs-algorithmic-6".parse().expect("name")));
    assert!(
        !deferlist.contains(
            &"functional-cycle-reach-frontier-cs-algorithmic-252"
                .parse()
                .expect("name")
        )
    );
    for challenge_name in [
        "adaptive-impostor-search-frontier-cs-algorithmic-245",
        "disk-probing-frontier-cs-algorithmic-60",
        "heap-tree-sum-frontier-cs-algorithmic-209",
        "hidden-circuit-gates-frontier-cs-algorithmic-101",
        "induced-triple-graph-frontier-cs-algorithmic-120",
        "inversion-recovery-frontier-cs-algorithmic-73",
        "mineral-pairing-frontier-cs-algorithmic-125",
        "space-thief-stars-frontier-cs-algorithmic-63",
    ] {
        assert!(
            !deferlist.contains(&challenge_name.parse().expect("name")),
            "{challenge_name} should be submitter-ready after official replay"
        );
    }
}

#[test]
fn name_set_accepts_explicit_names() {
    let name = "hello-world-rs".parse().expect("challenge name");
    let set = name_set_from_args(&[name], None).expect("name set");

    assert!(set.contains(&"hello-world-rs".parse().expect("challenge name")));
}

#[test]
fn solution_defer_marker_detects_smoke_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("agentics.solution.json"),
        serde_json::json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "note": "Public smoke solution"
        })
        .to_string(),
    )
    .expect("manifest");

    let marker = solution_defer_marker(temp.path()).expect("marker");

    assert!(marker.is_some());
}

#[test]
fn baseline_submitter_rejects_remote_http_without_override() {
    let error = validate_api_base_url(&"http://agentics.example".parse().expect("url"))
        .expect_err("remote HTTP should fail");

    assert!(error.to_string().contains("HTTP API base URLs"));
}

#[test]
fn baseline_submitter_allows_loopback_http() {
    validate_api_base_url(&"http://127.0.0.1:3110".parse().expect("url"))
        .expect("loopback HTTP should be allowed");
}

#[test]
fn frontier_testlib_wrappers_have_case_timeouts() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let challenge_root = repo_root.join("challenge-repos/agentics-challenges/challenges");
    let mut missing = Vec::new();

    for entry in fs::read_dir(&challenge_root).expect("challenge root") {
        let run_py = entry
            .expect("challenge entry")
            .path()
            .join("v1/interactive-evaluator/run.py");
        if !run_py.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&run_py).expect("wrapper source");
        if contents.contains("Frontier-CS Testlib interactive evaluator")
            && !contents.contains("timeout=case_timeout_sec")
        {
            missing.push(run_py);
        }
    }

    assert!(
        missing.is_empty(),
        "Frontier-CS Testlib wrappers must convert blocked participants into structured evaluator results: {missing:?}"
    );
}

#[test]
fn frontier_testlib_wrapper_writes_result_on_interactor_timeout() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let wrapper = repo_root
        .join("challenge-repos/agentics-challenges/challenges/mineral-pairing-frontier-cs-algorithmic-125/v1/interactive-evaluator/run.py");
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("bin");
    let challenge_dir = temp.path().join("challenge");
    let session_input_dir = temp.path().join("session-input");
    let session_file = temp.path().join("session.json");
    let output_path = temp.path().join("result.json");
    fs::create_dir_all(&fake_bin).expect("fake bin dir");
    fs::create_dir_all(challenge_dir.join("interactive-evaluator")).expect("challenge dir");
    fs::create_dir_all(&session_input_dir).expect("session input dir");
    fs::write(
        fake_bin.join("g++"),
        r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    out="$1"
  fi
  shift || true
done
cat > "$out" <<'EOF'
#!/bin/sh
sleep 2
EOF
chmod +x "$out"
"#,
    )
    .expect("fake g++");
    fs::set_permissions(fake_bin.join("g++"), fs::Permissions::from_mode(0o755))
        .expect("fake g++ permissions");
    fs::write(
        challenge_dir.join("interactive-evaluator/interactor.cpp"),
        "int main() { return 0; }\n",
    )
    .expect("interactor source");
    fs::write(session_input_dir.join("case.in"), "1\n").expect("case input");
    fs::write(session_input_dir.join("case.ans"), "1\n").expect("case answer");
    fs::write(
        &session_file,
        serde_json::json!({
            "session_name": "timeout-fixture",
            "metadata": {
                "case_timeout_sec": 0.1,
                "cases": [
                    {
                        "case_name": "case-1",
                        "input_path": "case.in",
                        "answer_path": "case.ans"
                    }
                ]
            }
        })
        .to_string(),
    )
    .expect("session file");

    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let fake_path = format!("{}:{}", fake_bin.display(), existing_path.to_string_lossy());
    let output = Command::new("python3")
        .arg(&wrapper)
        .arg("--challenge-dir")
        .arg(&challenge_dir)
        .arg("--session-file")
        .arg(&session_file)
        .arg("--session-input-dir")
        .arg(&session_input_dir)
        .arg("--output-path")
        .arg(&output_path)
        .arg("--mode")
        .arg("validation")
        .arg("--target")
        .arg("linux-arm64-cpu")
        .env("PATH", fake_path)
        .output()
        .expect("wrapper command");

    assert!(
        output.status.success(),
        "wrapper should exit successfully after evaluator-authored failure: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).expect("wrapper result json"))
            .expect("valid result json");

    assert_eq!(
        result.get("status").and_then(serde_json::Value::as_str),
        Some("failed")
    );
    assert_eq!(
        result
            .get("validation_summary")
            .and_then(|summary| summary.get("protocol_errors"))
            .and_then(serde_json::Value::as_i64),
        Some(1)
    );
    let message = result
        .get("public_results")
        .and_then(serde_json::Value::as_array)
        .and_then(|results| results.first())
        .and_then(|result| result.get("message"))
        .and_then(serde_json::Value::as_str)
        .expect("message");
    assert!(
        message.contains("timed out"),
        "timeout should be visible in structured diagnostics: {result}"
    );
}

#[test]
fn baseline_submitter_resumes_nonterminal_record() {
    let record = BaselineStateRecord {
        challenge_name: "hello-world-rs".parse().expect("challenge name"),
        target: "linux-arm64-cpu".parse().expect("target"),
        solution_path: "solution".into(),
        submission_id: Some(
            "11111111-1111-4111-8111-111111111111"
                .parse()
                .expect("submission id"),
        ),
        status: "failed_to_wait".to_string(),
        note: "timeout".to_string(),
        recorded_at_unix_secs: 1,
    };

    assert_eq!(
        resumable_submission_id(Some(&record), false),
        record.submission_id
    );
    assert!(resumable_submission_id(Some(&record), true).is_none());
}

#[test]
fn target_selection_requires_explicit_target() {
    let error = TargetSelection::from_args(&[]).expect_err("missing target should fail");

    assert!(
        error.to_string().contains("--target <target> is required"),
        "{error:#}"
    );
}

#[test]
fn explicit_target_selection_keeps_requested_targets() {
    let selection = TargetSelection::from_args(&["linux-arm64-cpu".parse().expect("cpu target")])
        .expect("selection");
    let challenge_name = "multi-target".parse().expect("challenge name");
    let declared = [
        "linux-arm64-cuda".parse().expect("cuda target"),
        "linux-arm64-cpu".parse().expect("cpu target"),
    ];

    let selected =
        select_declared_targets(&challenge_name, &declared, &selection).expect("selected targets");

    assert_eq!(
        selected,
        vec!["linux-arm64-cpu".parse().expect("cpu target")]
    );
}

#[test]
fn explicit_target_selection_returns_all_requested_declared_targets() {
    let selection = TargetSelection::from_args(&[
        "linux-arm64-cuda".parse().expect("cuda target"),
        "linux-arm64-cpu".parse().expect("cpu target"),
    ])
    .expect("selection");
    let challenge_name = "multi-target".parse().expect("challenge name");
    let declared = [
        "linux-arm64-cuda".parse().expect("cuda target"),
        "linux-arm64-cpu".parse().expect("cpu target"),
    ];

    let selected =
        select_declared_targets(&challenge_name, &declared, &selection).expect("selected targets");

    assert_eq!(
        selected,
        vec![
            "linux-arm64-cpu".parse().expect("cpu target"),
            "linux-arm64-cuda".parse().expect("cuda target")
        ]
    );
}

#[test]
fn explicit_target_selection_rejects_missing_target() {
    let selection = TargetSelection::from_args(&["linux-arm64-cpu".parse().expect("cpu target")])
        .expect("selection");
    let challenge_name = "gpu-only".parse().expect("challenge name");
    let declared = ["linux-arm64-cuda".parse().expect("cuda target")];

    let error = select_declared_targets(&challenge_name, &declared, &selection)
        .expect_err("missing explicit target should fail");

    assert!(
        error
            .to_string()
            .contains("does not declare requested target")
    );
}
