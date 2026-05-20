//! Single Rust manifest for web-facing JSON schema exports.

use std::collections::BTreeMap;

use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value};

use crate::models::auth::{
    AdminLoginRequest, AdminSessionResponse, CreatorMeResponse, CreatorSessionResponse,
    GithubOauthCallbackQuery, GithubOauthLoginRequest, GithubOauthLoginResponse,
};
use crate::models::challenge::{
    AdminChallengeListResponse, ChallengeAdminResponse, ChallengeDetailResponse,
    ChallengeListResponse, PublishChallengeResponse,
};
use crate::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeDraftCleanupResponse,
    ChallengeDraftListResponse, ChallengeDraftResponse, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, CreatorChallengeDraftResponse, UploadChallengePrivateAssetRequest,
};
use crate::models::request::{
    AdminCapacityResponse, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreatePioneerCodeRequest,
    CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse, DisableAgentResponse,
    EvaluationJobResponse, HideSolutionSubmissionResponse, LeaderboardResponse,
    PioneerCodeDetailResponse, PioneerCodeListResponse, PublicSolutionSubmissionListResponse,
    PublicStatsResponse, RankingContextResponse, RegisterAgentRequest, RevokePioneerCodeResponse,
    ScoreDistributionResponse, SolutionSubmissionArtifactResponse, SolutionSubmissionLogsResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};

/// Export all Rust DTO schemas consumed by the web frontend.
pub fn export_web_schemas() -> Result<BTreeMap<String, Value>, serde_json::Error> {
    let mut schemas = BTreeMap::new();

    insert_schema::<AdminCapacityResponse>(&mut schemas, "adminCapacityResponseSchema")?;
    insert_schema::<AdminChallengeListResponse>(&mut schemas, "adminChallengeListResponseSchema")?;
    insert_schema::<AdminChallengePrivateAssetListResponse>(
        &mut schemas,
        "adminChallengePrivateAssetListResponseSchema",
    )?;
    insert_schema::<AdminLoginRequest>(&mut schemas, "adminLoginRequestSchema")?;
    insert_schema::<AdminServiceHeartbeatListResponse>(
        &mut schemas,
        "adminServiceHeartbeatListResponseSchema",
    )?;
    insert_schema::<AdminSessionResponse>(&mut schemas, "adminSessionResponseSchema")?;
    insert_schema::<AdminSolutionSubmissionListResponse>(
        &mut schemas,
        "adminSolutionSubmissionListResponseSchema",
    )?;
    insert_schema::<ChallengeAdminResponse>(&mut schemas, "challengeAdminResponseSchema")?;
    insert_schema::<ChallengeDetailResponse>(&mut schemas, "challengeDetailResponseSchema")?;
    insert_schema::<ChallengeDraftCleanupResponse>(
        &mut schemas,
        "challengeDraftCleanupResponseSchema",
    )?;
    insert_schema::<ChallengeDraftListResponse>(&mut schemas, "challengeDraftListResponseSchema")?;
    insert_schema::<ChallengeDraftResponse>(&mut schemas, "challengeDraftResponseSchema")?;
    insert_schema::<ChallengeListResponse>(&mut schemas, "challengeListResponseSchema")?;
    insert_schema::<ChallengePrivateAssetResponse>(
        &mut schemas,
        "challengePrivateAssetResponseSchema",
    )?;
    insert_schema::<ChallengeShortlistResponse>(&mut schemas, "challengeShortlistResponseSchema")?;
    insert_schema::<ChallengeShortlistRevisionResponse>(
        &mut schemas,
        "challengeShortlistRevisionResponseSchema",
    )?;
    insert_schema::<CreateChallengeDraftRequest>(
        &mut schemas,
        "createChallengeDraftRequestSchema",
    )?;
    insert_schema::<CreatorChallengeDraftResponse>(
        &mut schemas,
        "creatorChallengeDraftResponseSchema",
    )?;
    insert_schema::<CreateChallengeShortlistRevisionRequest>(
        &mut schemas,
        "createChallengeShortlistRevisionRequestSchema",
    )?;
    insert_schema::<CreatePioneerCodeRequest>(&mut schemas, "createPioneerCodeRequestSchema")?;
    insert_schema::<UploadChallengePrivateAssetRequest>(
        &mut schemas,
        "uploadChallengePrivateAssetRequestSchema",
    )?;
    insert_schema::<PublishChallengeResponse>(&mut schemas, "publishChallengeResponseSchema")?;
    insert_schema::<CreatorMeResponse>(&mut schemas, "creatorMeResponseSchema")?;
    insert_schema::<CreatorSessionResponse>(&mut schemas, "creatorSessionResponseSchema")?;
    insert_schema::<CreatorChallengeParticipantsResponse>(
        &mut schemas,
        "creatorChallengeParticipantsResponseSchema",
    )?;
    insert_schema::<CreatorChallengeStatsResponse>(
        &mut schemas,
        "creatorChallengeStatsResponseSchema",
    )?;
    insert_schema::<DisableAgentResponse>(&mut schemas, "disableAgentResponseSchema")?;
    insert_schema::<EvaluationJobResponse>(&mut schemas, "evaluationJobResponseSchema")?;
    insert_schema::<GithubOauthCallbackQuery>(&mut schemas, "githubOauthCallbackQuerySchema")?;
    insert_schema::<GithubOauthLoginRequest>(&mut schemas, "githubOauthLoginRequestSchema")?;
    insert_schema::<GithubOauthLoginResponse>(&mut schemas, "githubOauthLoginResponseSchema")?;
    insert_schema::<HideSolutionSubmissionResponse>(
        &mut schemas,
        "hideSolutionSubmissionResponseSchema",
    )?;
    insert_schema::<LeaderboardResponse>(&mut schemas, "leaderboardResponseSchema")?;
    insert_schema::<PioneerCodeDetailResponse>(&mut schemas, "pioneerCodeDetailResponseSchema")?;
    insert_schema::<PioneerCodeListResponse>(&mut schemas, "pioneerCodeListResponseSchema")?;
    insert_schema::<PublicSolutionSubmissionListResponse>(
        &mut schemas,
        "publicSolutionSubmissionListResponseSchema",
    )?;
    insert_schema::<PublicStatsResponse>(&mut schemas, "publicStatsResponseSchema")?;
    insert_schema::<RankingContextResponse>(&mut schemas, "rankingContextResponseSchema")?;
    insert_schema::<RegisterAgentRequest>(&mut schemas, "registerAgentRequestSchema")?;
    insert_schema::<RevokePioneerCodeResponse>(&mut schemas, "revokePioneerCodeResponseSchema")?;
    insert_schema::<ScoreDistributionResponse>(&mut schemas, "scoreDistributionResponseSchema")?;
    insert_schema::<SolutionSubmissionArtifactResponse>(
        &mut schemas,
        "solutionSubmissionArtifactResponseSchema",
    )?;
    insert_schema::<SolutionSubmissionLogsResponse>(
        &mut schemas,
        "solutionSubmissionLogsResponseSchema",
    )?;
    insert_schema::<SolutionSubmissionResultReportResponse>(
        &mut schemas,
        "solutionSubmissionResultReportResponseSchema",
    )?;
    insert_schema::<SolutionSubmissionResponse>(&mut schemas, "solutionSubmissionResponseSchema")?;

    Ok(schemas)
}

