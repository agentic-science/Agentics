use agentics_domain::models::ErrorResponse;
use agentics_domain::models::challenge::{
    AdminChallengeListResponse, ChallengeDetailResponse, ChallengeListResponse,
};
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengePrivateAssetResponse,
    ChallengeReviewDecisionRequest, ChallengeReviewRecordCleanupResponse,
    ChallengeReviewRecordListResponse, ChallengeReviewRecordResponse,
    CreateChallengeReviewRecordRequest, CreatorChallengeReviewRecordResponse,
    UploadChallengePrivateAssetRequest, ValidateChallengeReviewRecordRequest,
};
use agentics_domain::models::ids::{
    AgentId, ChallengeReviewRecordId, PioneerCodeId, SolutionSubmissionId,
};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    ChallengeMoltbookDiscussionResponse, ChallengeShortlistResponse,
    ChallengeShortlistRevisionResponse, CreateChallengeShortlistRevisionRequest,
    CreatePioneerCodeRequest, CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
    CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse, DisableAgentResponse,
    EvaluationJobResponse, LeaderboardResponse, PioneerCodeDetailResponse, PioneerCodeListResponse,
    PublicSolutionSubmissionListResponse, RankingContextResponse, RegisterAgentRequest,
    RegisterAgentResponse, RevokePioneerCodeResponse, ScoreDistributionResponse,
    SetChallengeMoltbookDiscussionRequest, SolutionSubmissionLogsResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};
