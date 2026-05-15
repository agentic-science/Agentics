use anyhow::{Context, Result, bail};
use reqwest::{Client, Method, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::models::ErrorResponse;
use shared::models::challenge::{ChallengeDetailResponse, ChallengeListResponse};
use shared::models::challenge_creation::{
    ChallengeDraftCleanupResponse, ChallengeDraftResponse, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use shared::models::ids::SolutionSubmissionId;
use shared::models::names::{ChallengeName, MetricName, TargetName};
use shared::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreateSolutionSubmissionRequest,
    CreateSolutionSubmissionResponse, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse, LeaderboardResponse, RankingContextResponse,
    RegisterAgentRequest, RegisterAgentResponse, ScoreDistributionResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
};

#[derive(Debug)]
pub(crate) struct ApiStatusError {
    message: String,
}

impl ApiStatusError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for ApiStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiStatusError {}

#[derive(Debug, Clone)]
pub(crate) struct ApiClient {
    http: Client,
    base_url: Url,
    token: Option<String>,
}

impl ApiClient {
    pub(crate) fn new(api_base_url: &str, token: Option<String>) -> Result<Self> {
        Ok(Self {
            http: Client::new(),
            base_url: parse_base_url(api_base_url)?,
            token,
        })
    }

    pub(crate) async fn register(
        &self,
        request: &RegisterAgentRequest,
    ) -> Result<RegisterAgentResponse> {
        self.post_json("/api/agents/register", request, false).await
    }

    pub(crate) async fn list_challenges(&self) -> Result<ChallengeListResponse> {
        self.get_json("/api/public/challenges", false).await
    }

    pub(crate) async fn get_challenge(
        &self,
        challenge_name: &ChallengeName,
    ) -> Result<ChallengeDetailResponse> {
        let path = format!("/api/public/challenges/{challenge_name}");
        self.get_json(&path, false).await
    }

    pub(crate) async fn create_solution_submission(
        &self,
        request: &CreateSolutionSubmissionRequest,
    ) -> Result<CreateSolutionSubmissionResponse> {
        self.post_json("/api/solution-submissions", request, true)
            .await
    }

    pub(crate) async fn create_validation_run(
        &self,
        request: &CreateSolutionSubmissionRequest,
    ) -> Result<CreateSolutionSubmissionResponse> {
        self.post_json("/api/validation-runs", request, true).await
    }

    pub(crate) async fn get_solution_submission(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResponse> {
        let path = format!("/api/solution-submissions/{solution_submission_id}");
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_validation_run(
        &self,
        validation_run_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionResponse> {
        let path = format!("/api/validation-runs/{validation_run_id}");
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_solution_submission_logs(
        &self,
        solution_submission_id: &SolutionSubmissionId,
    ) -> Result<SolutionSubmissionLogsResponse> {
        let path = format!("/api/solution-submissions/{solution_submission_id}/logs");
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_solution_submission_ranking_context(
        &self,
        solution_submission_id: &SolutionSubmissionId,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<RankingContextResponse> {
        let path = format!(
            "/api/solution-submissions/{solution_submission_id}/ranking-context?challenge_name={challenge_name}&target={target}"
        );
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_leaderboard(
        &self,
        challenge_name: &ChallengeName,
        target: &TargetName,
    ) -> Result<LeaderboardResponse> {
        let path = format!("/api/public/challenges/{challenge_name}/leaderboard?target={target}");
        self.get_json(&path, false).await
    }

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

    pub(crate) async fn get_creator_challenge_stats(
        &self,
        challenge_name: &ChallengeName,
        target: Option<&TargetName>,
    ) -> Result<CreatorChallengeStatsResponse> {
        let path = creator_challenge_path(challenge_name, "stats", target);
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_creator_challenge_participants(
        &self,
        challenge_name: &ChallengeName,
        target: Option<&TargetName>,
    ) -> Result<CreatorChallengeParticipantsResponse> {
        let path = creator_challenge_path(challenge_name, "participants", target);
        self.get_json(&path, true).await
    }

    pub(crate) async fn get_challenge_shortlist(
        &self,
        challenge_name: &ChallengeName,
    ) -> Result<ChallengeShortlistResponse> {
        let path = format!("/api/creator/challenges/{challenge_name}/shortlist");
        self.get_json(&path, true).await
    }

    pub(crate) async fn create_challenge_shortlist_revision(
        &self,
        challenge_name: &ChallengeName,
        request: &CreateChallengeShortlistRevisionRequest,
    ) -> Result<ChallengeShortlistRevisionResponse> {
        let path = format!("/api/creator/challenges/{challenge_name}/shortlist-revisions");
        self.post_json(&path, request, true).await
    }

    pub(crate) async fn create_challenge_draft(
        &self,
        request: &CreateChallengeDraftRequest,
    ) -> Result<ChallengeDraftResponse> {
        self.post_json("/api/creator/challenge-drafts", request, true)
            .await
    }

    pub(crate) async fn get_challenge_draft(
        &self,
        draft_id: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/api/creator/challenge-drafts/{draft_id}");
        self.get_json(&path, true).await
    }

    pub(crate) async fn upload_challenge_private_asset(
        &self,
        draft_id: &str,
        request: &UploadChallengePrivateAssetRequest,
    ) -> Result<ChallengePrivateAssetResponse> {
        let path = format!("/api/creator/challenge-drafts/{draft_id}/private-assets");
        self.post_json(&path, request, true).await
    }

    pub(crate) async fn validate_challenge_draft_admin(
        &self,
        draft_id: &str,
        request: &ValidateChallengeDraftRequest,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/validate");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    pub(crate) async fn approve_challenge_draft_admin(
        &self,
        draft_id: &str,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/approve");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    pub(crate) async fn reject_challenge_draft_admin(
        &self,
        draft_id: &str,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/reject");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    pub(crate) async fn publish_challenge_draft_admin(
        &self,
        draft_id: &str,
        request: &ValidateChallengeDraftRequest,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/publish");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    pub(crate) async fn abandon_challenge_draft_admin(
        &self,
        draft_id: &str,
        request: &ReviewChallengeDraftRequest,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftResponse> {
        let path = format!("/admin/challenge-drafts/{draft_id}/abandon");
        self.post_json_admin(&path, request, username, password)
            .await
    }

    pub(crate) async fn cleanup_challenge_drafts_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<ChallengeDraftCleanupResponse> {
        self.post_json_admin(
            "/admin/challenge-drafts/cleanup",
            &serde_json::json!({}),
            username,
            password,
        )
        .await
    }

    async fn get_json<T>(&self, path: &str, authenticated: bool) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = self.request(Method::GET, path, authenticated)?;
        parse_response(request.send().await?).await
    }

    async fn post_json<B, T>(&self, path: &str, body: &B, authenticated: bool) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self.request(Method::POST, path, authenticated)?.json(body);
        parse_response(request.send().await?).await
    }

    async fn post_json_admin<B, T>(
        &self,
        path: &str,
        body: &B,
        username: &str,
        password: &str,
    ) -> Result<T>
    where
        B: Serialize + Sync + ?Sized,
        T: DeserializeOwned,
    {
        let request = self
            .request(Method::POST, path, false)?
            .basic_auth(username, Some(password))
            .json(body);
        parse_response(request.send().await?).await
    }

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
            Ok(request.bearer_auth(token))
        } else {
            Ok(request)
        }
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .with_context(|| format!("failed to build API endpoint for {path}"))
    }
}

fn parse_base_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value).with_context(|| format!("invalid API base URL `{value}`"))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => bail!("API base URL must use http or https, got `{scheme}`"),
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("API base URL must not include a query string or fragment");
    }
    if !url.path().ends_with('/') {
        let mut path = url.path().to_string();
        path.push('/');
        url.set_path(&path);
    }
    Ok(url)
}

fn creator_challenge_path(
    challenge_name: &ChallengeName,
    surface: &str,
    target: Option<&TargetName>,
) -> String {
    let mut path = format!("/api/creator/challenges/{challenge_name}/{surface}");
    if let Some(target) = target {
        path.push_str("?target=");
        path.push_str(target);
    }
    path
}

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
        return serde_json::from_str(&body)
            .with_context(|| format!("failed to decode successful API response as JSON: {body}"));
    }

    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
        return Err(ApiStatusError::new(format!(
            "Agentics API returned {} {}: {} ({})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("error"),
            error.message,
            error.error
        ))
        .into());
    }

    let message = if body.trim().is_empty() {
        "<empty response body>".to_string()
    } else {
        body
    };
    Err(ApiStatusError::new(format!(
        "Agentics API returned {} {}: {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("error"),
        message
    ))
    .into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use shared::models::request::RegisterAgentRequest;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::ApiClient;

    #[tokio::test]
    async fn register_sends_expected_payload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agents/register"))
            .and(body_json(json!({
                "name": "solver",
                "agent_description": "autonomous solver",
                "owner": "lab",
                "model_info": { "model": "gpt-test" }
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "agent_id": "agent-1",
                "token": "agentics_token",
                "name": "solver",
                "created_at": "2026-05-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(&server.uri(), None).expect("client should build");
        let response = client
            .register(&RegisterAgentRequest {
                name: "solver".to_string(),
                agent_description: "autonomous solver".to_string(),
                owner: "lab".to_string(),
                model_info: json!({ "model": "gpt-test" }),
            })
            .await
            .expect("register should succeed");

        assert_eq!(response.agent_id, "agent-1");
        assert_eq!(response.token, "agentics_token");
    }

    #[tokio::test]
    async fn api_errors_use_structured_error_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "bad_request",
                "message": "name must not be empty"
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(&server.uri(), None).expect("client should build");
        let error = client
            .list_challenges()
            .await
            .expect_err("request should fail");

        assert!(error.to_string().contains("bad_request"));
        assert!(error.to_string().contains("name must not be empty"));
    }
}
