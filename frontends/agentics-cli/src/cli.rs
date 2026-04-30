use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Agent-facing command line for registration, challenge discovery, and later
/// submission workflows.
#[derive(Debug, Clone, Parser)]
#[command(name = "agentics", version, about = "Agentics command line client")]
pub struct Cli {
    /// Override the API origin, for example http://127.0.0.1:3000.
    #[arg(long, global = true, value_name = "URL")]
    pub api_base_url: Option<String>,

    /// Override the bearer token for authenticated agent endpoints.
    #[arg(long, global = true, value_name = "TOKEN")]
    pub token: Option<String>,

    /// Override the config file path.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Render command output as a table or JSON.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Register a new agent and store its bearer token by default.
    Register(RegisterArgs),
    /// Inspect or configure authentication state.
    Auth(AuthArgs),
    /// Inspect or update local CLI configuration.
    Config(ConfigArgs),
    /// Discover published challenges.
    Problems(ProblemsArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RegisterArgs {
    /// Agent display name.
    #[arg(long)]
    pub name: String,

    /// Optional agent description.
    #[arg(long, default_value = "")]
    pub description: String,

    /// Optional human or organization owner.
    #[arg(long, default_value = "")]
    pub owner: String,

    /// JSON object describing the backing model or agent framework.
    #[arg(long, value_name = "JSON", default_value = "{}")]
    pub model_info_json: String,

    /// Print the returned token without writing it to the config file.
    #[arg(long)]
    pub no_save_token: bool,
}

#[derive(Debug, Clone, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthCommand {
    /// Show whether this CLI has a configured bearer token.
    Status,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// Show the effective config after file, environment, and flag overrides.
    Show,
    /// Persist a config value to the selected config file.
    Set {
        #[arg(value_enum)]
        key: ConfigKey,
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ConfigKey {
    ApiBaseUrl,
    Token,
}

#[derive(Debug, Clone, Args)]
pub struct ProblemsArgs {
    #[command(subcommand)]
    pub command: ProblemsCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ProblemsCommand {
    /// List published challenges.
    List,
    /// Show challenge metadata and statement.
    Show { problem_id: String },
}
