use std::collections::BTreeMap;
use std::io;

use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value};
use shared::models::auth::{
    AdminSessionResponse, CreatorMeResponse, CreatorSessionResponse, GithubOauthLoginResponse,
};
use shared::models::challenge::{
    AdminChallengeListResponse, ChallengeAdminResponse, ChallengeDetailResponse,
    ChallengeListResponse, PublishChallengeResponse,
};
use shared::models::challenge_creation::{
    ChallengeDraftCleanupResponse, ChallengeDraftListResponse, ChallengeDraftResponse,
    ChallengePrivateAssetResponse, CreateChallengeDraftRequest, UploadChallengePrivateAssetRequest,
};
use shared::models::request::{
    AdminCapacityResponse, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse, DisableAgentResponse, EvaluationJobResponse,
    HideSolutionSubmissionResponse, LeaderboardResponse, PublicSolutionSubmissionListResponse,
    RankingContextResponse, ScoreDistributionResponse, SolutionSubmissionArtifactResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};

/// Handles main for this module.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut schemas = BTreeMap::new();

    insert_schema::<AdminCapacityResponse>(&mut schemas, "adminCapacityResponseSchema")?;
    insert_schema::<AdminChallengeListResponse>(&mut schemas, "adminChallengeListResponseSchema")?;
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
    insert_schema::<CreateChallengeShortlistRevisionRequest>(
        &mut schemas,
        "createChallengeShortlistRevisionRequestSchema",
    )?;
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
    insert_schema::<GithubOauthLoginResponse>(&mut schemas, "githubOauthLoginResponseSchema")?;
    insert_schema::<HideSolutionSubmissionResponse>(
        &mut schemas,
        "hideSolutionSubmissionResponseSchema",
    )?;
    insert_schema::<LeaderboardResponse>(&mut schemas, "leaderboardResponseSchema")?;
    insert_schema::<PublicSolutionSubmissionListResponse>(
        &mut schemas,
        "publicSolutionSubmissionListResponseSchema",
    )?;
    insert_schema::<RankingContextResponse>(&mut schemas, "rankingContextResponseSchema")?;
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

    serde_json::to_writer_pretty(io::stdout().lock(), &schemas)?;
    println!();
    Ok(())
}

/// Handles insert schema for this module.
fn insert_schema<T: JsonSchema>(
    schemas: &mut BTreeMap<String, Value>,
    export_name: &str,
) -> Result<(), serde_json::Error> {
    let mut schema = serde_json::to_value(schema_for!(T))?;
    normalize_response_schema(&mut schema);
    schemas.insert(export_name.to_string(), schema);
    Ok(())
}

/// Handles normalize response schema for this module.
fn normalize_response_schema(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_response_schema(item);
            }
        }
        Value::Object(object) => normalize_schema_object(object),
        _ => {}
    }
}

/// Handles normalize schema object for this module.
fn normalize_schema_object(object: &mut Map<String, Value>) {
    require_default_serialized_properties(object);
    object.remove("default");
    remove_null_type(object, "anyOf");
    remove_null_type(object, "oneOf");
    remove_null_from_type_array(object);
    collapse_string_const_union(object, "anyOf");
    collapse_string_const_union(object, "oneOf");

    for value in object.values_mut() {
        normalize_response_schema(value);
    }

    if matches!(object.get("type"), Some(Value::String(value)) if value == "object")
        && !object.contains_key("additionalProperties")
    {
        object.insert("additionalProperties".to_string(), Value::Bool(false));
    }
}

/// Requires default serialized properties and reports a domain error otherwise.
fn require_default_serialized_properties(object: &mut Map<String, Value>) {
    let Some(Value::Object(properties)) = object.get("properties") else {
        return;
    };

    let mut defaulted_properties = Vec::new();
    for (name, schema) in properties {
        let Value::Object(property_object) = schema else {
            continue;
        };
        if property_object.contains_key("default")
            && !matches!(property_object.get("default"), Some(Value::Null))
        {
            defaulted_properties.push(Value::String(name.clone()));
        }
    }

    if defaulted_properties.is_empty() {
        return;
    }

    let required = object
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(required) = required else {
        return;
    };
    for property in defaulted_properties {
        if !required.contains(&property) {
            required.push(property);
        }
    }
}

/// Handles remove null type for this module.
fn remove_null_type(object: &mut Map<String, Value>, key: &str) {
    let Some(Value::Array(items)) = object.get_mut(key) else {
        return;
    };

    items.retain(|item| !is_null_schema(item));
    if items.len() == 1 {
        let only = items.remove(0);
        object.remove(key);
        merge_schema_object(object, only);
    }
}

/// Handles remove null from type array for this module.
fn remove_null_from_type_array(object: &mut Map<String, Value>) {
    let Some(Value::Array(types)) = object.get_mut("type") else {
        return;
    };

    types.retain(|item| !matches!(item, Value::String(value) if value == "null"));
    if types.len() == 1 {
        let only = types.remove(0);
        object.insert("type".to_string(), only);
    }
}

/// Handles collapse string const union for this module.
fn collapse_string_const_union(object: &mut Map<String, Value>, key: &str) {
    let Some(Value::Array(items)) = object.get(key) else {
        return;
    };

    let mut variants = Vec::new();
    for item in items {
        let Value::Object(item_object) = item else {
            return;
        };
        let Some(Value::String(value)) = item_object.get("const") else {
            return;
        };
        variants.push(Value::String(value.clone()));
    }

    if variants.is_empty() {
        return;
    }

    object.remove(key);
    object
        .entry("type")
        .or_insert_with(|| Value::String("string".to_string()));
    object.insert("enum".to_string(), Value::Array(variants));
}

/// Handles merge schema object for this module.
fn merge_schema_object(target: &mut Map<String, Value>, source: Value) {
    match source {
        Value::Object(source_object) => {
            for (key, value) in source_object {
                target.entry(key).or_insert(value);
            }
        }
        other => {
            target.insert("const".to_string(), other);
        }
    }
}

/// Returns whether null schema holds.
fn is_null_schema(value: &Value) -> bool {
    matches!(
        value,
        Value::Object(object)
            if matches!(object.get("type"), Some(Value::String(schema_type)) if schema_type == "null")
    )
}
