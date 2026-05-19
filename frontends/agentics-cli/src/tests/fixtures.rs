use super::*;

/// Handles challenge detail json for this module.
pub(super) fn challenge_detail_json(validation_enabled: bool) -> serde_json::Value {
    json!({
        "name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "spec": {
            "schema_version": 1,
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "summary": { "en": "Add numbers", "zh": "数字求和" },
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
            "scorer": {
                "command": ["python", "scorer/run.py"],
                "result_file": "result.json"
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
                        "scorer_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "timeout_sec": 30,
                        "memory_limit_mb": 512,
                        "cpu_limit_millis": 1000,
                        "disk_limit_mb": 1024,
                        "setup_network_access": "enabled",
                        "build_network_access": "disabled",
                        "run_network_access": "disabled",
                        "scorer_network_access": "disabled"
                    }
                }
            ],
            "execution": {
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
                "official_score": 1.8,
                "rank_score": 1.8,
                "aggregate_metrics": [
                    { "metric_name": "score", "value": 1.8 }
                ],
                "official_metrics": [],
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
                "aggregate_metrics": [
                    { "metric_name": "score", "value": 1.8 }
                ],
                "official_metrics": [],
                "official_score": 1.8,
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
            "primary_score": 1.8,
            "rank_score": 1.8,
            "aggregate_metrics": [
                { "metric_name": "score", "value": 1.8 }
            ],
            "run_metrics": [],
            "public_results": []
        },
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
    object.insert("visible_after_eval".to_string(), json!(false));
    object.insert(
        "validation_evaluation".to_string(),
        json!({
            "id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "target": "linux-arm64-cpu",
            "status": "completed",
            "eval_type": "validation",
            "primary_score": 0.75,
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

/// Handles challenge draft json for this module.
pub(super) fn challenge_draft_json(status: &str) -> serde_json::Value {
    json!({
        "id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "challenge_name": "sample-sum",
        "request": "new_challenge",
        "status": status,
        "creator_agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
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
    })
}
