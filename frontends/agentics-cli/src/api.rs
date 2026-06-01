use agentics_domain::models::ErrorResponse;
use agentics_domain::models::challenge::{ChallengeDetailResponse, ChallengeListResponse};
use agentics_domain::models::challenge_creation::{
    ChallengeDraftCleanupResponse, ChallengeDraftResponse, ReviewChallengeDraftRequest,
    ValidateChallengeDraftRequest,
};
use agentics_domain::models::ids::{ChallengeDraftId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{
    CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse, LeaderboardResponse,
    PublicSolutionSubmissionListResponse, RankingContextResponse, RegisterAgentRequest,
    RegisterAgentResponse, ScoreDistributionResponse, SolutionSubmissionLogsResponse,
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

    /// Validates challenge draft admin invariants for this contract.
    pub(crate) async fn validate_challenge_draft_admin(
        &self,
        draft_id: &ChallengeDraftId,
        request: &ValidateChallengeDraftRequest,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/validate");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    /// Handles approve challenge draft admin for this module.
    pub(crate) async fn approve_challenge_draft_admin(
        &self,
        draft_id: &ChallengeDraftId,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/approve");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    /// Handles reject challenge draft admin for this module.
    pub(crate) async fn reject_challenge_draft_admin(
        &self,
        draft_id: &ChallengeDraftId,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/reject");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    /// Handles publish challenge draft admin for this module.
    pub(crate) async fn publish_challenge_draft_admin(
        &self,
        draft_id: &ChallengeDraftId,
        request: &ValidateChallengeDraftRequest,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/publish");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    /// Handles abandon challenge draft admin for this module.
    pub(crate) async fn abandon_challenge_draft_admin(
        &self,
        draft_id: &ChallengeDraftId,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/abandon");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    /// Handles cleanup challenge drafts admin for this module.
    pub(crate) async fn cleanup_challenge_drafts_admin(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<ChallengeDraftCleanupResponse> {
        self.post_json_admin(
            "/admin/challenge-drafts/cleanup",
            &serde_json::json!({}),
            username,
            password,
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
        username: &str,
        password: &SecretString,
    ) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::POST, path, false)?
            .basic_auth(username, Some(password.expose_secret()))
            .header("X-Agentics-Admin-Automation", "true")
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
