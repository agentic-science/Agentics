//! Public GitHub challenge-creation and draft lifecycle models.

mod lifecycle;
mod manifest;
mod private_assets;
mod requests;
mod responses;

pub use lifecycle::{ChallengeDraftStatus, ChallengeDraftValidationStatus};
pub use manifest::{
    AGENTICS_CHALLENGE_MANIFEST_FILE, ChallengeArchiveRequestSpec, ChallengeCreationCiSpec,
    ChallengeCreationManifest, ChallengeCreationRequestKind,
};
pub use private_assets::{
    AdminChallengePrivateAssetListResponse, AdminChallengePrivateAssetResponse,
    ChallengePrivateAssetKind, ChallengePrivateAssetRequirement, ChallengePrivateAssetResponse,
    ChallengePrivateAssetStatus,
};
pub use requests::{
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
pub use responses::{
    ChallengeDraftCleanupResponse, ChallengeDraftListResponse, ChallengeDraftResponse,
    ChallengeDraftValidationRecordResponse, CreatorChallengeDraftResponse,
    CreatorChallengeDraftValidationRecordResponse,
};
