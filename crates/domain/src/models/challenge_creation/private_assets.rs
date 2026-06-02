//! Private asset requirements and upload lifecycle DTOs.

use serde::{Deserialize, Serialize};

use crate::models::hashes::Sha256Digest;
use crate::models::ids::{ChallengePrivateAssetId, ChallengeReviewRecordId, HumanId};
use crate::models::names::AssetName;
use crate::models::paths::BundleRelativePath;
use crate::storage::StorageKey;

/// Private asset that must be uploaded directly to Agentics for a review record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengePrivateAssetRequirement {
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_paths: Vec<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_note: Option<String>,
}

/// Supported private asset classes for challenge creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePrivateAssetKind {
    PrivateBenchmarkData,
    PrivateEvaluatorPackage,
    PrivateSeeds,
    PrivateReferenceOutputs,
}

impl ChallengePrivateAssetKind {
    /// Stable database string for this private asset kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateBenchmarkData => "private_benchmark_data",
            Self::PrivateEvaluatorPackage => "private_evaluator_package",
            Self::PrivateSeeds => "private_seeds",
            Self::PrivateReferenceOutputs => "private_reference_outputs",
        }
    }

    /// Parse a stable database string for this private asset kind.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "private_benchmark_data" => Some(Self::PrivateBenchmarkData),
            "private_evaluator_package" => Some(Self::PrivateEvaluatorPackage),
            "private_seeds" => Some(Self::PrivateSeeds),
            "private_reference_outputs" => Some(Self::PrivateReferenceOutputs),
            _ => None,
        }
    }
}

/// API response for one private benchmark asset bound to a review record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengePrivateAssetResponse {
    pub id: ChallengePrivateAssetId,
    pub review_record_id: ChallengeReviewRecordId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub uploader_human_id: HumanId,
    pub created_at: String,
}

/// Internal lifecycle status for one private asset upload record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePrivateAssetStatus {
    Pending,
    Active,
    Failed,
    Purging,
}

impl ChallengePrivateAssetStatus {
    /// Stable database string for this private asset lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Purging => "purging",
        }
    }

    /// Parse a stable database string for this private asset lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "failed" => Some(Self::Failed),
            "purging" => Some(Self::Purging),
            _ => None,
        }
    }
}

/// Admin-only response for one private asset upload lifecycle record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengePrivateAssetResponse {
    pub id: ChallengePrivateAssetId,
    pub review_record_id: ChallengeReviewRecordId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub status: ChallengePrivateAssetStatus,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary_storage_key: Option<StorageKey>,
    pub uploader_human_id: HumanId,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
}

/// Admin-only list response for private asset upload lifecycle records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengePrivateAssetListResponse {
    pub items: Vec<AdminChallengePrivateAssetResponse>,
}
