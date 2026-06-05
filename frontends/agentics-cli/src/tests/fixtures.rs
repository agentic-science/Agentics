use super::*;

/// Handles challenge detail json for this module.
pub(super) fn challenge_detail_json(validation_enabled: bool) -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "keywords": ["math"],
        "moltbook": {
            "submolt_name": "agentics-platform",
            "submolt_url": "https://www.moltbook.com/m/agentics-platform",
            "discussion_url": "https://www.moltbook.com/post/sample-sum"
        },
        "spec": {
            "schema_version": 1,
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "summary": { "en": "Add numbers", "zh": "数字求和" },
            "keywords": ["math"],
            "starts_at": "2026-01-01T00:00:00Z",
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_live"
            },
            "solution_publication": "public",
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "targets": [
                {
                    "name": "linux-arm64-cpu",
                    "docker_platform": "linux/arm64",
                    "accelerator": null,
                    "validation_enabled": validation_enabled,
                    "resource_profile": {
                        "name": "python-cpu-small",
                        "solution_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "evaluator_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "solution": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        },
                        "evaluator": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        }
                    }
                }
            ],
            "execution": {
                "mode": "separated_evaluator",
                "separated_evaluator": {
                    "command": ["python", "separated-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "validation_runs": "public/runs.json"
            },
            "datasets": {
                "public_dir": "public",
                "public_policy": "full",
                "private_benchmark_policy": "score_only",
                "private_benchmark_enabled": true
            },
            "metric_schema": {
                "metrics": [
                    {
                        "name": "score",
                        "label": "Score",
                        "direction": "maximize",
                        "visibility": "public"
                    },
                    {
                        "name": "passed_cases",
                        "label": "Passed Cases",
                        "unit": "cases",
                        "direction": "maximize",
                        "visibility": "public"
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score",
                    "tie_breaker_metric_names": ["passed_cases"]
                }
            }
        },
        "statement_markdown": "# Sample Sum"
    })
}

/// Handles public submission list json for this module.
pub(super) fn public_submission_list_json() -> serde_json::Value {
    json!({
        "total_count": 3,
        "items": [
            {
                "id": "11111111-1111-4111-8111-111111111111",
                "challenge_name": "sample-sum",
                "target": "linux-arm64-cpu",
                "challenge_title": "Sample Sum",
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "status": "completed",
                "note": "public note",
                "explanation": "fast solution",
                "credit_text": "",
                "rank_score": 1.8,
                "official_primary_metric": { "metric_name": "score", "value": 1.8 },
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:01Z"
            }
        ]
    })
}

/// Handles leaderboard json for this module.
pub(super) fn leaderboard_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "target": "linux-arm64-cpu",
        "items": [
            {
                "target": "linux-arm64-cpu",
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "best_solution_submission_id": "11111111-1111-4111-8111-111111111111",
                "best_rank_score": 1.8,
                "rank_score": 1.8,
                "official_primary_metric": { "metric_name": "score", "value": 1.8 },
                "updated_at": "2026-05-01T00:00:01Z"
            }
        ]
    })
}

/// Handles score distribution json for this module.
pub(super) fn score_distribution_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "target": "linux-arm64-cpu",
        "metric_name": "score",
        "count": 2,
        "min": 1.0,
        "max": 1.8,
        "mean": 1.4,
        "quantiles": [
            { "quantile": 0.5, "value": 1.0 },
            { "quantile": 0.9, "value": 1.8 }
        ],
        "histogram": [
            { "lower": 1.0, "upper": 1.8, "count": 2 }
        ]
    })
}

/// Handles solution submission json for this module.
pub(super) fn solution_submission_json() -> serde_json::Value {
    json!({
        "id": "11111111-1111-4111-8111-111111111111",
        "challenge_name": "sample-sum",
        "challenge_title": "Sample Sum",
        "target": "linux-arm64-cpu",
        "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "agent_display_name": "solver",
        "status": "completed",
        "note": "public note",
        "explanation": "fast solution",
        "credit_text": "",
        "visible_after_eval": true,
        "official_evaluation": {
            "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "target": "linux-arm64-cpu",
            "status": "completed",
            "eval_type": "official",
            "rank_score": 1.8,
            "aggregate_metrics": [
                { "metric_name": "score", "value": 1.8 }
            ],
            "run_metrics": [],
            "public_results": []
        },
        "official_primary_metric": { "metric_name": "score", "value": 1.8 },
        "created_at": "2026-05-01T00:00:00Z",
        "updated_at": "2026-05-01T00:00:01Z"
    })
}

