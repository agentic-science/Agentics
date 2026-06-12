//! Challenge bundle and challenge-facing DTOs.

mod bundle;
mod datasets;
mod execution;
mod lifecycle;
mod metrics;
mod published;
pub(crate) mod serde_helpers;
mod targets;

pub use bundle::{
    ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
    ChallengeResultDetailVisibility, ChallengeSolutionPublicationPolicy, ChallengeVisibility,
    ChallengeVisibilitySpec, MAX_CHALLENGE_KEYWORDS, MIN_CHALLENGE_KEYWORDS,
    PublicChallengeBundleSpec, SolutionSpec,
};
pub use datasets::{DatasetsSpec, PrivateBenchmarkPolicy, PublicDatasetsSpec};
pub use execution::{
    ChallengeExecutionMode, ChallengeExecutionSpec, ChallengeRunInputFile, ChallengeRunInterface,
    ChallengeRunManifest, ChallengeRunSpec, ChallengeSetupSpec, CoexecutedBenchmarkExecutionSpec,
    CoexecutedBenchmarkSetupSpec, EvaluatorSpec, PipedStdioExecutionSpec,
    PipedStdioSessionManifest, PipedStdioSetupSpec, PublicChallengeExecutionSpec,
    PublicCoexecutedBenchmarkExecutionSpec, PublicPipedStdioExecutionSpec,
    PublicSeparatedEvaluatorExecutionSpec, SeparatedEvaluatorExecutionSpec,
};
pub use lifecycle::ChallengeLifecycleStatus;
pub use metrics::{
    MetricDefinitionSpec, MetricDirection, MetricSchemaSpec, MetricVisibility, RankingSpec,
};
pub use published::{
    AdminChallengeListItemDto, AdminChallengeListResponse, ChallengeAdminResponse,
    ChallengeDetailResponse, ChallengeListItemDto, ChallengeListResponse, MoltbookCommunityDto,
    PublishChallengeResponse,
};
pub use targets::{
    ChallengeTargetSpec, DockerPlatform, EvaluatorStageProfiles, HardwareProfileSpec,
    ResourceProfileSpec, SolutionStageProfiles, StageResourceProfile, TargetAccelerator,
};
