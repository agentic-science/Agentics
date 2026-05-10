#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod api;
mod cli;
mod commands;
mod config;
mod output;
mod package;
mod workspace;

use anyhow::Result;
use clap::Parser;

use crate::api::{ApiClient, ApiStatusError};
use crate::cli::{
    AuthCommand, ChallengeCreatorCommand, ChallengesCommand, Cli, Commands, ConfigCommand,
    StatusKind,
};
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

pub(crate) async fn execute(cli: Cli, env: Environment) -> Result<String> {
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
        Commands::ChallengeCreator(args) => match args.command {
            ChallengeCreatorCommand::Draft { command } => {
                commands::challenge_draft(command, cli.output, &settings).await
            }
        },
        Commands::InitSolution(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let challenge = client.get_challenge(&args.challenge_id).await?;
            let summary = workspace::init_solution_workspace(
                &challenge,
                args.dir,
                args.runtime_profile,
                args.interface,
            )?;
            output::render_init_solution(&summary, cli.output)
        }
        Commands::Submit(args) => commands::submit(args, cli.output, &settings).await,
        Commands::Validate(args) => commands::validate(args, cli.output, &settings).await,
        Commands::Status(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.kind {
                StatusKind::SolutionSubmission => {
                    let response = client.get_solution_submission(&args.id).await?;
                    output::render_solution_submission_status(&response, cli.output)
                }
                StatusKind::ValidationRun => {
                    let response = client.get_validation_run(&args.id).await?;
                    output::render_validation_run_status(&response, cli.output)
                }
                StatusKind::Auto => match client.get_solution_submission(&args.id).await {
                    Ok(response) => {
                        output::render_solution_submission_status(&response, cli.output)
                    }
                    Err(error) if is_not_found(&error) => {
                        let response = client.get_validation_run(&args.id).await?;
                        output::render_validation_run_status(&response, cli.output)
                    }
                    Err(error) => Err(error),
                },
            }
        }
    }
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<ApiStatusError>()
        .is_some_and(|api_error| api_error.status() == reqwest::StatusCode::NOT_FOUND)
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
                "agent_description": "",
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
            .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
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
        Mock::given(method("GET"))
            .and(path("/api/public/challenges/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/solution-submissions"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "solution_submission-1",
                "status": "queued",
                "challenge_id": "sample-sum",
                "challenge_version_id": "version-1",
                "benchmark_target_id": "cpu-linux-arm64",
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
            "--target",
            "cpu-linux-arm64",
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
        let post = requests
            .iter()
            .find(|request| request.url.path() == "/api/solution-submissions")
            .expect("solution submission create request should be recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&post.body).expect("request body should be JSON");

        assert!(output.contains("Submitted solution_submission-1"));
        assert_eq!(body["challenge_id"], "sample-sum");
        assert_eq!(body["benchmark_target_id"], "cpu-linux-arm64");
        assert_eq!(body["explanation"], "first attempt");
        assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
    }

    #[tokio::test]
    async fn submit_rejects_unknown_target_before_packaging() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
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
            "submit",
            "sample-sum",
            "--target",
            "cpu-linux-ppc64le",
            "--dir",
            workspace_dir.to_str().expect("utf8 path"),
        ]);

        let error = execute(cli, Environment::default())
            .await
            .expect_err("unknown target should be rejected before packaging");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");

        assert!(error.to_string().contains("benchmark target"));
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/public/challenges/sample-sum");
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
                "benchmark_target_id": "cpu-linux-arm64",
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
                    "benchmark_target_id": "cpu-linux-arm64",
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
    async fn status_falls_back_to_validation_run() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/solution-submissions/validation-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": "not_found",
                "message": "solution submission not found"
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
                "benchmark_target_id": "cpu-linux-arm64",
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
                    "benchmark_target_id": "cpu-linux-arm64",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "benchmark_target_id": "cpu-linux-arm64",
                    "status": "completed",
                    "eval_type": "validation",
                    "primary_score": 1.0,
                    "rank_score": 1.0,
                    "aggregate_metrics": [],
                    "run_metrics": [],
                    "public_results": []
                },
                "created_at": "2026-05-01T00:00:00Z",
                "updated_at": "2026-05-01T00:00:01Z"
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
            "validation-1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("status should succeed");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");

        assert!(output.contains("validation_run: validation-1"));
        assert!(output.contains("validation: completed"));
        assert!(
            requests
                .iter()
                .any(|request| request.url.path() == "/api/solution-submissions/validation-1")
        );
        assert!(
            requests
                .iter()
                .any(|request| request.url.path() == "/api/validation-runs/validation-1")
        );
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
                "benchmark_target_id": "cpu-linux-arm64",
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
                "benchmark_target_id": "cpu-linux-arm64",
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
                    "benchmark_target_id": "cpu-linux-arm64",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "benchmark_target_id": "cpu-linux-arm64",
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
        assert_eq!(body["benchmark_target_id"], "cpu-linux-arm64");
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

    #[tokio::test]
    async fn challenge_creator_creates_draft_from_repo_manifest() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/creator/challenge-drafts"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(challenge_draft_json("draft")))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let challenge_root = temp.path().join("challenges/sample-sum");
        std::fs::create_dir_all(&challenge_root).expect("challenge root");
        std::fs::write(
            challenge_root.join("agentics.challenge.json"),
            challenge_manifest_json().to_string(),
        )
        .expect("manifest");
        let config_path = temp.path().join("config.toml");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "challenge-creator",
            "draft",
            "create",
            "--repo-url",
            "https://github.com/agentics-reifying/agentics-challenges",
            "--pr-number",
            "7",
            "--pr-url",
            "https://github.com/agentics-reifying/agentics-challenges/pull/7",
            "--commit-sha",
            "0123456789abcdef",
            "--repo-dir",
            temp.path().to_str().expect("utf8 path"),
            "--challenge-path",
            "challenges/sample-sum",
            "--pr-author-github-user-id",
            "1001",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("draft create should succeed");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body");

        assert!(output.contains("challenge_draft: draft-1"));
        assert_eq!(body["manifest"]["request"], "new_challenge");
        assert_eq!(body["challenge_path"], "challenges/sample-sum");
    }

    #[tokio::test]
    async fn challenge_creator_uploads_private_asset_file() {
        let server = MockServer::start().await;
        let encoded_asset = {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            STANDARD.encode(b"private zip bytes")
        };
        Mock::given(method("POST"))
            .and(path("/api/creator/challenge-drafts/draft-1/private-assets"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(json!({
                "asset_id": "official-cases",
                "kind": "private_benchmark_data",
                "required": true,
                "asset_base64": encoded_asset
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "asset-row-1",
                "draft_id": "draft-1",
                "asset_id": "official-cases",
                "kind": "private_benchmark_data",
                "required": true,
                "size_bytes": 17,
                "sha256": "asset-sha",
                "storage_uri": "storage/challenge-drafts/draft-1/private-assets/official-cases.bin",
                "uploader_agent_id": "agent-1",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("config.toml");
        let asset_path = temp.path().join("official-cases.zip");
        std::fs::write(&asset_path, b"private zip bytes").expect("asset file");
        let cli = Cli::parse_from([
            "agentics",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--api-base-url",
            &server.uri(),
            "--token",
            "test-token",
            "challenge-creator",
            "draft",
            "upload-private-asset",
            "draft-1",
            "--asset-id",
            "official-cases",
            "--kind",
            "private_benchmark_data",
            "--file",
            asset_path.to_str().expect("utf8 path"),
            "--required",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("asset upload should succeed");

        assert!(output.contains("private_asset: asset-row-1"));
        assert!(output.contains("asset_id: official-cases"));
    }

    #[tokio::test]
    async fn challenge_creator_validates_draft_with_admin_auth() {
        let server = MockServer::start().await;
        let admin_auth = format!("Basic {}", {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            STANDARD.encode("admin:secret")
        });
        Mock::given(method("POST"))
            .and(path("/admin/challenge-drafts/draft-1/validate"))
            .and(header("authorization", admin_auth))
            .and(body_json(json!({ "repository_path": "/tmp/challenges" })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(challenge_draft_json("validated")),
            )
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
            "challenge-creator",
            "draft",
            "validate",
            "draft-1",
            "--repository-path",
            "/tmp/challenges",
            "--admin-username",
            "admin",
            "--admin-password",
            "secret",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("admin validation should succeed");

        assert!(output.contains("status: validated"));
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
                "benchmark_targets": [
                    {
                        "id": "cpu-linux-arm64",
                        "docker_platform": "linux/arm64",
                        "accelerator": "cpu",
                        "validation_enabled": validation_enabled,
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
                        }
                    }
                ],
                "execution": {
                    "validation_runs": "public/runs.json",
                    "official_runs": "private-benchmark/runs.json"
                },
                "datasets": {
                    "public_dir": "public",
                    "private_benchmark_dir": "private-benchmark",
                    "public_policy": "full",
                    "private_benchmark_policy": "score_only",
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

    fn challenge_manifest_json() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_id": "sample-sum",
            "title": "Sample Sum",
            "summary": "Add numbers",
            "readme_path": "README.md",
            "version": {
                "version": "v1",
                "bundle_path": "versions/v1"
            },
            "private_assets": [
                {
                    "asset_id": "official-cases",
                    "kind": "private_benchmark_data",
                    "required": true
                }
            ]
        })
    }

    fn challenge_draft_json(status: &str) -> serde_json::Value {
        json!({
            "id": "draft-1",
            "challenge_id": "sample-sum",
            "request": "new_challenge",
            "status": status,
            "creator_agent_id": "agent-1",
            "creator_github_user_id": 1001,
            "creator_github_login": "creator",
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 7,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
            "commit_sha": "0123456789abcdef",
            "challenge_path": "challenges/sample-sum",
            "manifest_sha256": "abc123",
            "manifest": challenge_manifest_json(),
            "private_assets": [],
            "validation_records": [],
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:00Z"
        })
    }
}