/// Insert one schema into the web export map.
fn insert_schema<T: JsonSchema>(
    schemas: &mut BTreeMap<String, Value>,
    export_name: &str,
) -> Result<(), serde_json::Error> {
    let mut schema = serde_json::to_value(schema_for!(T))?;
    normalize_response_schema(&mut schema);
    schemas.insert(export_name.to_string(), schema);
    Ok(())
}

/// Preserve optional-field omission semantics in generated Zod schemas.
fn normalize_response_schema(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_response_schema(item);
            }
        }
        Value::Object(map) => {
            if map.get("x-agentics-preserve-null").and_then(Value::as_bool) == Some(true) {
                map.remove("x-agentics-preserve-null");
                normalize_object_children(map);
                return;
            }

            let is_nullable = map
                .get("type")
                .and_then(Value::as_array)
                .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("null")));
            let has_any_of = map.contains_key("anyOf") || map.contains_key("oneOf");
            if is_nullable || has_any_of {
                remove_nullability(map);
            }
            normalize_object_children(map);
        }
        _ => {}
    }
}

/// Normalize all child schema values in an object.
fn normalize_object_children(map: &mut Map<String, Value>) {
    for value in map.values_mut() {
        normalize_response_schema(value);
    }
}

/// Remove JSON null branches so absent optionals stay `undefined` in web schemas.
fn remove_nullability(map: &mut Map<String, Value>) {
    if let Some(Value::Array(types)) = map.get_mut("type") {
        types.retain(|value| value.as_str() != Some("null"));
        if types.len() == 1
            && let Some(only) = types.pop()
        {
            map.insert("type".to_string(), only);
        }
    }
    for key in ["anyOf", "oneOf"] {
        let should_replace = map.get(key).is_some_and(|value| {
            value.as_array().is_some_and(|items| {
                items.len() == 2
                    && items
                        .iter()
                        .any(|item| item.get("type").and_then(Value::as_str) == Some("null"))
            })
        });
        if should_replace
            && let Some(Value::Array(mut items)) = map.remove(key)
            && let Some(non_null) = items
                .drain(..)
                .find(|item| item.get("type").and_then(Value::as_str) != Some("null"))
            && let Value::Object(non_null_map) = non_null
        {
            for (child_key, child_value) in non_null_map {
                map.entry(child_key).or_insert(child_value);
            }
        }
    }
}
