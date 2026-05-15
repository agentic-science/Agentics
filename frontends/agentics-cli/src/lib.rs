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

use anyhow::Result;
use clap::Parser;

use crate::api::ApiClient;
use crate::cli::{
    AuthCommand, ChallengeCreatorCommand, ChallengesCommand, Cli, Commands, ConfigCommand,
    LeaderboardCommand, MetricsCommand, SubmissionsCommand,
};
use crate::config::{ConfigStore, Environment, ResolvedSettings};

/// Handles run from env for this module.
pub async fn run_from_env() -> Result<()> {
    let cli = Cli::parse();
    let env = Environment::from_process();
    let output = execute(cli, env).await?;
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

/// Handles execute for this module.
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
                ChallengesCommand::Show { challenge_name } => {
                    let response = client.get_challenge(&challenge_name).await?;
                    output::render_challenge_detail(&response, cli.output)
                }
            }
        }
        Commands::ChallengeCreator(args) => match args.command {
            ChallengeCreatorCommand::Draft { command } => {
                commands::challenge_draft(command, cli.output, &settings).await
            }
            ChallengeCreatorCommand::Stats {
                challenge_name: _,
                target: _,
            } => {
                anyhow::bail!(
                    "creator stats require GitHub OAuth web-session support; use the creator web UI"
                )
            }
            ChallengeCreatorCommand::Participants {
                challenge_name: _,
                target: _,
            } => {
                anyhow::bail!(
                    "creator participants require GitHub OAuth web-session support; use the creator web UI"
                )
            }
            ChallengeCreatorCommand::Shortlist { command } => {
                commands::challenge_shortlist(command, cli.output, &settings)
            }
        },
        Commands::InitSolution(args) => {
            let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
            let challenge = client.get_challenge(&args.challenge_name).await?;
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
                    target,
                } => {
                    let challenge_detail = client.get_challenge(&challenge).await?;
                    if challenge_detail.spec.target(&target).is_none() {
                        anyhow::bail!(
                            "challenge `{}` does not support target `{target}`",
                            challenge_detail.name
                        );
                    }
                    let response = client
                        .get_solution_submission_ranking_context(
                            &submission_id,
                            &challenge_detail.name,
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
                    challenge_name,
                    target,
                } => {
                    let response = client.get_leaderboard(&challenge_name, &target).await?;
                    output::render_leaderboard(&response, cli.output)
                }
            }
        }
        Commands::Metrics(args) => {
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
                    output::render_score_distribution(&response, cli.output)
                }
            }
        }
    }
}

/// Handles config path for this module.
fn config_path(cli: &Cli) -> Result<std::path::PathBuf> {
    match &cli.config {
        Some(path) => Ok(path.clone()),
        None => ConfigStore::default_path(),
    }
}

#[cfg(test)]
mod tests;
