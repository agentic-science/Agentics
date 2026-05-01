mod api;
mod cli;
mod config;
mod output;
mod package;
mod workspace;

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use clap::Parser;
use shared::models::request::{CreateSubmissionRequest, RegisterAgentRequest};

use crate::api::ApiClient;
use crate::cli::{
    AuthCommand, Cli, Commands, ConfigCommand, ConfigKey, ProblemsCommand, RegisterArgs,
    SubmitArgs, ValidateArgs,
};
use crate::config::{
    CliConfig, ConfigStore, Environment, ResolvedSettings, normalize_api_base_url,
};

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
            register(args, cli.output, &store, file_config, &settings).await
        }
        Commands::Auth(args) => match args.command {
            AuthCommand::Status => output::render_auth_status(&settings, cli.output),
        },
        Commands::Config(args) => match args.command {
            ConfigCommand::Show => output::render_auth_status(&settings, cli.output),
            ConfigCommand::Set { key, value } => {
                set_config(key, &value, cli.output, &store, &settings)
            }
        },
        Commands::Problems(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            match args.command {
                ProblemsCommand::List => {
                    let response = client.list_problems().await?;
                    output::render_problem_list(&response, cli.output)
                }
                ProblemsCommand::Show { problem_id } => {
                    let response = client.get_problem(&problem_id).await?;
                    output::render_problem_detail(&response, cli.output)
                }
            }
        }
        Commands::InitSolution(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let problem = client.get_problem(&args.problem_id).await?;
            let summary = workspace::init_solution_workspace(&problem, args.dir)?;
            output::render_init_solution(&summary, cli.output)
        }
        Commands::Submit(args) => submit(args, cli.output, &settings).await,
        Commands::Validate(args) => validate(args, cli.output, &settings).await,
        Commands::Status(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let response = client.get_submission(&args.submission_id).await?;
            output::render_submission_status(&response, cli.output)
        }
    }
}

async fn register(
    args: RegisterArgs,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    mut file_config: CliConfig,
    settings: &ResolvedSettings,
) -> Result<String> {
    let model_info = parse_model_info(&args.model_info_json)?;
    let request = RegisterAgentRequest {
        name: args.name,
        description: args.description,
        owner: args.owner,
        model_info,
    };

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client.register(&request).await?;
    let saved_token = !args.no_save_token;
    if saved_token {
        file_config.api_base_url = Some(settings.api_base_url.clone());
        file_config.token = Some(response.token.clone());
        store.save(&file_config)?;
    }

    output::render_register_agent(&response, saved_token, settings, output_format)
}

fn set_config(
    key: ConfigKey,
    value: &str,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    settings: &ResolvedSettings,
) -> Result<String> {
    let mut config = store.load()?;
    let updated_key = match key {
        ConfigKey::ApiBaseUrl => {
            config.api_base_url = Some(normalize_api_base_url(value)?);
            "api_base_url"
        }
        ConfigKey::Token => {
            let token = value.trim();
            if token.is_empty() {
                anyhow::bail!("token must not be empty");
            }
            config.token = Some(token.to_string());
            "token"
        }
    };
    store.save(&config)?;
    output::render_config_set(updated_key, settings, output_format)
}

fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}

async fn submit(
    args: SubmitArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let package = package::package_solution_workspace(&args.dir)?;
    let request = create_submission_request(
        args.problem_id,
        &package,
        args.explanation,
        args.parent_submission_id,
        args.credit_text,
    );

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client.create_submission(&request).await?;

    output::render_create_submission(&response, &package, output_format)
}

async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    if !args.remote {
        bail!("local validation is not implemented yet; pass --remote to use the Agentics API");
    }

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let problem = client.get_problem(&args.problem_id).await?;
    if !problem.spec.datasets.validation_enabled {
        bail!(
            "validation pass is disabled for problem `{}`; submit officially or ask the challenge owner to enable validation",
            problem.id
        );
    }

    let package = package::package_solution_workspace(&args.dir)?;
    let request = create_submission_request(
        args.problem_id,
        &package,
        args.explanation,
        args.parent_submission_id,
        args.credit_text,
    );

    let response = client.create_validation_run(&request).await?;
    if args.no_wait {
        return output::render_create_validation_run(&response, &package, output_format);
    }

    let final_response = poll_validation_run(
        &client,
        &response.id,
        Duration::from_millis(args.poll_interval_ms.max(1)),
        Duration::from_secs(args.timeout_sec),
    )
    .await?;
    output::render_validation_run_status(&final_response, output_format)
}

fn create_submission_request(
    problem_id: String,
    package: &package::SubmissionPackage,
    explanation: String,
    parent_submission_id: Option<String>,
    credit_text: String,
) -> CreateSubmissionRequest {
    CreateSubmissionRequest {
        problem_id,
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation,
        parent_submission_id,
        credit_text,
    }
}