/// Handles validation-only solution submission json for report rendering tests.
pub(super) fn validation_only_solution_submission_json() -> serde_json::Value {
    let mut value = solution_submission_json();
    let object = value
        .as_object_mut()
        .expect("solution submission fixture should be an object");
    object.remove("official_evaluation");
    object.remove("official_primary_metric");
    object.insert("visible_after_eval".to_string(), json!(false));
    object.insert(
        "validation_evaluation".to_string(),
        json!({
            "id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "target": "linux-arm64-cpu",
            "status": "completed",
            "eval_type": "validation",
            "rank_score": 0.75,
            "aggregate_metrics": [
                { "metric_name": "score", "value": 0.75 }
            ],
            "run_metrics": [],
            "public_results": []
        }),
    );
    value
}

/// Handles ranking context json for this module.
pub(super) fn ranking_context_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "target": "linux-arm64-cpu",
        "solution_submission_id": "11111111-1111-4111-8111-111111111111",
        "rank": 1,
        "total_ranked": 2,
        "percentile": 1.0,
        "is_agent_best": true,
        "entry": leaderboard_json()["items"][0].clone(),
        "nearby_entries": [
            {
                "rank": 1,
                "entry": leaderboard_json()["items"][0].clone()
            }
        ]
    })
}

/// Handles challenge manifest json for this module.
pub(super) fn challenge_manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "new_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "keywords": ["math"],
        "readme_path": "README.md",
        "bundle_path": "v1",
        "private_assets": [
            {
                "asset_name": "official-cases",
                "kind": "private_benchmark_data",
                "required": true
            }
        ]
    })
}

/// Handles challenge review record json for this module.
pub(super) fn challenge_review_record_json(status: &str) -> serde_json::Value {
    let mut value = json!({
        "id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "challenge_name": "sample-sum",
        "request": "new_challenge",
        "status": status,
        "creator_human_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "creator_github_user_id": 1001,
        "creator_github_login": "creator",
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": "0123456789abcdef0123456789abcdef01234567",
        "challenge_path": "challenges/sample-sum",
        "manifest_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "manifest": challenge_manifest_json(),
        "private_assets": [],
        "validation_records": [],
        "created_at": "2026-05-01T00:00:00Z",
        "updated_at": "2026-05-01T00:00:00Z"
    });
    if status == "validated" {
        value["validation_bundle_sha256"] =
            json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        value["validation_message"] = json!("validation completed");
        value["validation_records"] = json!([
            {
                "id": "ffffffff-ffff-4fff-8fff-ffffffffffff",
                "review_record_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
                "status": "passed",
                "message": "validation completed",
                "repository_path": "/tmp/challenges",
                "manifest_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "bundle_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "created_at": "2026-05-01T00:00:00Z"
            }
        ]);
    }
    value
}

/// Handles creator owner stats json for this module.
pub(super) fn creator_stats_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "target": "linux-arm64-cpu",
        "agent_count": 2,
        "solution_submission_count": 5,
        "completed_solution_submission_count": 3,
        "failed_solution_submission_count": 1,
        "queued_or_running_solution_submission_count": 1,
        "visible_solution_submission_count": 3,
        "validation_run_count": 4,
        "official_run_count": 2,
        "latest_solution_submission_at": "2026-05-01T00:00:00Z",
        "latest_completed_evaluation_at": "2026-05-01T00:00:01Z",
        "best_rank_score_min": 1.0,
        "best_rank_score_max": 2.5,
        "best_rank_score_mean": 1.75
    })
}

/// Handles creator participants json for this module.
pub(super) fn creator_participants_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "target": "linux-arm64-cpu",
        "items": [
            {
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "solution_submission_count": 2,
                "best_solution_submission_id": "11111111-1111-4111-8111-111111111111",
                "best_rank_score": 2.5,
                "latest_status": "completed",
                "latest_solution_submission_at": "2026-05-01T00:00:00Z"
            }
        ]
    })
}

/// Handles creator shortlist json for this module.
pub(super) fn challenge_shortlist_json() -> serde_json::Value {
    json!({
        "challenge_name": "sample-sum",
        "items": [
            {
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "added_by_human_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "created_at": "2026-05-01T00:00:00Z"
            }
        ]
    })
}

/// Handles creator shortlist revision json for this module.
pub(super) fn challenge_shortlist_revision_json() -> serde_json::Value {
    json!({
        "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        "challenge_name": "sample-sum",
        "uploader_human_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        "requested_count": 2,
        "added_count": 1,
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "storage_key": "shortlists/sample-sum/revisions/cccccccc-cccc-4ccc-8ccc-cccccccccccc.json",
        "created_at": "2026-05-01T00:00:00Z"
    })
}

/// Handles pioneer code detail json for this module.
pub(super) fn pioneer_code_detail_json() -> serde_json::Value {
    json!({
        "code": {
            "id": "11111111-1111-4111-8111-111111111111",
            "code_display": "jack-7f9eb67a",
            "label": "jack",
            "note": "early access",
            "max_uses": 2,
            "use_count": 1,
            "status": "active",
            "expires_at": "2026-06-01T00:00:00Z",
            "created_by_display": "admin",
            "created_at": "2026-05-01T00:00:00Z"
        },
        "uses": [
            {
                "subject_kind": "agent",
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "registration_kind": "agent_api",
                "used_at": "2026-05-01T00:00:00Z"
            }
        ]
    })
}

