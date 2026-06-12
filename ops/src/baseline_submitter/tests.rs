use std::fs;

use super::{
    BaselineStateRecord, build_deferlist, name_set_from_args, resumable_submission_id,
    solution_defer_marker, validate_api_base_url,
};

#[test]
fn default_deferlist_is_disabled_when_requested() {
    let deferlist = build_deferlist(true, None).expect("deferlist");

    assert!(deferlist.is_empty());
}

#[test]
fn default_deferlist_keeps_public_smoke_and_allows_upgraded_baselines() {
    let deferlist = build_deferlist(false, None).expect("deferlist");

    assert!(deferlist.contains(&"world-map-frontier-cs-algorithmic-6".parse().expect("name")));
    assert!(
        !deferlist.contains(
            &"functional-cycle-reach-frontier-cs-algorithmic-252"
                .parse()
                .expect("name")
        )
    );
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
