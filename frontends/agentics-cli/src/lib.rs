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
#![warn(unused_crate_dependencies)]

mod api;
mod cli;
mod commands;
mod config;
mod output;
mod package;
mod workspace;

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;

use crate::api::ApiClient;
use crate::cli::{
    AuthArgs, AuthCommand, ChallengeCreatorArgs, ChallengeCreatorCommand, ChallengesArgs,
    ChallengesCommand, Cli, Commands, ConfigArgs, ConfigCommand, InitSolutionArgs, LeaderboardArgs,
    LeaderboardCommand, MetricsArgs, MetricsCommand, SubmissionsArgs, SubmissionsCommand,
};
use crate::config::{ConfigStore, Environment, ResolvedSettings};

/// Handles run from env for this module.
pub async fn run_from_env() -> Result<()> {
    let cli = Cli::parse();
    let env = Environment::from_process()?;
    let output = execute(cli, env).await?;
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

#[derive(Clone, Default)]
/// Provides command input streams while keeping production stdin behavior testable.
pub(crate) struct CommandInput {
    stdin: Option<Arc<Mutex<Option<String>>>>,
}

impl CommandInput {
    /// Reads from process stdin for normal CLI execution.
    fn process() -> Self {
        Self { stdin: None }
    }

    /// Provides one test stdin payload.
    #[cfg(test)]
    pub(crate) fn test_stdin(value: impl Into<String>) -> Self {
        Self {
            stdin: Some(Arc::new(Mutex::new(Some(value.into())))),
        }
    }

    /// Reads the configured stdin payload or process stdin.
    pub(crate) fn read_to_string(&self, label: &str) -> Result<String> {
        if let Some(stdin) = &self.stdin {
            let mut guard = stdin
                .lock()
                .map_err(|_| anyhow::anyhow!("test stdin lock poisoned"))?;
            return guard
                .take()
                .ok_or_else(|| anyhow::anyhow!("{label} stdin was already consumed"));
        }

        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .with_context(|| format!("failed to read {label} from stdin"))?;
        Ok(input)
    }
}

/// Handles execute for this module.
pub(crate) async fn execute(cli: Cli, env: Environment) -> Result<String> {
    execute_with_input(cli, env, CommandInput::process()).await
}

/// Handles execute with a caller-provided input source.
pub(crate) async fn execute_with_input(
    cli: Cli,
    env: Environment,
    input: CommandInput,
) -> Result<String> {
    let store = ConfigStore::new(config_path(&cli)?);
    let file_config = store.load()?;
    let output_format = if cli.json {
        crate::cli::OutputFormat::Json
    } else {
        crate::cli::OutputFormat::Table
    };
    let settings = ResolvedSettings::resolve(
        cli.api_base_url.as_deref(),
        cli.token.as_deref(),
        &env,
        &file_config,
        store.path().to_path_buf(),
    )?;

    dispatch_command(
        cli.command,
        output_format,
        &store,
        file_config,
        &settings,
        &input,
    )
    .await
}

async fn dispatch_command(
    command: Commands,
    output_format: crate::cli::OutputFormat,
    store: &ConfigStore,
    file_config: crate::config::CliConfig,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    match command {
        Commands::Register(args) => {
            commands::register(args, output_format, store, file_config, settings).await
        }
        Commands::Auth(args) => dispatch_auth(args, output_format, settings),
        Commands::Config(args) => dispatch_config(args, output_format, store, settings, input),
        Commands::Challenges(args) => dispatch_challenges(args, output_format, settings).await,
        Commands::ChallengeCreator(args) => {
            dispatch_challenge_creator(args, output_format, settings, input).await
        }
        Commands::Admin(args) => {
            commands::admin_command(args, output_format, settings, input).await
        }
        Commands::InitSolution(args) => dispatch_init_solution(args, output_format, settings).await,
        Commands::Submit(args) => commands::submit(args, output_format, settings).await,
        Commands::Validate(args) => commands::validate(args, output_format, settings).await,
        Commands::Submissions(args) => dispatch_submissions(args, output_format, settings).await,
        Commands::Leaderboard(args) => dispatch_leaderboard(args, output_format, settings).await,
        Commands::Metrics(args) => dispatch_metrics(args, output_format, settings).await,
    }
}

fn dispatch_auth(
    args: AuthArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    match args.command {
        AuthCommand::Status => output::render_auth_status(settings, output_format),
    }
}

fn dispatch_config(
    args: ConfigArgs,
    output_format: crate::cli::OutputFormat,
    store: &ConfigStore,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    match args.command {
        ConfigCommand::Show => output::render_auth_status(settings, output_format),
        ConfigCommand::Set { key, value, stdin } => commands::set_config(
            key,
            value.as_deref(),
            stdin,
            output_format,
            store,
            settings,
            input,
        ),
    }
}

async fn dispatch_challenges(
    args: ChallengesArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match args.command {
        ChallengesCommand::List => {
            let response = client.list_challenges().await?;
            output::render_challenge_list(&response, output_format)
        }
        ChallengesCommand::Show { challenge_name } => {
            let response = client.get_challenge(&challenge_name).await?;
            output::render_challenge_detail(&response, output_format)
        }
        ChallengesCommand::Stats {
            challenge_name,
            target,
            metric,
        } => {
            commands::challenge_stats(challenge_name, target, metric, output_format, settings).await
        }
    }
}

async fn dispatch_challenge_creator(
    args: ChallengeCreatorArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let creator = args.creator;
    match args.command {
        ChallengeCreatorCommand::Check { path } => {
            commands::challenge_creator_check(path, output_format).await
        }
        ChallengeCreatorCommand::ReviewRecord { command } => {
            commands::challenge_review_record(command, &creator, output_format, settings, input)
                .await
        }
        ChallengeCreatorCommand::Stats {
            challenge_name,
            target,
        } => {
            commands::creator_stats(
                challenge_name,
                target,
                &creator,
                output_format,
                settings,
                input,
            )
            .await
        }
        ChallengeCreatorCommand::Participants {
            challenge_name,
            target,
        } => {
            commands::creator_participants(
                challenge_name,
                target,
                &creator,
                output_format,
                settings,
                input,
            )
            .await
        }
        ChallengeCreatorCommand::Shortlist { command } => {
            commands::challenge_shortlist(command, &creator, output_format, settings, input).await
        }
    }
}

async fn dispatch_init_solution(
    args: InitSolutionArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_name).await?;
    let summary = workspace::init_solution_workspace(
        &challenge,
        args.dir,
        args.runtime_profile,
        args.interface,
    )?;
    output::render_init_solution(&summary, output_format)
}

