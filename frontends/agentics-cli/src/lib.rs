#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::enum_glob_use,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::wildcard_imports,
        reason = "unit tests use direct assertions and fixture indexing for concise failure diagnostics"
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

use crate::api::ApiClient;
use crate::cli::{
    AuthCommand, ChallengeCreatorCommand, ChallengesCommand, Cli, Commands, ConfigCommand,
    LeaderboardCommand, MetricsCommand, RoundsCommand, SubmissionsCommand,
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
        Commands::Rounds(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                RoundsCommand::List { challenge_id } => {
                    let response = client.get_challenge(&challenge_id).await?;
                    output::render_round_list(&response, cli.output)
                }
                RoundsCommand::Show {
                    challenge_id,
                    round_id,
                } => {
                    let response = client.get_challenge(&challenge_id).await?;
                    output::render_round_detail(&response, &round_id, cli.output)
                }
            }
        }
        Commands::Submissions(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                SubmissionsCommand::Show { submission_id } => {
                    let response = client.get_solution_submission(&submission_id).await?;
                    output::render_solution_submission_status(&response, cli.output)
                }
                SubmissionsCommand::Wait {
                    submission_id,
                    poll_interval_ms,
                    timeout_sec,
                } => {
                    let response = commands::wait_for_solution_submission(
                        &client,
                        &submission_id,
                        std::time::Duration::from_millis(poll_interval_ms.max(1)),
                        std::time::Duration::from_secs(timeout_sec),
                    )
                    .await?;
                    output::render_solution_submission_status(&response, cli.output)
                }
                SubmissionsCommand::Logs { submission_id } => {
                    let response = client.get_solution_submission_logs(&submission_id).await?;
                    output::render_solution_submission_logs(&response, cli.output)
                }
                SubmissionsCommand::Rank {
                    submission_id,
                    challenge,
                    round,
                    target,
                } => {
                    let response = client
                        .get_solution_submission_ranking_context(
                            &submission_id,
                            &challenge,
                            &round,
                            &target,
                        )
                        .await?;
                    output::render_ranking_context(&response, cli.output)
                }
            }
        }
        Commands::Leaderboard(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                LeaderboardCommand::Show {
                    challenge_id,
                    round,
                    target,
                } => {
                    let response = client
                        .get_leaderboard(&challenge_id, &round, &target)
                        .await?;
                    output::render_leaderboard(&response, cli.output)
                }
            }
        }
        Commands::Metrics(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                MetricsCommand::Distribution {
                    challenge_id,
                    round,
                    target,
                    metric,
                } => {
                    let response = client
                        .get_score_distribution(&challenge_id, &round, &target, &metric)
                        .await?;
                    output::render_score_distribution(&response, cli.output)
                }
            }
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
                        "rounds": [round_json()]
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
            "ID          SLUG  ROUNDS  TITLE\nsample-sum  sum   main    Sample Sum"
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
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
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
            "--round",
            "main",
            "--target",
            "linux-arm64-cpu",
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
        assert_eq!(body["round_id"], "main");
        assert_eq!(body["benchmark_target_id"], "linux-arm64-cpu");
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
            "--round",
            "main",
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
    async fn submissions_show_fetches_authenticated_solution_submission() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/solution-submissions/solution_submission-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "solution_submission-1",
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
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
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
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
            "submissions",
            "show",
            "solution_submission-1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("submissions show should succeed");

        assert!(output.contains("solution submission: solution_submission-1"));
        assert!(output.contains("evaluation_job: job-1 (queued)"));
    }

    #[tokio::test]
    async fn submissions_show_fetches_validation_run_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/solution-submissions/validation-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "validation-1",
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
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
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
                    "status": "completed",
                    "eval_type": "validation",
                    "primary_score": 1.0,
                    "rank_score": 1.0,
                    "aggregate_metrics": [],
                    "run_metrics": [],
                    "public_results": []
                },
                "validation_evaluation": {
                    "id": "eval-1",
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
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
            "submissions",
            "show",
            "validation-1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("submissions show should read validation ids through solution submissions");

        assert!(output.contains("solution submission: validation-1"));
        assert!(output.contains("validation_evaluation: completed"));
    }

    #[test]
    fn old_status_command_is_removed() {
        let result = Cli::try_parse_from(["agentics", "status", "submission-1"]);

        assert!(result.is_err());
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
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
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
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
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
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "round_id": "main",
                    "benchmark_target_id": "linux-arm64-cpu",
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
            "--round",
            "main",
            "--target",
            "linux-arm64-cpu",
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
        assert_eq!(body["round_id"], "main");
        assert_eq!(body["benchmark_target_id"], "linux-arm64-cpu");
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
            "--round",
            "main",
            "--target",
            "linux-arm64-cpu",
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
            "rounds": [round_json()],
            "spec": {
                "schema_version": 1,
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_summary": "Add numbers",
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
                        "id": "linux-arm64-cpu",
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
                "rounds": [round_json()],
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

    fn round_json() -> serde_json::Value {
        json!({
            "id": "main",
            "title": "Main Round",
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_after_close"
            },
            "solution_publication": "submitter_opt_in"
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
            "bundle_path": "v1",
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
