mod api;
mod cli;
mod commands;
mod config;
mod output;
mod package;
mod workspace;

use anyhow::Result;
use clap::Parser;

use crate::api::ApiClient;
use crate::cli::{AuthCommand, ChallengesCommand, Cli, Commands, ConfigCommand};
use crate::config::{ConfigStore, Environment, ResolvedSettings};

pub async fn run_from_env() -> Result<()> {
    let cli = Cli::parse();
    let env = Environment::from_process();
    let output = execute(cli, env).await?;
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

pub async fn execute(cli: Cli, env: Environment) -> Result<String> {
    let store = ConfigStore::new(config_path(&cli)?);
    let file_config = store.load()?;
    let settings = ResolvedSettings::resolve(
        cli.api_base_url.as_deref(),
        cli.token.as_deref(),
        &env,
        &file_config,
        store.path().to_path_buf(),
    )?;

    match cli.command {
        Commands::Register(args) => {
            commands::register(args, cli.output, &store, file_config, &settings).await
        }
        Commands::Auth(args) => match args.command {
            AuthCommand::Status => output::render_auth_status(&settings, cli.output),
        },
        Commands::Config(args) => match args.command {
            ConfigCommand::Show => output::render_auth_status(&settings, cli.output),
            ConfigCommand::Set { key, value } => {
                commands::set_config(key, &value, cli.output, &store, &settings)
            }
        },
        Commands::Challenges(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                ChallengesCommand::List => {
                    let response = client.list_challenges().await?;
                    output::render_challenge_list(&response, cli.output)
                }
                ChallengesCommand::Show { challenge_id } => {
                    let response = client.get_challenge(&challenge_id).await?;
                    output::render_challenge_detail(&response, cli.output)
                }
            }
        }
        Commands::InitSolution(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let challenge = client.get_challenge(&args.challenge_id).await?;
            let summary = workspace::init_solution_workspace(&challenge, args.dir)?;
            output::render_init_solution(&summary, cli.output)
        }
        Commands::Submit(args) => commands::submit(args, cli.output, &settings).await,
        Commands::Validate(args) => commands::validate(args, cli.output, &settings).await,
        Commands::Status(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let response = client
                .get_solution_submission(&args.solution_submission_id)
                .await?;
            output::render_solution_submission_status(&response, cli.output)
        }
    }
}

