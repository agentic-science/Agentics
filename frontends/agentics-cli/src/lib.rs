mod api;
mod cli;
mod config;
mod output;

use anyhow::{Context, Result};
use clap::Parser;
use shared::models::request::RegisterAgentRequest;

use crate::api::ApiClient;
use crate::cli::{
    AuthCommand, Cli, Commands, ConfigCommand, ConfigKey, ProblemsCommand, RegisterArgs,
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
    use wiremock::matchers::{body_json, method, path};
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
}
