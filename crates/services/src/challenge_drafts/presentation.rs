use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetResponse, ChallengeDraftResponse,
    ChallengeDraftValidationRecordResponse, ChallengePrivateAssetResponse,
};
use agentics_persistence::{
    AdminChallengePrivateAssetRecord, ChallengeDraftRecord, ChallengeDraftValidationRecord,
    ChallengePrivateAssetRecord,
};

pub(super) fn draft_response(record: ChallengeDraftRecord) -> ChallengeDraftResponse {
    ChallengeDraftResponse {
        id: record.id,
        challenge_name: record.challenge_name,
        request: record.request,
        status: record.status,
        creator_agent_id: record.creator_agent_id,
        creator_github_user_id: record.creator_github_user_id,
        creator_github_login: record.creator_github_login,
        repo_url: record.repo_url,
        pr_number: record.pr_number,
        pr_url: record.pr_url,
        commit_sha: record.commit_sha,
        challenge_path: record.challenge_path,
        manifest_sha256: record.manifest_sha256,
        manifest: record.manifest,
        validation_bundle_sha256: record.validation_bundle_sha256,
        approved_bundle_sha256: record.approved_bundle_sha256,
        validation_message: record.validation_message,
        validation_repository_path: record.validation_repository_path,
        published_challenge_name: record.published_challenge_name,
        private_assets: record
            .private_assets
            .into_iter()
            .map(private_asset_response)
            .collect(),
        validation_records: record
            .validation_records
            .into_iter()
            .map(validation_record_response)
            .collect(),
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

pub(super) fn private_asset_response(
    record: ChallengePrivateAssetRecord,
) -> ChallengePrivateAssetResponse {
    ChallengePrivateAssetResponse {
        id: record.id,
        draft_id: record.draft_id,
        asset_name: record.asset_name,
        kind: record.kind,
        required: record.required,
        size_bytes: record.size_bytes,
        sha256: record.sha256,
        storage_key: record.storage_key,
        uploader_agent_id: record.uploader_agent_id,
        created_at: record.created_at.to_rfc3339(),
    }
}

pub(super) fn admin_private_asset_response(
    record: AdminChallengePrivateAssetRecord,
) -> AdminChallengePrivateAssetResponse {
    AdminChallengePrivateAssetResponse {
        id: record.id,
        draft_id: record.draft_id,
        asset_name: record.asset_name,
        kind: record.kind,
        required: record.required,
        status: record.status,
        size_bytes: record.size_bytes,
        sha256: record.sha256,
        storage_key: record.storage_key,
        temporary_storage_key: record.temporary_storage_key,
        uploader_agent_id: record.uploader_agent_id,
        created_at: record.created_at.to_rfc3339(),
        activated_at: record.activated_at.map(|value| value.to_rfc3339()),
        failed_at: record.failed_at.map(|value| value.to_rfc3339()),
        failure_message: record.failure_message,
    }
}

fn validation_record_response(
    record: ChallengeDraftValidationRecord,
) -> ChallengeDraftValidationRecordResponse {
    ChallengeDraftValidationRecordResponse {
        id: record.id,
        draft_id: record.draft_id,
        status: record.status,
        message: record.message,
        repository_path: record.repository_path,
        manifest_sha256: record.manifest_sha256,
        bundle_sha256: record.bundle_sha256,
        created_at: record.created_at.to_rfc3339(),
    }
}
