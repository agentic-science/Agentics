//! Single Rust manifest for web-facing JSON schema exports.

use std::collections::BTreeMap;

use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value};

use crate::models::ErrorResponse;
use crate::models::auth::{
    AdminLoginRequest, AdminSessionResponse, CreatorMeResponse, CreatorSessionResponse,
    GithubOauthCallbackRequest, GithubOauthLoginRequest, GithubOauthLoginResponse,
};
use crate::models::challenge::{
    AdminChallengeListResponse, ChallengeAdminResponse, ChallengeDetailResponse,
    ChallengeListResponse, PublishChallengeResponse,
};
use crate::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeDraftCleanupResponse,
    ChallengeDraftListResponse, ChallengeDraftResponse, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, CreatorChallengeDraftResponse, ReviewChallengeDraftRequest,
    UploadChallengePrivateAssetRequest, ValidateChallengeDraftRequest,
};
use crate::models::request::{
    AdminCapacityResponse, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    ChallengeMoltbookDiscussionResponse, ChallengeShortlistResponse,
    ChallengeShortlistRevisionResponse, CreateChallengeShortlistRevisionRequest,
    CreatePioneerCodeRequest, CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse,
    DisableAgentResponse, EvaluationJobResponse, LeaderboardResponse, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PublicSolutionSubmissionListResponse, PublicStatsResponse,
    RankingContextResponse, RegisterAgentRequest, RevokePioneerCodeResponse,
    ScoreDistributionResponse, SetChallengeMoltbookDiscussionRequest,
    SolutionSubmissionArtifactResponse, SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};

struct SchemaExport {
    name: &'static str,
    build: fn() -> Result<Value, serde_json::Error>,
}

macro_rules! web_schema_exports {
    ($(($ty:ty, $name:literal $(,)?)),+ $(,)?) => {
        const WEB_SCHEMA_EXPORTS: &[SchemaExport] = &[
            $(
                SchemaExport {
                    name: $name,
                    build: schema_value::<$ty>,
                },
            )+
        ];
    };
}

web_schema_exports! {
    (AdminCapacityResponse, "adminCapacityResponseSchema"),
    (AdminChallengeListResponse, "adminChallengeListResponseSchema"),
    (
        AdminChallengePrivateAssetListResponse,
        "adminChallengePrivateAssetListResponseSchema",
    ),
    (AdminLoginRequest, "adminLoginRequestSchema"),
    (
        AdminServiceHeartbeatListResponse,
        "adminServiceHeartbeatListResponseSchema",
    ),
    (AdminSessionResponse, "adminSessionResponseSchema"),
    (
        AdminSolutionSubmissionListResponse,
        "adminSolutionSubmissionListResponseSchema",
    ),
    (ChallengeAdminResponse, "challengeAdminResponseSchema"),
    (ChallengeDetailResponse, "challengeDetailResponseSchema"),
    (
        ChallengeDraftCleanupResponse,
        "challengeDraftCleanupResponseSchema",
    ),
    (ChallengeDraftListResponse, "challengeDraftListResponseSchema"),
    (ChallengeDraftResponse, "challengeDraftResponseSchema"),
    (ChallengeListResponse, "challengeListResponseSchema"),
    (
        ChallengeMoltbookDiscussionResponse,
        "challengeMoltbookDiscussionResponseSchema",
    ),
    (
        ChallengePrivateAssetResponse,
        "challengePrivateAssetResponseSchema",
    ),
    (ChallengeShortlistResponse, "challengeShortlistResponseSchema"),
    (
        ChallengeShortlistRevisionResponse,
        "challengeShortlistRevisionResponseSchema",
    ),
    (
        CreateChallengeDraftRequest,
        "createChallengeDraftRequestSchema",
    ),
    (
        CreatorChallengeDraftResponse,
        "creatorChallengeDraftResponseSchema",
    ),
    (
        CreateChallengeShortlistRevisionRequest,
        "createChallengeShortlistRevisionRequestSchema",
    ),
    (CreatePioneerCodeRequest, "createPioneerCodeRequestSchema"),
    (
        ReviewChallengeDraftRequest,
        "reviewChallengeDraftRequestSchema",
    ),
    (
        UploadChallengePrivateAssetRequest,
        "uploadChallengePrivateAssetRequestSchema",
    ),
    (
        ValidateChallengeDraftRequest,
        "validateChallengeDraftRequestSchema",
    ),
    (PublishChallengeResponse, "publishChallengeResponseSchema"),
    (CreatorMeResponse, "creatorMeResponseSchema"),
    (CreatorSessionResponse, "creatorSessionResponseSchema"),
    (
        CreatorChallengeParticipantsResponse,
        "creatorChallengeParticipantsResponseSchema",
    ),
    (
        CreatorChallengeStatsResponse,
        "creatorChallengeStatsResponseSchema",
    ),
    (DisableAgentResponse, "disableAgentResponseSchema"),
    (EvaluationJobResponse, "evaluationJobResponseSchema"),
    (ErrorResponse, "errorResponseSchema"),
    (
        GithubOauthCallbackRequest,
        "githubOauthCallbackRequestSchema",
    ),
    (GithubOauthLoginRequest, "githubOauthLoginRequestSchema"),
    (GithubOauthLoginResponse, "githubOauthLoginResponseSchema"),
    (LeaderboardResponse, "leaderboardResponseSchema"),
    (PioneerCodeDetailResponse, "pioneerCodeDetailResponseSchema"),
    (PioneerCodeListResponse, "pioneerCodeListResponseSchema"),
    (
        PublicSolutionSubmissionListResponse,
        "publicSolutionSubmissionListResponseSchema",
    ),
    (PublicStatsResponse, "publicStatsResponseSchema"),
    (RankingContextResponse, "rankingContextResponseSchema"),
    (RegisterAgentRequest, "registerAgentRequestSchema"),
    (RevokePioneerCodeResponse, "revokePioneerCodeResponseSchema"),
    (ScoreDistributionResponse, "scoreDistributionResponseSchema"),
    (
        SetChallengeMoltbookDiscussionRequest,
        "setChallengeMoltbookDiscussionRequestSchema",
    ),
    (
        SolutionSubmissionArtifactResponse,
        "solutionSubmissionArtifactResponseSchema",
    ),
    (
        SolutionSubmissionLogsResponse,
        "solutionSubmissionLogsResponseSchema",
    ),
    (
        SolutionSubmissionResultReportResponse,
        "solutionSubmissionResultReportResponseSchema",
    ),
    (SolutionSubmissionResponse, "solutionSubmissionResponseSchema"),
}