async fn dispatch_submissions(
    args: SubmissionsArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match args.command {
        SubmissionsCommand::List {
            challenge_name,
            target,
            limit,
        } => {
            commands::list_public_solution_submissions(
                challenge_name,
                target,
                limit,
                output_format,
                settings,
            )
            .await
        }
        SubmissionsCommand::Show { submission_id } => {
            let response = client
                .get_public_solution_submission(&submission_id)
                .await?;
            output::render_solution_submission_status(&response, output_format)
        }
        SubmissionsCommand::Status { submission_id } => {
            let response = client.get_solution_submission(&submission_id).await?;
            output::render_solution_submission_status(&response, output_format)
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
            output::render_solution_submission_status(&response, output_format)
        }
        SubmissionsCommand::Logs { submission_id } => {
            let response = client.get_solution_submission_logs(&submission_id).await?;
            output::render_solution_submission_logs(&response, output_format)
        }
        SubmissionsCommand::Report { submission_id } => {
            commands::solution_submission_report(submission_id, output_format, settings).await
        }
        SubmissionsCommand::Rank {
            submission_id,
            challenge,
            target,
        } => {
            let challenge_detail = client.get_challenge(&challenge).await?;
            if challenge_detail.spec.target(&target).is_none() {
                anyhow::bail!(
                    "challenge `{}` does not support target `{target}`",
                    challenge_detail.challenge_name
                );
            }
            let response = if settings.token_configured() {
                match client
                    .get_solution_submission_ranking_context(
                        &submission_id,
                        &challenge_detail.challenge_name,
                        &target,
                    )
                    .await
                {
                    Ok(context) => context,
                    Err(error)
                        if ApiClient::is_not_found(&error) || ApiClient::is_forbidden(&error) =>
                    {
                        client
                            .get_public_solution_submission_ranking_context(
                                &submission_id,
                                &challenge_detail.challenge_name,
                                &target,
                            )
                            .await?
                    }
                    Err(error) => return Err(error),
                }
            } else {
                client
                    .get_public_solution_submission_ranking_context(
                        &submission_id,
                        &challenge_detail.challenge_name,
                        &target,
                    )
                    .await?
            };
            output::render_ranking_context(&response, output_format)
        }
    }
}

async fn dispatch_leaderboard(
    args: LeaderboardArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match args.command {
        LeaderboardCommand::Show {
            challenge_name,
            target,
        } => {
            let response = client.get_leaderboard(&challenge_name, &target).await?;
            output::render_leaderboard(&response, output_format)
        }
    }
}

async fn dispatch_metrics(
    args: MetricsArgs,
    output_format: crate::cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match args.command {
        MetricsCommand::Distribution {
            challenge_name,
            target,
            metric,
        } => {
            let response = client
                .get_score_distribution(&challenge_name, &target, &metric)
                .await?;
            output::render_score_distribution(&response, output_format)
        }
    }
}

/// Handles config path for this module.
fn config_path(cli: &Cli) -> Result<std::path::PathBuf> {
    match &cli.config {
        Some(path) => Ok(path.clone()),
        None => ConfigStore::standard_path(),
    }
}

#[cfg(test)]
mod tests;
