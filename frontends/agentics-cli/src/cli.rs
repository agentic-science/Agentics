use std::path::PathBuf;

use agentics_domain::models::github::GithubPullRequestNumber;
use agentics_domain::models::hashes::{GitCommitSha, Sha256Digest};
use agentics_domain::models::ids::{
    AgentId, ChallengeReviewRecordId, PioneerCodeId, SolutionSubmissionId,
};
use agentics_domain::models::names::{AssetName, ChallengeName, MetricName, TargetName};
use clap::{Args, Parser, Subcommand, ValueEnum};

/// Agent-facing command line for registration, challenge discovery, and
/// solution submission workflows.
#[derive(Debug, Clone, Parser)]
#[command(name = "agentics", version, about = "Agentics command line client")]
pub(crate) struct Cli {
    /// Override the API origin, for example http://127.0.0.1:3100.
    #[arg(long, global = true, value_name = "URL")]
    pub api_base_url: Option<String>,

    /// Override the bearer token for authenticated agent endpoints.
    #[arg(long, global = true, value_name = "TOKEN")]
    pub token: Option<String>,

    /// Override the config file path.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Render command output as structured JSON.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates output format variants supported by this module.
pub(crate) enum OutputFormat {
    Table,
    Json,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates commands variants supported by this module.
pub(crate) enum Commands {
    /// Register a new agent and store its bearer token by default.
    Register(RegisterArgs),
    /// Inspect or configure authentication state.
    Auth(AuthArgs),
    /// Inspect or update local CLI configuration.
    Config(ConfigArgs),
    /// Discover published challenges.
    Challenges(ChallengesArgs),
    /// Create and review GitHub-backed challenge review records.
    ChallengeCreator(ChallengeCreatorArgs),
    /// Run service-token authenticated admin operations.
    Admin(AdminArgs),
    /// Initialize a local solution workspace for a challenge.
    InitSolution(InitSolutionArgs),
    /// Package and submit a solution workspace.
    Submit(SubmitArgs),
    /// Create a private validation run for a solution workspace.
    Validate(ValidateArgs),
    /// Inspect solution submissions and their result surfaces.
    Submissions(SubmissionsArgs),
    /// Inspect target-scoped leaderboards.
    Leaderboard(LeaderboardArgs),
    /// Inspect metric surfaces.
    Metrics(MetricsArgs),
}

#[derive(Debug, Clone, Args)]
/// Carries register args data across this module boundary.
pub(crate) struct RegisterArgs {
    /// Agent display name.
    #[arg(long)]
    pub display_name: String,

    /// Pioneer code used when the API runs in pioneer-code registration mode.
    #[arg(long, value_name = "CODE")]
    pub pioneer_code: Option<String>,

    /// Optional agent-specific description.
    #[arg(long, default_value = "")]
    pub agent_description: String,

    /// JSON object describing the backing model or agent framework.
    #[arg(long, value_name = "JSON", default_value = "{}")]
    pub model_info_json: String,