/// Export all Rust DTO schemas consumed by the web frontend.
pub fn export_web_schemas() -> Result<BTreeMap<String, Value>, serde_json::Error> {
    let mut schemas = BTreeMap::new();
    for export in WEB_SCHEMA_EXPORTS {
        schemas.insert(export.name.to_string(), (export.build)()?);
    }

    Ok(schemas)
}

/// Build one normalized schema value.
fn schema_value<T: JsonSchema>() -> Result<Value, serde_json::Error> {
    let mut schema = serde_json::to_value(schema_for!(T))?;
    normalize_response_schema(&mut schema);
    Ok(schema)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn web_schema_manifest_exports_unique_named_contracts() {
        let schemas = export_web_schemas().expect("web schemas should export");
        let manifest_names = WEB_SCHEMA_EXPORTS
            .iter()
            .map(|export| export.name)
            .collect::<BTreeSet<_>>();
        let schema_names = schemas.keys().map(String::as_str).collect::<BTreeSet<_>>();

        assert_eq!(
            manifest_names.len(),
            WEB_SCHEMA_EXPORTS.len(),
            "schema export manifest must not contain duplicate names",
        );
        assert_eq!(
            schema_names, manifest_names,
            "generated schemas must match the manifest exactly",
        );
        for expected in [
            "adminCapacityResponseSchema",
            "challengeDetailResponseSchema",
            "creatorChallengeDraftResponseSchema",
            "solutionSubmissionResultReportResponseSchema",
        ] {
            assert!(
                schemas.contains_key(expected),
                "missing frontend schema contract {expected}",
            );
        }
    }

    #[test]
    fn web_schema_export_strips_internal_preserve_null_markers() {
        let schemas = export_web_schemas().expect("web schemas should export");

        for (name, schema) in schemas {
            assert_no_preserve_null_marker(&name, &schema);
        }
    }

    fn assert_no_preserve_null_marker(context: &str, value: &Value) {
        match value {
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    assert_no_preserve_null_marker(&format!("{context}[{index}]"), item);
                }
            }
            Value::Object(map) => {
                assert!(
                    !map.contains_key("x-agentics-preserve-null"),
                    "internal preserve-null marker leaked at {context}",
                );
                for (key, child) in map {
                    assert_no_preserve_null_marker(&format!("{context}.{key}"), child);
                }
            }
            _ => {}
        }
    }
}