use anyhow::{Context, Result};
use reqwest::{Client, Method, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::ApiBaseUrl;

#[derive(Debug)]
/// Carries api status error data across this module boundary.
pub(crate) struct ApiStatusError {
    status: reqwest::StatusCode,
    message: String,
}

impl ApiStatusError {
    /// Handles new for this module.
    fn new(status: reqwest::StatusCode, message: String) -> Self {
        Self { status, message }
    }

    /// Returns whether the upstream API reported a missing resource.
    fn is_not_found(&self) -> bool {
        self.status == reqwest::StatusCode::NOT_FOUND
    }

    /// Returns whether the upstream API rejected access to this surface.
    fn is_forbidden(&self) -> bool {
        self.status == reqwest::StatusCode::FORBIDDEN
    }
}

impl std::fmt::Display for ApiStatusError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiStatusError {}

#[derive(Debug, Clone)]
/// Carries api client data across this module boundary.
pub(crate) struct ApiClient {
    http: Client,
    base_url: Url,
    token: Option<SecretString>,
}

impl ApiClient {
    /// Handles new for this module.
    pub(crate) fn new(api_base_url: &ApiBaseUrl, token: Option<SecretString>) -> Result<Self> {
        Ok(Self {
            http: Client::new(),
            base_url: api_base_url.as_url().clone(),
            token,
        })
    }

    /// Handles register for this module.
    pub(crate) async fn register(
        &self,
        request: &RegisterAgentRequest,
    ) -> Result<RegisterAgentResponse> {
        self.post_json("/api/agents/register", request, false).await
    }

    /// Lists challenges using the configured query scope.
    pub(crate) async fn list_challenges(&self) -> Result<ChallengeListResponse> {
        self.get_json("/api/public/challenges", false).await
    }

    /// Fetches challenge for the requested scope.
    pub(crate) async fn get_challenge(
        &self,
        challenge_name: &ChallengeName,
    ) -> Result<ChallengeDetailResponse> {
        let path = format!("/api/public/challenges/{challenge_name}");
        self.get_json(&path, false).await
    }

    /// Creates solution submission after validating caller inputs.
    pub(crate) async fn create_solution_submission(
        &self,
        request: &CreateSolutionSubmissionRequest,
    ) -> Result<CreateSolutionSubmissionResponse> {
        self.post_json("/api/agent/solution-submissions", request, true)
            .await
    }

    /// Creates validation run after validating caller inputs.
    pub(crate) async fn create_validation_run(
        &self,
        request: &CreateSolutionSubmissionRequest,
    ) -> Result<CreateSolutionSubmissionResponse> {
        self.post_json("/api/agent/validation-runs", request, true)
            .await
    }

    /// Fetches solution submission for the requested scope.
    pub(crate) async fn get_solution_submission(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResponse> {
        let path = format!("/api/agent/solution-submissions/{solution_submission_id}");
        self.get_json(&path, true).await
    }

    /// Fetches public visible solution submission details.
    pub(crate) async fn get_public_solution_submission(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResponse> {
        let path = format!("/api/public/solution-submissions/{solution_submission_id}");
        self.get_json(&path, false).await
    }

    /// Fetches public visible solution submissions for one challenge target.
    pub(crate) async fn list_public_solution_submissions(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
        limit: i64,
    ) -> Result<PublicSolutionSubmissionListResponse> {
        let path = format!(
            "/api/public/challenges/{challenge_name}/solution-submissions?target={target}&limit={limit}"
        );
        self.get_json(&path, false).await
    }

    /// Fetches owner-visible result report for the requested submission.
    pub(crate) async fn get_solution_submission_result_report(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResultReportResponse> {
        let path =
            format!("/api/agent/solution-submissions/{solution_submission_id}/result-report");
        self.get_json(&path, true).await
    }

    /// Fetches public redacted result report for the requested submission.
    pub(crate) async fn get_public_solution_submission_result_report(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResultReportResponse> {
        let path =
            format!("/api/public/solution-submissions/{solution_submission_id}/result-report");
        self.get_json(&path, false).await
    }

    /// Fetches validation run for the requested scope.
    pub(crate) async fn get_validation_run(
        &self,
        validation_run_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResponse> {
        let path = format!("/api/agent/validation-runs/{validation_run_id}");
        self.get_json(&path, true).await
    }

    /// Fetches solution submission logs for the requested scope.
    pub(crate) async fn get_solution_submission_logs(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionLogsResponse> {
        let path = format!("/api/agent/solution-submissions/{solution_submission_id}/logs");
        self.get_json(&path, true).await
    }

    /// Fetches solution submission ranking context for the requested scope.
    pub(crate) async fn get_solution_submission_ranking_context(
        &self,
        solution_submission_id: &SolutionSubmissionId,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<RankingContextResponse> {
        let path = format!(
            "/api/agent/solution-submissions/{solution_submission_id}/ranking-context?challenge_name={challenge_name}&target={target}"
        );
        self.get_json(&path, true).await
    }

    /// Fetches public ranking context for a visible solution submission.
    pub(crate) async fn get_public_solution_submission_ranking_context(
        &self,
        solution_submission_id: &SolutionSubmissionId,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<RankingContextResponse> {
        let path = format!(
            "/api/public/solution-submissions/{solution_submission_id}/ranking-context?challenge_name={challenge_name}&target={target}"
        );
        self.get_json(&path, false).await
    }

    /// Returns whether an API error represents a missing resource.
    pub(crate) fn is_not_found(error: &anyhow::Error) -> bool {
        error
            .downcast_ref::<ApiStatusError>()
            .is_some_and(ApiStatusError::is_not_found)
    }

    /// Returns whether an API error represents a visibility or authorization denial.
    pub(crate) fn is_forbidden(error: &anyhow::Error) -> bool {
        error
            .downcast_ref::<ApiStatusError>()
            .is_some_and(ApiStatusError::is_forbidden)
    }

    /// Fetches leaderboard for the requested scope.
    pub(crate) async fn get_leaderboard(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<LeaderboardResponse> {
        let path = format!("/api/public/challenges/{challenge_name}/leaderboard?target={target}");
        self.get_json(&path, false).await
    }

    /// Fetches score distribution for the requested scope.
    pub(crate) async fn get_score_distribution(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
        metric_name: &MetricName,
    ) -> Result<ScoreDistributionResponse> {
        let path = format!(
            "/api/public/challenges/{challenge_name}/score-distributions?target={target}&metric={metric_name}"
        );
        self.get_json(&path, false).await
    }

    /// Validates challenge review record admin invariants for this contract.
    pub(crate) async fn create_challenge_review_record_creator(
        &self,
        request: &CreateChallengeReviewRecordRequest,
        creator_api_token: &SecretString,
    ) -> Result<CreatorChallengeReviewRecordResponse> {
        self.post_json_creator(
            "/api/creator/challenge-review-records",
            request,
            creator_api_token,
        )
        .await
    }

    /// Fetches one challenge review record owned by the creator.
    pub(crate) async fn get_challenge_review_record_creator(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        creator_api_token: &SecretString,
    ) -> Result<CreatorChallengeReviewRecordResponse> {
        let path = format!("/api/creator/challenge-review-records/{review_record_id}");
        self.get_json_creator(&path, creator_api_token).await
    }

    /// Uploads one private asset for a creator-owned challenge review record.
    pub(crate) async fn upload_challenge_private_asset_creator(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &UploadChallengePrivateAssetRequest,
        creator_api_token: &SecretString,
    ) -> Result<ChallengePrivateAssetResponse> {
        let path =
            format!("/api/creator/challenge-review-records/{review_record_id}/private-assets");
        self.post_json_creator(&path, request, creator_api_token)
            .await
    }

    /// Fetches owner-visible challenge statistics.
    pub(crate) async fn get_creator_challenge_stats(
        &self,
        challenge_name: &ChallengeName,
        target: Option<&TargetName>,
        creator_api_token: &SecretString,
    ) -> Result<CreatorChallengeStatsResponse> {
        let path = match target {
            Some(target) => {
                format!("/api/creator/challenges/{challenge_name}/stats?target={target}")
            }
            None => format!("/api/creator/challenges/{challenge_name}/stats"),
        };
        self.get_json_creator(&path, creator_api_token).await
    }

    /// Fetches owner-visible challenge participants.
    pub(crate) async fn list_creator_challenge_participants(
        &self,
        challenge_name: &ChallengeName,
        target: Option<&TargetName>,
        creator_api_token: &SecretString,
    ) -> Result<CreatorChallengeParticipantsResponse> {
        let path = match target {
            Some(target) => {
                format!("/api/creator/challenges/{challenge_name}/participants?target={target}")
            }
            None => format!("/api/creator/challenges/{challenge_name}/participants"),
        };
        self.get_json_creator(&path, creator_api_token).await
    }

    /// Fetches the effective owner-managed shortlist union.
    pub(crate) async fn get_challenge_shortlist_creator(
        &self,
        challenge_name: &ChallengeName,
        creator_api_token: &SecretString,
    ) -> Result<ChallengeShortlistResponse> {
        let path = format!("/api/creator/challenges/{challenge_name}/shortlist");
        self.get_json_creator(&path, creator_api_token).await
    }

    /// Uploads a shortlist delta for one creator-owned challenge.
    pub(crate) async fn create_challenge_shortlist_revision_creator(
        &self,
        challenge_name: &ChallengeName,
        request: &CreateChallengeShortlistRevisionRequest,
        creator_api_token: &SecretString,
    ) -> Result<ChallengeShortlistRevisionResponse> {
        let path = format!("/api/creator/challenges/{challenge_name}/shortlist-revisions");
        self.post_json_creator(&path, request, creator_api_token)
            .await
    }

    /// Validates challenge review record admin invariants for this contract.
    pub(crate) async fn list_admin_challenges(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<AdminChallengeListResponse> {
        self.get_json_admin("/admin/challenges", admin_service_token)
            .await
    }

    /// Lists pioneer codes.
    pub(crate) async fn list_pioneer_codes_admin(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<PioneerCodeListResponse> {
        self.get_json_admin("/admin/pioneer-codes", admin_service_token)
            .await
    }

    /// Creates a pioneer code.
    pub(crate) async fn create_pioneer_code_admin(
        &self,
        request: &CreatePioneerCodeRequest,
        admin_service_token: &SecretString,
    ) -> Result<PioneerCodeDetailResponse> {
        self.post_json_admin("/admin/pioneer-codes", request, admin_service_token)
            .await
    }

    /// Fetches one pioneer code.
    pub(crate) async fn get_pioneer_code_admin(
        &self,
        id: &PioneerCodeId,
        admin_service_token: &SecretString,
    ) -> Result<PioneerCodeDetailResponse> {
        let path = format!("/admin/pioneer-codes/{id}");
        self.get_json_admin(&path, admin_service_token).await
    }

    /// Revokes one pioneer code.
    pub(crate) async fn revoke_pioneer_code_admin(
        &self,
        id: &PioneerCodeId,
        admin_service_token: &SecretString,
    ) -> Result<RevokePioneerCodeResponse> {
        let path = format!("/admin/pioneer-codes/{id}/revoke");
        self.post_json_admin(&path, &serde_json::json!({}), admin_service_token)
            .await
    }

    /// Sets a challenge Moltbook discussion anchor.
    pub(crate) async fn set_challenge_moltbook_discussion_admin(
        &self,
        challenge_name: &ChallengeName,
        request: &SetChallengeMoltbookDiscussionRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeMoltbookDiscussionResponse> {
        let path = format!("/admin/challenges/{challenge_name}/moltbook-discussion");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Clears a challenge Moltbook discussion anchor.
    pub(crate) async fn clear_challenge_moltbook_discussion_admin(
        &self,
        challenge_name: &ChallengeName,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeMoltbookDiscussionResponse> {
        let path = format!("/admin/challenges/{challenge_name}/moltbook-discussion");
        self.delete_json_admin(&path, admin_service_token).await
    }

    /// Lists admin solution submissions.
    pub(crate) async fn list_admin_solution_submissions(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<AdminSolutionSubmissionListResponse> {
        self.get_json_admin("/admin/solution-submissions", admin_service_token)
            .await
    }

    /// Lists service heartbeats.
    pub(crate) async fn list_admin_service_heartbeats(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<AdminServiceHeartbeatListResponse> {
        self.get_json_admin("/admin/service-heartbeats", admin_service_token)
            .await
    }

    /// Shows admin capacity.
    pub(crate) async fn get_admin_capacity(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<AdminCapacityResponse> {
        self.get_json_admin("/admin/capacity", admin_service_token)
            .await
    }

    /// Queues a rejudge job.
    pub(crate) async fn rejudge_admin(
        &self,
        submission_id: &SolutionSubmissionId,
        admin_service_token: &SecretString,
    ) -> Result<EvaluationJobResponse> {
        let path = format!("/admin/solution-submissions/{submission_id}/rejudge");
        self.post_json_admin(&path, &serde_json::json!({}), admin_service_token)
            .await
    }

    /// Queues an official run job.
    pub(crate) async fn official_run_admin(
        &self,
        submission_id: &SolutionSubmissionId,
        admin_service_token: &SecretString,
    ) -> Result<EvaluationJobResponse> {
        let path = format!("/admin/solution-submissions/{submission_id}/official-run");
        self.post_json_admin(&path, &serde_json::json!({}), admin_service_token)
            .await
    }

    /// Disables an agent.
    pub(crate) async fn disable_agent_admin(
        &self,
        agent_id: &AgentId,
        admin_service_token: &SecretString,
    ) -> Result<DisableAgentResponse> {
        let path = format!("/admin/agents/{agent_id}/disable");
        self.post_json_admin(&path, &serde_json::json!({}), admin_service_token)
            .await
    }

    /// Validates challenge review record admin invariants for this contract.
    pub(crate) async fn list_challenge_review_records_admin(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordListResponse> {
        self.get_json_admin("/admin/challenge-review-records", admin_service_token)
            .await
    }

    /// Lists private asset lifecycle rows for one challenge review record.
    pub(crate) async fn list_challenge_review_record_private_assets_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        admin_service_token: &SecretString,
    ) -> Result<AdminChallengePrivateAssetListResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/private-assets");
        self.get_json_admin(&path, admin_service_token).await
    }

    /// Validates challenge review record admin invariants for this contract.
    pub(crate) async fn validate_challenge_review_record_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &ValidateChallengeReviewRecordRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/validate");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Handles approve challenge review record admin for this module.
    pub(crate) async fn approve_challenge_review_record_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &ChallengeReviewDecisionRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/approve");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Handles reject challenge review record admin for this module.
    pub(crate) async fn reject_challenge_review_record_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &ChallengeReviewDecisionRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/reject");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Handles publish challenge review record admin for this module.
    pub(crate) async fn publish_challenge_review_record_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &ValidateChallengeReviewRecordRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/publish");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Handles abandon challenge review record admin for this module.
    pub(crate) async fn abandon_challenge_review_record_admin(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        request: &ChallengeReviewDecisionRequest,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordResponse> {
        let path = format!("/admin/challenge-review-records/{review_record_id}/abandon");
        self.post_json_admin(&path, request, admin_service_token)
            .await
    }

    /// Handles cleanup challenge review records admin for this module.
    pub(crate) async fn cleanup_challenge_review_records_admin(
        &self,
        admin_service_token: &SecretString,
    ) -> Result<ChallengeReviewRecordCleanupResponse> {
        self.post_json_admin(
            "/admin/challenge-review-records/cleanup",
            &serde_json::json!({}),
            admin_service_token,
        )
        .await
    }

    /// Fetches json for the requested scope.
    async fn get_json<T>(&self, path: &str, authenticated: bool) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self.request(Method::GET, path, authenticated)?;
        parse_response(request.send().await?).await
    }

    /// Handles post json for this module.
    async fn post_json<B, T>(&self, path: &str, body: &B, authenticated: bool) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self.request(Method::POST, path, authenticated)?.json(body);
        parse_response(request.send().await?).await
    }

    /// Handles post json admin for this module.
    async fn post_json_admin<B, T>(
        &self,
        path: &str,
        body: &B,
        admin_service_token: &SecretString,
    ) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::POST, path, false)?
            .bearer_auth(admin_service_token.expose_secret())
            .json(body);
        parse_response(request.send().await?).await
    }

    /// Handles get json admin for this module.
    async fn get_json_admin<T>(&self, path: &str, admin_service_token: &SecretString) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::GET, path, false)?
            .bearer_auth(admin_service_token.expose_secret());
        parse_response(request.send().await?).await
    }

    /// Handles delete json admin for this module.
    async fn delete_json_admin<T>(
        &self,
        path: &str,
        admin_service_token: &SecretString,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::DELETE, path, false)?
            .bearer_auth(admin_service_token.expose_secret());
        parse_response(request.send().await?).await
    }

    /// Handles get json for creator API-token authenticated routes.
    async fn get_json_creator<T>(&self, path: &str, creator_api_token: &SecretString) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::GET, path, false)?
            .bearer_auth(creator_api_token.expose_secret());
        parse_response(request.send().await?).await
    }

    /// Handles post json for creator API-token authenticated routes.
    async fn post_json_creator<B, T>(
        &self,
        path: &str,
        body: &B,
        creator_api_token: &SecretString,
    ) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::POST, path, false)?
            .bearer_auth(creator_api_token.expose_secret())
            .json(body);
        parse_response(request.send().await?).await
    }

    /// Handles request for this module.
    fn request(
        &self,
        method: Method,
        path: &str,
        authenticated: bool,
    ) -> Result<reqwest::RequestBuilder> {
        let url = self.endpoint(path)?;
        let request = self.http.request(method, url);
        if authenticated {
            let token = self
                .token
                .as_ref()
                .context("this command requires a configured bearer token")?;
            Ok(request.bearer_auth(token.expose_secret()))
        } else {
            Ok(request)
        }
    }

    /// Handles endpoint for this module.
    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .with_context(|| format!("failed to build API endpoint for {path}"))
    }
}

/// Parses response from an external boundary string.
async fn parse_response<T>(response: reqwest::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read API response body")?;

    if status.is_success() {
        return serde_json::from_str(&body).with_context(|| {
            format!(
                "failed to decode successful API response as JSON: status={} body_bytes={}",
                status.as_u16(),
                body.len()
            )
        });
    }

    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
        return Err(ApiStatusError::new(
            status,
            format!(
                "Agentics API returned {} {}: {} ({})",
                status.as_u16(),
                status.canonical_reason().unwrap_or("error"),
                error.error.message,
                error.error.code
            ),
        )
        .into());
    }

    let message = if body.trim().is_empty() {
        "<empty response body>".to_string()
    } else {
        body
    };
    Err(ApiStatusError::new(
        status,
        format!(
            "Agentics API returned {} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("error"),
            message
        ),
    )
    .into())
}

#[cfg(test)]
mod tests {
    use agentics_domain::models::pioneer_codes::PioneerCodeInput;
    use agentics_domain::models::request::RegisterAgentRequest;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::ApiBaseUrl;

    use super::ApiClient;

    /// Verifies that register sends expected payload.
    #[tokio::test]
    async fn register_sends_expected_payload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agents/register"))
            .and(body_json(json!({
                "display_name": "solver",
                "pioneer_code": "deadbeef",
                "agent_description": "autonomous solver",
                "model_info": { "model": "gpt-test" }
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "agent_id": "11111111-1111-4111-8111-111111111111",
                "token": "agentics_token",
                "display_name": "solver",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let api_base_url =
            ApiBaseUrl::try_new(&server.uri()).expect("mock server URL should parse");
        let client = ApiClient::new(&api_base_url, None).expect("client should build");
        let response = client
            .register(&RegisterAgentRequest {
                display_name: "solver".to_string(),
                pioneer_code: Some(
                    PioneerCodeInput::try_new("deadbeef").expect("test code should parse"),
                ),
                agent_description: "autonomous solver".to_string(),
                model_info: json!({ "model": "gpt-test" }),
            })
            .await
            .expect("register should succeed");

        assert_eq!(
            response.agent_id.to_string(),
            "11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(response.token, "agentics_token");
    }

    /// Verifies that api errors use structured error message.
    #[tokio::test]
    async fn api_errors_use_structured_error_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {
                    "code": "bad_request",
                    "message": "name must not be empty"
                }
            })))
            .mount(&server)
            .await;

        let api_base_url =
            ApiBaseUrl::try_new(&server.uri()).expect("mock server URL should parse");
        let client = ApiClient::new(&api_base_url, None).expect("client should build");
        let error = client
            .list_challenges()
            .await
            .expect_err("request should fail");

        assert!(error.to_string().contains("bad_request"));
        assert!(error.to_string().contains("name must not be empty"));
    }
}
