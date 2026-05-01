use anyhow::{Context, Result, bail};
use reqwest::{Client, Method, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::models::ErrorResponse;
use shared::models::problem::{ProblemDetailResponse, ProblemListResponse};
use shared::models::request::{
    CreateSubmissionRequest, CreateSubmissionResponse, RegisterAgentRequest, RegisterAgentResponse,
    SubmissionResponse,
};

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: Client,
    base_url: Url,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(api_base_url: &str, token: Option<String>) -> Result<Self> {
        Ok(Self {
            http: Client::new(),
            base_url: parse_base_url(api_base_url)?,
            token,
        })
    }

    pub async fn register(&self, request: &RegisterAgentRequest) -> Result<RegisterAgentResponse> {
        self.post_json("/api/agents/register", request, false).await
    }

    pub async fn list_problems(&self) -> Result<ProblemListResponse> {
        self.get_json("/api/public/problems", false).await
    }

    pub async fn get_problem(&self, problem_id: &str) -> Result<ProblemDetailResponse> {
        let path = format!("/api/public/problems/{problem_id}");
        self.get_json(&path, false).await
    }

    pub async fn create_submission(
        &self,
        request: &CreateSubmissionRequest,
    ) -> Result<CreateSubmissionResponse> {
        self.post_json("/api/submissions", request, true).await
    }

    pub async fn create_validation_run(
        &self,
        request: &CreateSubmissionRequest,
    ) -> Result<CreateSubmissionResponse> {
        self.post_json("/api/validation-runs", request, true).await
    }

    pub async fn get_submission(&self, submission_id: &str) -> Result<SubmissionResponse> {
        let path = format!("/api/submissions/{submission_id}");
        self.get_json(&path, true).await
    }

    pub async fn get_validation_run(&self, validation_run_id: &str) -> Result<SubmissionResponse> {
        let path = format!("/api/validation-runs/{validation_run_id}");
        self.get_json(&path, true).await
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
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let request = self.request(Method::POST, path, authenticated)?.json(body);
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
        bail!(
            "Agentics API returned {} {}: {} ({})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("error"),
            error.message,
            error.error
        );
    }

    let message = if body.trim().is_empty() {
        "<empty response body>".to_string()
    } else {
        body
    };
    bail!(
        "Agentics API returned {} {}: {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("error"),
        message
    );
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
                "description": "autonomous solver",
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
                description: "autonomous solver".to_string(),
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
            .and(path("/api/public/problems"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "bad_request",
                "message": "name must not be empty"
            })))
            .mount(&server)
            .await;

        let client = ApiClient::new(&server.uri(), None).expect("client should build");
        let error = client
            .list_problems()
            .await
            .expect_err("request should fail");

        assert!(error.to_string().contains("bad_request"));
        assert!(error.to_string().contains("name must not be empty"));
    }
}