async fn poll_validation_run(
    client: &ApiClient,
    validation_run_id: &str,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<shared::models::request::SubmissionResponse> {
    let deadline = Instant::now() + timeout;
    loop {
        let response = client.get_validation_run(validation_run_id).await?;
        if is_terminal_status(&response.status) {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            bail!("validation run {validation_run_id} did not finish within {timeout:?}");
        }
        tokio::time::sleep(poll_interval).await;
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}

fn config_path(cli: &Cli) -> Result<std::path::PathBuf> {
    match &cli.config {
        Some(path) => Ok(path.clone()),
        None => ConfigStore::default_path(),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::cli::Cli;
    use crate::config::{CliConfig, ConfigStore, Environment};
    use crate::execute;

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
    async fn problems_list_uses_public_api_and_renders_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "id": "sample-sum",
                        "slug": "sum",
                        "title": "Sample Sum",
                        "description": "Add numbers",
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
            "problems",
            "list",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("problem list should succeed");

        assert_eq!(
            output,
            "ID          SLUG  VERSION  TITLE\nsample-sum  sum   v1       Sample Sum"
        );
    }

    #[tokio::test]
    async fn init_solution_fetches_problem_and_creates_workspace() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/problems/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "sample-sum",
                "slug": "sum",
                "title": "Sample Sum",
                "description": "Add numbers",
                "current_version": {
                    "id": "version-1",
                    "version": "v1"
                },
                "spec": {
                    "schema_version": 1,
                    "problem_id": "sample-sum",
                    "problem_title": "Sample Sum",
                    "problem_version": "v1",
                    "submission": {
                        "format": "python_zip_project",
                        "language": "python",
                        "entrypoint": "main.py"
                    },
                    "scorer": {
                        "entrypoint": "scorer/run.py",
                        "result_file": "result.json"
                    },
                    "limits": {
                        "time_limit_sec": 30.0,
                        "memory_limit_mb": 512
                    },
                    "datasets": {
                        "shown_dir": "shown",
                        "hidden_dir": "hidden",
                        "shown_policy": "full",
                        "hidden_policy": "score_only",
                        "heldout_enabled": false
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
        assert!(workspace_dir.join(".git/hooks/pre-commit").is_file());
        assert!(!workspace_dir.join("run.sh").exists());
    }

    #[tokio::test]
    async fn submit_packages_workspace_and_posts_authenticated_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/submissions"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "submission-1",
                "status": "queued",
                "problem_id": "sample-sum",
                "problem_version_id": "version-1",
                "artifact_path": "submissions/submission-1.zip",
                "evaluation_job_id": "job-1",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_dir = temp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).expect("workspace dir");
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

        assert!(output.contains("Submitted submission-1"));
        assert_eq!(body["problem_id"], "sample-sum");
        assert_eq!(body["explanation"], "first attempt");
        assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
    }

    #[tokio::test]
    async fn status_fetches_authenticated_submission() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/submissions/submission-1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "submission-1",
                "problem_id": "sample-sum",
                "problem_title": "Sample Sum",
                "problem_version_id": "version-1",
                "agent_id": "agent-1",
                "agent_name": "solver",
                "status": "queued",
                "explanation": "",
                "parent_submission_id": null,
                "credit_text": "",
                "visible_after_eval": false,
                "artifact_path": "submissions/submission-1.zip",
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
            "submission-1",
        ]);

        let output = execute(cli, Environment::default())
            .await
            .expect("status should succeed");

        assert!(output.contains("submission: submission-1"));
        assert!(output.contains("evaluation_job: job-1 (queued)"));
    }

    #[tokio::test]
    async fn validate_remote_posts_validation_run_and_polls_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/problems/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_detail_json(true)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/validation-runs"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "validation-1",
                "status": "queued",
                "problem_id": "sample-sum",
                "problem_version_id": "version-1",
                "artifact_path": "submissions/validation-1.zip",
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
                "problem_id": "sample-sum",
                "problem_title": "Sample Sum",
                "problem_version_id": "version-1",
                "agent_id": "agent-1",
                "agent_name": "solver",
                "status": "completed",
                "explanation": "quick check",
                "parent_submission_id": null,
                "credit_text": "",
                "visible_after_eval": false,
                "artifact_path": "submissions/validation-1.zip",
                "evaluation_job": {
                    "id": "job-1",
                    "status": "completed"
                },
                "evaluation": {
                    "id": "eval-1",
                    "status": "completed",
                    "eval_type": "validation",
                    "primary_score": 1.0,
                    "shown_results": [],
                    "hidden_summary": {
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
        assert!(output.contains("visible_after_eval: false"));
        assert_eq!(body["problem_id"], "sample-sum");
        assert_eq!(body["explanation"], "quick check");
        assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
    }

    #[tokio::test]
    async fn validate_remote_rejects_disabled_validation_before_packaging() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/problems/sample-sum"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_detail_json(false)))
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
        assert_eq!(requests[0].url.path(), "/api/public/problems/sample-sum");
    }

    fn problem_detail_json(validation_enabled: bool) -> serde_json::Value {
        json!({
            "id": "sample-sum",
            "slug": "sample-sum",
            "title": "Sample Sum",
            "description": "Add numbers",
            "current_version": {
                "id": "version-1",
                "version": "v1"
            },
            "spec": {
                "schema_version": 1,
                "problem_id": "sample-sum",
                "problem_title": "Sample Sum",
                "problem_version": "v1",
                "submission": {
                    "format": "python_zip_project",
                    "language": "python",
                    "entrypoint": "main.py"
                },
                "scorer": {
                    "entrypoint": "scorer/run.py",
                    "result_file": "result.json"
                },
                "limits": {
                    "time_limit_sec": 2.0,
                    "memory_limit_mb": 128
                },
                "datasets": {
                    "shown_dir": "shown",
                    "hidden_dir": "hidden",
                    "shown_policy": "full",
                    "hidden_policy": "score_only",
                    "validation_enabled": validation_enabled,
                    "heldout_enabled": true,
                    "heldout_dir": "heldout"
                }
            },
            "statement_markdown": "# Sample Sum"
        })
    }
}