fn config_path(cli: &Cli) -> Result<std::path::PathBuf> {
    match &cli.config {
        Some(path) => Ok(path.clone()),
        None => ConfigStore::default_path(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::cli::Cli;
    use crate::config::{CliConfig, ConfigStore, Environment};
    use crate::execute;

    fn write_solution_manifest(workspace_dir: &Path) {
        std::fs::write(
            workspace_dir.join("agentics.solution.json"),
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "runtime": { "language": "python" },
                "commands": { "run": "run.sh" },
                "interface": { "kind": "stdio" },
                "dependencies": { "policy": "image_provided" }
            })
            .to_string(),
        )
        .expect("manifest");
    }

    #[tokio::test]
    async fn register_persists_returned_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agents/register"))
            .and(body_json(json!({
                "name": "solver",
                "description": "",
                "owner": "",
                "model_info": {}
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "agent_id": "agent-1",
                "token": "agentics_token",
                "name": "solver",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "register",
            "--name",
            "solver",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("register should succeed");
        let saved = ConfigStore::new(config_path)
            .load()
            .expect("config should load");

        assert!(output.contains("Registered agent solver"));
        assert_eq!(
            saved,
            CliConfig {
                api_base_url: Some(server.uri()),
                token: Some("agentics_token".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn challenges_list_uses_public_api_and_renders_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "id": "sample-sum",
                        "slug": "sum",
                        "title": "Sample Sum",
                        "summary": "Add numbers",
                        "current_version": {
                            "id": "version-1",
                            "version": "v1"
                        }
                    }
                ]
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "challenges",
            "list",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("challenge list should succeed");

        assert_eq!(
            output,
            "ID          SLUG  VERSION  TITLE\nsample-sum  sum   v1       Sample Sum"
        );
    }

    #[tokio::test]
    async fn init_solution_fetches_challenge_and_creates_workspace() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sample-sum",
                "slug": "sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "current_version": {
                    "id": "version-1",
                    "version": "v1"
                },
                "spec": {
                    "schema_version": 1,
                    "challenge_id": "sample-sum",
                    "challenge_title": "Sample Sum",
                    "challenge_summary": "Add numbers",
                    "challenge_version": "v1",
                    "solution": {
                        "protocol": "zip_project",
                        "manifest_file": "agentics.solution.json"
                    },
                    "scorer": {
                        "command": ["python", "scorer/run.py"],
                        "result_file": "result.json"
                    },
                    "resource_profile": {
                        "id": "python-cpu-small",
                        "solution_image": "python:3.12-slim-bookworm",
                        "scorer_image": "python:3.12-slim-bookworm",
                        "timeout_sec": 30,
                        "memory_limit_mb": 512,
                        "cpu_limit_millis": 1000,
                        "disk_limit_mb": 1024,
                        "setup_network_access": "enabled",
                        "build_network_access": "disabled",
                        "run_network_access": "disabled",
                        "scorer_network_access": "disabled"
                    },
                    "execution": {
                        "validation_runs": "public/runs.json",
                        "official_runs": "private-benchmark/runs.json"
                    },
                    "datasets": {
                        "public_dir": "public",
                        "private_benchmark_dir": "private-benchmark",
                        "public_policy": "full",
                        "private_benchmark_policy": "score_only",
                        "private_benchmark_enabled": false
                    }
                },
                "statement_markdown": "# Statement\n\nReturn the sum."
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.toml");
        let workspace_dir = temp.path().join("solution");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "init-solution",
            "sample-sum",
            "--dir",
            workspace_dir.to_str().expect("utf8 path"),
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("init-solution should succeed");

        assert!(output.contains("Initialized solution workspace"));
        assert!(workspace_dir.join("README.md").is_file());
        assert!(workspace_dir.join("agentics.solution.json").is_file());
        assert!(workspace_dir.join(".git/hooks/pre-commit").is_file());
        assert!(!workspace_dir.join("run.sh").exists());
    }

    #[tokio::test]
    async fn submit_packages_workspace_and_posts_authenticated_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/solution-submissions"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "solution_submission-1",
                "status": "queued",
                "challenge_id": "sample-sum",
                "challenge_version_id": "version-1",
                "artifact_path": "solution-submissions/solution_submission-1.zip",
                "evaluation_job_id": "job-1",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).expect("workspace dir");
        write_solution_manifest(&workspace_dir);
        std::fs::write(
            workspace_dir.join("run.sh"),
            "#!/usr/bin/env bash\npython main.py\n",
        )
        .expect("run.sh");
        std::fs::write(workspace_dir.join("main.py"), "print('ok')\n").expect("main.py");
        std::fs::write(workspace_dir.join("ignored.txt"), "ignored").expect("ignored");
        std::fs::write(workspace_dir.join(".gitignore"), "ignored.txt\n").expect("gitignore");

        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "submit",
            "sample-sum",
            "--dir",
            workspace_dir.to_str().expect("utf8 path"),
            "--explanation",
            "first attempt",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("submit should succeed");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body should be JSON");

        assert!(output.contains("Submitted solution_submission-1"));
        assert_eq!(body["challenge_id"], "sample-sum");
        assert_eq!(body["explanation"], "first attempt");
        assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
    }

    #[tokio::test]
    async fn status_fetches_authenticated_solution_submission() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/solution-submissions/solution_submission-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "solution_submission-1",
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_version_id": "version-1",
                "agent_id": "agent-1",
                "agent_name": "solver",
                "status": "queued",
                "explanation": "",
                "parent_solution_submission_id": null,
                "credit_text": "",
                "visible_after_eval": false,
                "artifact_path": "solution-submissions/solution_submission-1.zip",
                "evaluation_job": {
                    "id": "job-1",
                    "status": "queued"
                },
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "status",
            "solution_submission-1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("status should succeed");

        assert!(output.contains("solution submission: solution_submission-1"));
        assert!(output.contains("evaluation_job: job-1 (queued)"));
    }

    #[tokio::test]
    async fn validate_remote_posts_validation_run_and_polls_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/validation-runs"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "validation-1",
                "status": "queued",
                "challenge_id": "sample-sum",
                "challenge_version_id": "version-1",
                "artifact_path": "solution-submissions/validation-1.zip",
                "evaluation_job_id": "job-1",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/validation-runs/validation-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "validation-1",
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_version_id": "version-1",
                "agent_id": "agent-1",
                "agent_name": "solver",
                "status": "completed",
                "explanation": "quick check",
                "parent_solution_submission_id": null,
                "credit_text": "",
                "visible_after_eval": false,
                "artifact_path": "solution-submissions/validation-1.zip",
                "evaluation_job": {
                    "id": "job-1",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "status": "completed",
                    "eval_type": "validation",
                    "primary_score": 1.0,
                    "rank_score": 1.0,
                    "aggregate_metrics": [
                        { "metric_id": "score", "value": 1.0 },
                        { "metric_id": "passed_cases", "value": 2.0 }
                    ],
                    "run_metrics": [],
                    "public_results": [],
                    "validation_summary": {
                        "score": 1.0,
                        "passed": 2,
                        "total": 2
                    }
                },
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:01Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).expect("workspace dir");
        write_solution_manifest(&workspace_dir);
        std::fs::write(
            workspace_dir.join("run.sh"),
            "#!/usr/bin/env bash\npython main.py\n",
        )
        .expect("run.sh");
        std::fs::write(workspace_dir.join("main.py"), "print('ok')\n").expect("main.py");

        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "validate",
            "--remote",
            "sample-sum",
            "--dir",
            workspace_dir.to_str().expect("utf8 path"),
            "--explanation",
            "quick check",
            "--poll-interval-ms",
            "1",
            "--timeout-sec",
            "1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("remote validation should succeed");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");
        let post = requests
            .iter()
            .find(|request| request.url.path() == "/api/validation-runs")
            .expect("validation create request should be recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&post.body).expect("request body should be JSON");

        assert!(output.contains("validation_run: validation-1"));
        assert!(output.contains("validation: completed"));
        assert!(output.contains("primary_score: 1"));
        assert!(output.contains("rank_score: 1"));
        assert!(output.contains("visible_after_eval: false"));
        assert_eq!(body["challenge_id"], "sample-sum");
        assert_eq!(body["explanation"], "quick check");
        assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
    }

    #[tokio::test]
    async fn validate_remote_rejects_disabled_validation_before_packaging() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(false)))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).expect("workspace dir");

        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "validate",
            "--remote",
            "sample-sum",
            "--dir",
            workspace_dir.to_str().expect("utf8 path"),
        ]);

        let error = execute(cli, Environment::default())
            .await
            .expect_err("validation should be rejected before packaging");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");

        assert!(error.to_string().contains("validation pass is disabled"));
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/public/challenges/sample-sum");
    }

    fn challenge_detail_json(validation_enabled: bool) -> serde_json::Value {
        json!({
            "id": "sample-sum",
            "slug": "sample-sum",
            "title": "Sample Sum",
            "summary": "Add numbers",
            "current_version": {
                "id": "version-1",
                "version": "v1"
            },
            "spec": {
                "schema_version": 1,
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_summary": "Add numbers",
                "challenge_version": "v1",
                "solution": {
                    "protocol": "zip_project",
                    "manifest_file": "agentics.solution.json"
                },
                "scorer": {
                    "command": ["python", "scorer/run.py"],
                    "result_file": "result.json"
                },
                "resource_profile": {
                    "id": "python-cpu-small",
                    "solution_image": "python:3.12-slim-bookworm",
                    "scorer_image": "python:3.12-slim-bookworm",
                    "timeout_sec": 30,
                    "memory_limit_mb": 512,
                    "cpu_limit_millis": 1000,
                    "disk_limit_mb": 1024,
                    "setup_network_access": "enabled",
                    "build_network_access": "disabled",
                    "run_network_access": "disabled",
                    "scorer_network_access": "disabled"
                },
                "execution": {
                    "validation_runs": "public/runs.json",
                    "official_runs": "private-benchmark/runs.json"
                },
                "datasets": {
                    "public_dir": "public",
                    "private_benchmark_dir": "private-benchmark",
                    "public_policy": "full",
                    "private_benchmark_policy": "score_only",
                    "validation_enabled": validation_enabled,
                    "private_benchmark_enabled": true
                },
                "metric_schema": {
                    "metrics": [
                        {
                            "id": "score",
                            "label": "Score",
                            "direction": "maximize",
                            "visibility": "public"
                        },
                        {
                            "id": "passed_cases",
                            "label": "Passed Cases",
                            "unit": "cases",
                            "direction": "maximize",
                            "visibility": "public"
                        }
                    ],
                    "ranking": {
                        "primary_metric_id": "score",
                        "tie_breaker_metric_ids": ["passed_cases"]
                    }
                }
            },
            "statement_markdown": "# Sample Sum"
        })
    }
}
