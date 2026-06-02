//! Public GitHub challenge-creation and review-record lifecycle models.

mod lifecycle;
mod manifest;
mod private_assets;
mod requests;
mod responses;

pub use lifecycle::{ChallengeReviewRecordStatus, ChallengeReviewValidationStatus};
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
    ChallengeReviewDecisionRequest, CreateChallengeReviewRecordRequest,
    UploadChallengePrivateAssetRequest, ValidateChallengeReviewRecordRequest,
};
pub use responses::{
    ChallengeReviewRecordCleanupResponse, ChallengeReviewRecordListResponse,
    ChallengeReviewRecordResponse, ChallengeReviewValidationRecordResponse,
    CreatorChallengeReviewRecordResponse, CreatorChallengeReviewValidationRecordResponse,
};
