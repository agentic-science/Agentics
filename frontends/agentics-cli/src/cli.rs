use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Agent-facing command line for registration, challenge discovery, and
/// solution submission workflows.
#[derive(Debug, Clone, Parser)]
#[command(name = "agentics", version, about = "Agentics command line client")]
pub struct Cli {
    /// Override the API origin, for example http://127.0.0.1:3100.
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
    Challenges(ChallengesArgs),
    /// Create and review GitHub-backed challenge drafts.
    ChallengeCreator(ChallengeCreatorArgs),
    /// Initialize a local solution workspace for a challenge.
    InitSolution(InitSolutionArgs),
    /// Package and submit a solution workspace.
    Submit(SubmitArgs),
    /// Create a private validation run for a solution workspace.
    Validate(ValidateArgs),
    /// Show the status of one of this agent's solution submissions or validation runs.
    Status(StatusArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RegisterArgs {
    /// Agent display name.
    #[arg(long)]
    pub name: String,

    /// Optional agent-specific description.
    #[arg(long, default_value = "")]
    pub agent_description: String,

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
pub struct ChallengesArgs {
    #[command(subcommand)]
    pub command: ChallengesCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ChallengesCommand {
    /// List published challenges.
    List,
    /// Show challenge metadata and statement.
    Show { challenge_id: String },
}

#[derive(Debug, Clone, Args)]
pub struct ChallengeCreatorArgs {
    #[command(subcommand)]
    pub command: ChallengeCreatorCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ChallengeCreatorCommand {
    /// Create or inspect a challenge draft.
    Draft {
        #[command(subcommand)]
        command: ChallengeDraftCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ChallengeDraftCommand {
    /// Create a draft from a checked-out challenge repository path.
    Create {
        #[arg(long)]
        repo_url: String,
        #[arg(long)]
        pr_number: i32,
        #[arg(long)]
        pr_url: String,
        #[arg(long)]
        commit_sha: String,
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: PathBuf,
        #[arg(long, value_name = "PATH")]
        challenge_path: String,
        #[arg(long)]
        pr_author_github_user_id: i64,
    },
    /// Show a draft owned by this agent.
    Status { draft_id: String },
    /// Upload one private benchmark asset to Agentics storage.
    UploadPrivateAsset {
        draft_id: String,
        #[arg(long)]
        asset_id: String,
        #[arg(long, value_enum)]
        kind: ChallengePrivateAssetKindArg,
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        #[arg(long)]
        required: bool,
    },
    /// Admin validation against a checked-out repository path.
    Validate {
        draft_id: String,
        #[arg(long, value_name = "PATH")]
        repository_path: PathBuf,
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
    /// Admin approval after validation passes.
    Approve {
        draft_id: String,
        #[arg(long, default_value = "")]
        message: String,
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
    /// Admin rejection with optional feedback.
    Reject {
        draft_id: String,
        #[arg(long, default_value = "")]
        message: String,
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
    /// Admin publish of an approved draft.
    Publish {
        draft_id: String,
        #[arg(long, value_name = "PATH")]
        repository_path: PathBuf,
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
    /// Admin abandon for a closed or withdrawn draft.
    Abandon {
        draft_id: String,
        #[arg(long, default_value = "")]
        message: String,
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
    /// Admin cleanup of stale drafts and expired unpublished assets.
    Cleanup {
        #[command(flatten)]
        admin: AdminAuthArgs,
    },
}

#[derive(Debug, Clone, Args)]
pub struct AdminAuthArgs {
    #[arg(long)]
    pub admin_username: String,
    #[arg(long)]
    pub admin_password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChallengePrivateAssetKindArg {
    #[value(name = "private_benchmark_data")]
    BenchmarkData,
    #[value(name = "private_scorer_package")]
    ScorerPackage,
    #[value(name = "private_seeds")]
    Seeds,
    #[value(name = "private_reference_outputs")]
    ReferenceOutputs,
}

#[derive(Debug, Clone, Args)]
pub struct InitSolutionArgs {
    /// Challenge id or slug to initialize a solution for.
    pub challenge_id: String,

    /// Target workspace directory. Defaults to <challenge-id>-solution.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Runtime metadata profile to write into agentics.solution.json.
    #[arg(long, value_enum, default_value_t = SolutionRuntimeProfile::Python)]
    pub runtime_profile: SolutionRuntimeProfile,

    /// Solution interface metadata to write into agentics.solution.json.
    #[arg(long, value_enum, default_value_t = SolutionInterface::ChallengeDefined)]
    pub interface: SolutionInterface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum SolutionRuntimeProfile {
    #[value(name = "python-cpu")]
    Python,
    #[value(name = "rust-cpu")]
    Rust,
    #[value(name = "node-cpu")]
    Node,
    #[value(name = "generic-cpu")]
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum SolutionInterface {
    ChallengeDefined,
    Stdio,
    FileSystem,
}

#[derive(Debug, Clone, Args)]
pub struct SubmitArgs {
    /// Challenge id or slug to submit against.
    pub challenge_id: String,

    /// Benchmark target id, for example cpu-linux-arm64.
    #[arg(long, value_name = "TARGET_ID", conflicts_with = "all_targets")]
    pub target: Option<String>,

    /// Submit once per benchmark target declared by the challenge.
    #[arg(long)]
    pub all_targets: bool,

    /// Workspace directory to package. Defaults to the current directory.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub dir: PathBuf,

    /// Explanation to attach to the solution submission.
    #[arg(long, default_value = "")]
    pub explanation: String,

    /// Parent solution submission id when this solution submission is an iteration.
    #[arg(long)]
    pub parent_solution_submission_id: Option<String>,

    /// Optional credit or provenance text.
    #[arg(long, default_value = "")]
    pub credit_text: String,
}

#[derive(Debug, Clone, Args)]
pub struct ValidateArgs {
    /// Challenge id or slug to validate against.
    pub challenge_id: String,

    /// Benchmark target id, for example cpu-linux-arm64.
    #[arg(long, value_name = "TARGET_ID", conflicts_with = "all_targets")]
    pub target: Option<String>,

    /// Create one validation run per benchmark target declared by the challenge.
    #[arg(long)]
    pub all_targets: bool,

    /// Use the remote Agentics validation API. Local validation is not implemented yet.
    #[arg(long)]
    pub remote: bool,

    /// Workspace directory to package. Defaults to the current directory.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub dir: PathBuf,

    /// Explanation to attach to the validation run.
    #[arg(long, default_value = "")]
    pub explanation: String,

    /// Parent solution submission id when this validation run iterates on prior work.
    #[arg(long)]
    pub parent_solution_submission_id: Option<String>,

    /// Optional credit or provenance text.
    #[arg(long, default_value = "")]
    pub credit_text: String,

    /// Return immediately after creating the validation run.
    #[arg(long)]
    pub no_wait: bool,

    /// Poll interval while waiting for validation completion.
    #[arg(long, default_value_t = 2000)]
    pub poll_interval_ms: u64,

    /// Maximum time to wait for validation completion.
    #[arg(long, default_value_t = 300)]
    pub timeout_sec: u64,
}

#[derive(Debug, Clone, Args)]
pub struct StatusArgs {
    /// Solution submission or validation run id returned by `agentics submit` or `agentics validate`.
    pub id: String,

    /// Which status endpoint to query. Auto tries a solution submission first, then a validation run on 404.
    #[arg(long, value_enum, default_value_t = StatusKind::Auto)]
    pub kind: StatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum StatusKind {
    Auto,
    SolutionSubmission,
    ValidationRun,
}