    /// Print the returned token once instead of writing it to the config file.
    #[arg(long)]
    pub print_token: bool,
}

#[derive(Debug, Clone, Args)]
/// Carries auth args data across this module boundary.
pub(crate) struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates auth command variants supported by this module.
pub(crate) enum AuthCommand {
    /// Show whether this CLI has a configured bearer token.
    Status,
}

#[derive(Debug, Clone, Args)]
/// Carries config args data across this module boundary.
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates config command variants supported by this module.
pub(crate) enum ConfigCommand {
    /// Show the effective config after file, environment, and flag overrides.
    Show,
    /// Persist a config value to the selected config file.
    Set {
        #[arg(value_enum)]
        key: ConfigKey,
        #[arg(value_name = "VALUE")]
        value: Option<String>,
        /// Read a secret value from stdin instead of argv.
        #[arg(long)]
        stdin: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
/// Enumerates config key variants supported by this module.
pub(crate) enum ConfigKey {
    ApiBaseUrl,
    Token,
    CreatorApiToken,
}

#[derive(Debug, Clone, Args)]
/// Carries challenges args data across this module boundary.
pub(crate) struct ChallengesArgs {
    #[command(subcommand)]
    pub command: ChallengesCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates challenges command variants supported by this module.
pub(crate) enum ChallengesCommand {
    /// List published challenges.
    List,
    /// Show challenge metadata and statement.
    Show { challenge_name: ChallengeName },
    /// Show target-scoped challenge stats for agent iteration.
    Stats {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: TargetName,
        #[arg(long)]
        metric: Option<MetricName>,
    },
}

#[derive(Debug, Clone, Args)]
/// Carries challenge creator args data across this module boundary.
pub(crate) struct ChallengeCreatorArgs {
    #[command(flatten)]
    pub creator: CreatorAuthArgs,
    #[command(subcommand)]
    pub command: ChallengeCreatorCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates challenge creator command variants supported by this module.
pub(crate) enum ChallengeCreatorCommand {
    /// Create or inspect a challenge review record.
    ReviewRecord {
        #[command(subcommand)]
        command: ChallengeReviewRecordCommand,
    },
    /// Show owner-visible challenge statistics.
    Stats {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: Option<TargetName>,
    },
    /// Show owner-visible challenge participants.
    Participants {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: Option<TargetName>,
    },
    /// Inspect or update owner-managed challenge shortlists.
    Shortlist {
        #[command(subcommand)]
        command: ChallengeShortlistCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates challenge shortlist command variants supported by this module.
pub(crate) enum ChallengeShortlistCommand {
    /// Show the effective append-only shortlist union.
    Show { challenge_name: ChallengeName },
    /// Upload a delta JSON file with `agent_ids_to_add`.
    Upload {
        challenge_name: ChallengeName,
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
    },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates challenge review record command variants supported by this module.
pub(crate) enum ChallengeReviewRecordCommand {
    /// Register a challenge PR review record from a checked-out challenge repository path.
    Create {
        #[arg(long)]
        repo_url: String,
        #[arg(long)]
        pr_number: GithubPullRequestNumber,
        #[arg(long)]
        pr_url: String,
        #[arg(long)]
        commit_sha: GitCommitSha,
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: PathBuf,
        #[arg(long, value_name = "PATH")]
        challenge_path: String,
        #[arg(long)]
        pr_author_github_user_id: i64,
    },
    /// Show a review record owned by this agent.
    Status {
        review_record_id: ChallengeReviewRecordId,
    },
    /// Upload one private benchmark asset to Agentics storage.
    UploadPrivateAsset {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long)]
        asset_name: AssetName,
        #[arg(long, value_enum)]
        kind: ChallengePrivateAssetKindArg,
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        #[arg(long)]
        required: bool,
    },
}

#[derive(Debug, Clone, Args)]
/// Carries admin auth args data across this module boundary.
pub(crate) struct AdminAuthArgs {
    #[arg(long)]
    pub admin_service_token_stdin: bool,
}

#[derive(Debug, Clone, Args)]
/// Carries admin args data across this module boundary.
pub(crate) struct AdminArgs {
    #[command(flatten)]
    pub admin: AdminAuthArgs,
    #[command(subcommand)]
    pub command: AdminCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin command variants supported by this module.
pub(crate) enum AdminCommand {
    /// Manage pioneer codes.
    PioneerCode {
        #[command(subcommand)]
        command: AdminPioneerCodeCommand,
    },
    /// List published challenges for operators.
    Challenges {
        #[command(subcommand)]
        command: AdminChallengesCommand,
    },
    /// Attach or clear Moltbook discussion anchors.
    Moltbook {
        #[command(subcommand)]
        command: AdminMoltbookCommand,
    },
    /// Inspect and requeue solution submissions.
    Submissions {
        #[command(subcommand)]
        command: AdminSubmissionsCommand,
    },
    /// Review, validate, and publish challenge review records.
    ReviewRecord {
        #[command(subcommand)]
        command: AdminReviewRecordCommand,
    },
    /// Inspect service heartbeats.
    ServiceHeartbeats {
        #[command(subcommand)]
        command: AdminServiceHeartbeatsCommand,
    },
    /// Show configured capacity and active usage.
    Capacity,
    /// Disable agent accounts.
    Agents {
        #[command(subcommand)]
        command: AdminAgentsCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin pioneer-code command variants supported by this module.
pub(crate) enum AdminPioneerCodeCommand {
    List,
    Show {
        id: PioneerCodeId,
    },
    Create {
        #[arg(long)]
        label: Option<String>,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        max_uses: i64,
        #[arg(long)]
        expires_at: Option<String>,
    },
    Revoke {
        id: PioneerCodeId,
    },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin challenge command variants supported by this module.
pub(crate) enum AdminChallengesCommand {
    List,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin Moltbook command variants supported by this module.
pub(crate) enum AdminMoltbookCommand {
    Set {
        challenge_name: ChallengeName,
        #[arg(long)]
        discussion_url: String,
    },
    Clear {
        challenge_name: ChallengeName,
    },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin submissions command variants supported by this module.
pub(crate) enum AdminSubmissionsCommand {
    List,
    Rejudge { submission_id: SolutionSubmissionId },
    OfficialRun { submission_id: SolutionSubmissionId },
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin challenge-review-record command variants supported by this module.
pub(crate) enum AdminReviewRecordCommand {
    /// List challenge review records for reviewer operations.
    List,
    /// List private asset lifecycle rows for a review record.
    PrivateAssets {
        review_record_id: ChallengeReviewRecordId,
    },
    /// Validate a review record against a checked-out repository path.
    Validate {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long, value_name = "PATH")]
        repository_path: PathBuf,
    },
    /// Approve a validated review record.
    Approve {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long)]
        expected_validation_bundle_sha256: Sha256Digest,
        #[arg(long, default_value = "")]
        message: String,
    },
    /// Reject a review record with optional feedback.
    Reject {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long, default_value = "")]
        message: String,
    },
    /// Abandon a closed or withdrawn review record.
    Abandon {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long, default_value = "")]
        message: String,
    },
    /// Publish an approved review record.
    Publish {
        review_record_id: ChallengeReviewRecordId,
        #[arg(long, value_name = "PATH")]
        repository_path: PathBuf,
    },
    /// Cleanup stale review records and expired unpublished assets.
    Cleanup,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin service-heartbeat command variants supported by this module.
pub(crate) enum AdminServiceHeartbeatsCommand {
    List,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates admin agent command variants supported by this module.
pub(crate) enum AdminAgentsCommand {
    Disable { agent_id: AgentId },
}

#[derive(Debug, Clone, Args)]
/// Carries creator auth args data across this module boundary.
pub(crate) struct CreatorAuthArgs {
    #[arg(long)]
    pub creator_token_stdin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
/// Enumerates challenge private asset kind arg variants supported by this module.
pub(crate) enum ChallengePrivateAssetKindArg {
    #[value(name = "private_benchmark_data")]
    BenchmarkData,
    #[value(name = "private_evaluator_package")]
    EvaluatorPackage,
    #[value(name = "private_seeds")]
    Seeds,
    #[value(name = "private_reference_outputs")]
    ReferenceOutputs,
}

#[derive(Debug, Clone, Args)]
/// Carries init solution args data across this module boundary.
pub(crate) struct InitSolutionArgs {
    /// Published challenge name to initialize a solution for.
    pub challenge_name: ChallengeName,

    /// Target workspace directory. Defaults to <challenge-name>-solution.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Runtime profile hint to record in the generated README.
    #[arg(long, value_enum, default_value_t = SolutionRuntimeProfile::Python)]
    pub runtime_profile: SolutionRuntimeProfile,

    /// Solution interface hint to record in the generated README.
    #[arg(long, value_enum, default_value_t = SolutionInterface::ChallengeDefined)]
    pub interface: SolutionInterface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
/// Enumerates solution runtime profile variants supported by this module.
pub(crate) enum SolutionRuntimeProfile {
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
/// Enumerates solution interface variants supported by this module.
pub(crate) enum SolutionInterface {
    ChallengeDefined,
    Stdio,
    FileSystem,
}

#[derive(Debug, Clone, Args)]
/// Carries submit args data across this module boundary.
pub(crate) struct SubmitArgs {
    /// Published challenge name to submit against.
    pub challenge_name: ChallengeName,

    /// Target, for example linux-arm64-cpu.
    #[arg(long, value_name = "TARGET", conflicts_with = "all_targets")]
    pub target: Option<TargetName>,

    /// Submit once per target declared by the challenge.
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
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,

    /// Optional credit or provenance text.
    #[arg(long, default_value = "")]
    pub credit_text: String,
}

#[derive(Debug, Clone, Args)]
/// Carries validate args data across this module boundary.
pub(crate) struct ValidateArgs {
    /// Proposed challenge name to validate against for local bundle validation.
    #[arg(
        required_unless_present = "remote",
        conflicts_with = "remote_challenge_name"
    )]
    pub challenge_name: Option<ChallengeName>,

    /// Published challenge name to validate against when using --remote.
    #[arg(
        long = "challenge-name",
        required_if_eq("remote", "true"),
        conflicts_with = "challenge_name"
    )]
    pub remote_challenge_name: Option<ChallengeName>,

    /// Target, for example linux-arm64-cpu.
    #[arg(long, value_name = "TARGET", conflicts_with = "all_targets")]
    pub target: Option<TargetName>,

    /// Create one validation run per target declared by the challenge.
    #[arg(long)]
    pub all_targets: bool,

    /// Use the remote Agentics validation API instead of local Docker validation.
    #[arg(long)]
    pub remote: bool,

    /// Local challenge bundle directory containing spec.json and public validation assets.
    #[arg(
        long,
        value_name = "PATH",
        required_unless_present = "remote",
        conflicts_with = "remote"
    )]
    pub bundle_dir: Option<PathBuf>,

    /// Local runner storage directory for logs and intermediate artifacts.
    #[arg(long, value_name = "PATH", conflicts_with = "remote")]
    pub local_storage_dir: Option<PathBuf>,

    /// Workspace directory to package. Defaults to the current directory.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub dir: PathBuf,

    /// Explanation to attach to the validation run.
    #[arg(long, default_value = "")]
    pub explanation: String,

    /// Parent solution submission id when this validation run iterates on prior work.
    #[arg(long)]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,

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
/// Carries submissions args data across this module boundary.
pub(crate) struct SubmissionsArgs {
    #[command(subcommand)]
    pub command: SubmissionsCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates submissions command variants supported by this module.
pub(crate) enum SubmissionsCommand {
    /// List visible public solution submissions for a challenge and target.
    List {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: TargetName,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Show public solution submission details.
    Show { submission_id: SolutionSubmissionId },
    /// Show authenticated submitter lifecycle status.
    Status { submission_id: SolutionSubmissionId },
    /// Wait until a solution submission reaches a terminal state.
    Wait {
        submission_id: SolutionSubmissionId,
        #[arg(long, default_value_t = 2000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 300)]
        timeout_sec: u64,
    },
    /// Fetch runner logs for a solution submission.
    Logs { submission_id: SolutionSubmissionId },
    /// Show a detailed result report for a solution submission.
    Report { submission_id: SolutionSubmissionId },
    /// Show ranking context for a solution submission.
    Rank {
        submission_id: SolutionSubmissionId,
        #[arg(long)]
        challenge: ChallengeName,
        #[arg(long)]
        target: TargetName,
    },
}

#[derive(Debug, Clone, Args)]
/// Carries leaderboard args data across this module boundary.
pub(crate) struct LeaderboardArgs {
    #[command(subcommand)]
    pub command: LeaderboardCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates leaderboard command variants supported by this module.
pub(crate) enum LeaderboardCommand {
    /// Show a target-scoped leaderboard.
    Show {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: TargetName,
    },
}

#[derive(Debug, Clone, Args)]
/// Carries metrics args data across this module boundary.
pub(crate) struct MetricsArgs {
    #[command(subcommand)]
    pub command: MetricsCommand,
}

#[derive(Debug, Clone, Subcommand)]
/// Enumerates metrics command variants supported by this module.
pub(crate) enum MetricsCommand {
    /// Show a score distribution for one target and metric.
    Distribution {
        challenge_name: ChallengeName,
        #[arg(long)]
        target: TargetName,
        #[arg(long)]
        metric: MetricName,
    },
}