/// Handles pioneer code list json for this module.
pub(super) fn pioneer_code_list_json() -> serde_json::Value {
    json!({
        "items": [pioneer_code_detail_json()["code"].clone()]
    })
}

/// Handles pioneer code revoke json for this module.
pub(super) fn pioneer_code_revoke_json() -> serde_json::Value {
    json!({
        "id": "11111111-1111-4111-8111-111111111111",
        "status": "revoked",
        "revoked_human_count": 1,
        "revoked_human_session_count": 2,
        "revoked_admin_service_token_count": 0,
        "revoked_creator_api_token_count": 3,
        "revoked_agent_count": 1,
        "revoked_token_count": 4
    })
}

/// Handles admin challenge list json for this module.
pub(super) fn admin_challenge_list_json() -> serde_json::Value {
    json!({
        "items": [
            {
                "challenge_name": "sample-sum",
                "title": "Sample Sum",
                "summary": { "en": "Add numbers", "zh": "数字求和" },
                "keywords": ["math"],
                "status": "active",
                "starts_at": "2026-01-01T00:00:00Z",
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:01Z"
            }
        ]
    })
}

/// Handles Moltbook discussion response json for this module.
pub(super) fn moltbook_discussion_json(discussion_url: Option<&str>) -> serde_json::Value {
    let mut moltbook = json!({
        "submolt_name": "agentics-platform",
        "submolt_url": "https://www.moltbook.com/m/agentics-platform"
    });
    if let Some(url) = discussion_url {
        moltbook["discussion_url"] = json!(url);
    }
    json!({
        "challenge_name": "sample-sum",
        "moltbook": moltbook
    })
}

/// Handles admin submissions list json for this module.
pub(super) fn admin_solution_submission_list_json() -> serde_json::Value {
    json!({
        "items": [
            {
                "id": "11111111-1111-4111-8111-111111111111",
                "challenge_name": "sample-sum",
                "challenge_title": "Sample Sum",
                "target": "linux-arm64-cpu",
                "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "agent_display_name": "solver",
                "status": "completed",
                "note": "public note",
                "visible_after_eval": true,
                "latest_job_id": "22222222-2222-4222-8222-222222222222",
                "latest_job_status": "completed",
                "latest_job_eval_type": "official",
                "validation_status": "completed",
                "official_status": "completed",
                "rank_score": 2.5,
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:01Z"
            }
        ]
    })
}

/// Handles evaluation job json for this module.
pub(super) fn evaluation_job_json() -> serde_json::Value {
    json!({
        "job_id": "22222222-2222-4222-8222-222222222222",
        "solution_submission_id": "11111111-1111-4111-8111-111111111111",
        "target": "linux-arm64-cpu",
        "eval_type": "official",
        "status": "queued"
    })
}

/// Handles admin private assets list json for this module.
pub(super) fn admin_private_assets_json() -> serde_json::Value {
    json!({
        "items": [
            {
                "id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
                "review_record_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
                "asset_name": "official-cases",
                "kind": "private_benchmark_data",
                "required": true,
                "status": "active",
                "size_bytes": 17,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "storage_key": "challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets/official-cases.bin",
                "uploader_human_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "created_at": "2026-05-01T00:00:00Z",
                "activated_at": "2026-05-01T00:00:01Z"
            }
        ]
    })
}

/// Handles review record cleanup json for this module.
pub(super) fn review_record_cleanup_json() -> serde_json::Value {
    json!({
        "abandoned_review_records": 2,
        "purged_private_assets": 3,
        "purged_temporary_storage_objects": 4
    })
}

/// Handles service heartbeat list json for this module.
pub(super) fn service_heartbeat_list_json() -> serde_json::Value {
    json!({
        "items": [
            {
                "service_name": "worker-a",
                "last_seen_at": "2026-05-01T00:00:00Z",
                "payload": { "status": "idle" }
            }
        ]
    })
}

/// Handles admin capacity json for this module.
pub(super) fn admin_capacity_json() -> serde_json::Value {
    json!({
        "quota_window_seconds": 86400,
        "quotas": {
            "validation_runs_per_agent_challenge_day": 12,
            "official_runs_per_agent_challenge_day": 3,
            "max_active_official_jobs": 5,
            "max_active_agents": 100
        },
        "usage": {
            "active_agents": 7,
            "active_validation_jobs": 2,
            "active_official_jobs": 1
        }
    })
}

/// Handles disable agent json for this module.
pub(super) fn disable_agent_json() -> serde_json::Value {
    json!({
        "id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "status": "disabled"
    })
}
