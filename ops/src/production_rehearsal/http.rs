//! Small HTTP helpers for the production rehearsal harness.

use reqwest::{Client, Url};
use secrecy::{ExposeSecret, SecretString};

use super::ProductionRehearsalError;

pub(super) async fn get_json(
    client: &Client,
    api_base_url: &Url,
    path: &str,
) -> Result<serde_json::Value, ProductionRehearsalError> {
    let response = client
        .get(join_url(api_base_url, path)?)
        .send()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?;
    response_to_json(response).await
}

pub(super) async fn admin_get_json(
    client: &Client,
    api_base_url: &Url,
    path: &str,
    service_token: &SecretString,
) -> Result<serde_json::Value, ProductionRehearsalError> {
    let response = client
        .get(join_url(api_base_url, path)?)
        .bearer_auth(service_token.expose_secret())
        .send()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?;
    response_to_json(response).await
}

pub(super) async fn admin_post_json(
    client: &Client,
    api_base_url: &Url,
    path: &str,
    service_token: &SecretString,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ProductionRehearsalError> {
    let response = client
        .post(join_url(api_base_url, path)?)
        .bearer_auth(service_token.expose_secret())
        .json(body)
        .send()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?;
    response_to_json(response).await
}

pub(super) async fn response_to_json(
    response: reqwest::Response,
) -> Result<serde_json::Value, ProductionRehearsalError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(ProductionRehearsalError::HttpClient)?;
    if !status.is_success() {
        return Err(ProductionRehearsalError::HttpStatus { status, body });
    }
    serde_json::from_str(&body).map_err(ProductionRehearsalError::Json)
}

pub(super) fn join_url(base: &Url, path: &str) -> Result<Url, ProductionRehearsalError> {
    let mut base = base.clone();
    let path = path.trim_start_matches('/');
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path().trim_end_matches('/')));
    }
    base.join(path).map_err(|error| {
        ProductionRehearsalError::InvalidResponse(format!("invalid API path `{path}`: {error}"))
    })
}
