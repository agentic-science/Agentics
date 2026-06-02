//! Rust-native local MVP operational checker.
//!
//! This executable is the source of truth for local MVP checks. It checks Docker
//! daemon reachability, API health, public challenge catalog shape, optional
//! admin surfaces, and optional web frontend reachability.
//!
//! Native operations: HTTP requests use `reqwest`, JSON contracts use domain
//! DTOs, Docker reachability uses `bollard`, URLs use `url`, and secrets use
//! `secrecy`. It does not invoke `sh`, `curl`, `python3`, `docker`, or any other
//! command-line process.
//!
//! Cancellation: `run_from_process` races the configured checks against Ctrl-C.
//! Dropping the in-flight futures cancels pending HTTP/Docker work. The checker
//! creates no persistent state, so there is no rollback or dry-run behavior.
//! Re-running it is idempotent because it only observes service state.

use std::io::{self, Read};
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use agentics_config::{DEFAULT_API_HOST, DEFAULT_API_PORT, local_api_base_url};
use agentics_domain::models::HealthResponse;
use agentics_domain::models::challenge::ChallengeListResponse;
use agentics_domain::models::request::{AdminCapacityResponse, AdminServiceHeartbeatListResponse};
use bollard::Docker;
use clap::Parser;
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use url::Url;

const DEFAULT_TIMEOUT_SECONDS: u64 = 15;
type DockerProbeFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
type DockerProbe = Arc<dyn Fn() -> DockerProbeFuture + Send + Sync>;

/// Command-line arguments for `agentics-check-local-mvp`.
#[derive(Debug, Clone, Parser)]
#[command(
    about = "Checks the local Agentics MVP runtime surfaces.",
    long_about = "Checks Docker daemon reachability, API health, public challenge catalog shape, optional admin surfaces, and optional web frontend reachability.\n\nConfiguration is accepted through flags with AGENTICS_* environment fallbacks. Admin checks run only when an admin service token is supplied through AGENTICS_ADMIN_SERVICE_TOKEN or --admin-service-token-stdin. Web checks run only when a web base URL is supplied."
)]
pub struct Cli {
    /// API base URL. Falls back to AGENTICS_API_BASE_URL, then http://127.0.0.1:${AGENTICS_API_PORT:-3100}.
    #[arg(long)]
    api_base_url: Option<String>,
    /// API port used only when no API base URL is supplied. Falls back to AGENTICS_API_PORT, then 3100.
    #[arg(long)]
    api_port: Option<u16>,
    /// Web frontend base URL. Falls back to AGENTICS_WEB_BASE_URL. If absent, the web check is skipped.
    #[arg(long)]
    web_base_url: Option<String>,
    /// Read the admin service token from stdin instead of AGENTICS_ADMIN_SERVICE_TOKEN.
    #[arg(long)]
    admin_service_token_stdin: bool,
    /// Per-request timeout in seconds. Falls back to AGENTICS_CHECK_TIMEOUT_SECONDS, then 15.
    #[arg(long)]
    timeout_seconds: Option<u64>,
}

/// Environment snapshot used by config resolution.
#[derive(Debug, Clone, Default)]
pub struct CheckEnv {
    pub api_base_url: Option<String>,
    pub api_port: Option<u16>,
    pub web_base_url: Option<String>,
    pub admin_service_token: Option<SecretString>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawCheckEnv {
    api_base_url: Option<String>,
    api_port: Option<u16>,
    web_base_url: Option<String>,
    admin_service_token: Option<String>,
    check_timeout_seconds: Option<u64>,
}

impl CheckEnv {
    /// Read relevant environment variables from the current process.
    pub fn from_process() -> Result<Self, CheckError> {
        let raw = envy::prefixed("AGENTICS_")
            .from_env::<RawCheckEnv>()
            .map_err(|error| CheckError::InvalidConfig {
                field: "environment",
                message: error.to_string(),
            })?;
        Ok(Self {
            api_base_url: non_empty_owned(raw.api_base_url),
            api_port: raw.api_port,
            web_base_url: non_empty_owned(raw.web_base_url),
            admin_service_token: non_empty_owned(raw.admin_service_token).map(SecretString::from),
            timeout_seconds: raw.check_timeout_seconds,
        })
    }
}

/// Fully resolved checker configuration.
#[derive(Debug, Clone)]
pub struct CheckConfig {
    api_base_url: Url,
    web_base_url: Option<Url>,
    admin_service_token: Option<SecretString>,
    timeout: Duration,
}

/// One check result in deterministic display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    name: &'static str,
    status: CheckStatus,
}

impl CheckReport {
    /// Check name shown in output.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Check status.
    pub fn status(&self) -> &CheckStatus {
        &self.status
    }
}

/// Outcome for one local MVP check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Passed(String),
    Skipped(String),
    Failed(String),
}

impl CheckStatus {
    /// Whether this status is a failure.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Passed(_) => "PASS",
            Self::Skipped(_) => "SKIP",
            Self::Failed(_) => "FAIL",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::Passed(message) | Self::Skipped(message) | Self::Failed(message) => message,
        }
    }
}

/// Configuration or runtime failure before checks can be evaluated.
#[derive(Debug, Error)]
pub enum CheckError {
    #[error("invalid {field}: {message}")]
    InvalidConfig {
        field: &'static str,
        message: String,
    },
    #[error("failed to read admin service token from stdin: {0}")]
    Stdin(io::Error),
}

/// Run the checker from process arguments and environment.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    let stdin_service_token = match read_stdin_service_token(cli.admin_service_token_stdin) {
        Ok(service_token) => service_token,
        Err(error) => {
            eprintln!("[agentics-check] ERROR: {error}");
            return ExitCode::from(2);
        }
    };
    let env = match CheckEnv::from_process() {
        Ok(env) => env,
        Err(error) => {
            eprintln!("[agentics-check] ERROR: {error}");
            return ExitCode::from(2);
        }
    };
    let config = match resolve_config(&cli, &env, stdin_service_token) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[agentics-check] ERROR: {error}");
            return ExitCode::from(2);
        }
    };

    let run = run_checks(config, docker_daemon_probe());
    tokio::select! {
        reports = run => {
            print_reports(&reports);
            if reports.iter().any(|report| report.status.is_failed()) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        signal = tokio::signal::ctrl_c() => {
            match signal {
                Ok(()) => eprintln!("[agentics-check] interrupted by Ctrl-C"),
                Err(error) => eprintln!("[agentics-check] failed to listen for Ctrl-C: {error}"),
            }
            ExitCode::from(130)
        }
    }
}

/// Resolve command configuration from flags, environment, and optional stdin.
pub fn resolve_config(
    cli: &Cli,
    env: &CheckEnv,
    stdin_service_token: Option<SecretString>,
) -> Result<CheckConfig, CheckError> {
    let api_port = resolve_api_port(cli.api_port, env.api_port);
    let api_base_url =
        match first_non_empty(cli.api_base_url.as_deref(), env.api_base_url.as_deref()) {
            Some(value) => parse_base_url("API base URL", value)?,
            None => parse_base_url(
                "API base URL",
                &local_api_base_url(DEFAULT_API_HOST, api_port),
            )?,
        };

    let web_base_url = first_non_empty(cli.web_base_url.as_deref(), env.web_base_url.as_deref())
        .map(|value| parse_base_url("web base URL", value))
        .transpose()?;

    let admin_service_token = stdin_service_token.or_else(|| env.admin_service_token.clone());
    let timeout_seconds = resolve_timeout_seconds(cli.timeout_seconds, env.timeout_seconds)?;

    Ok(CheckConfig {
        api_base_url,
        web_base_url,
        admin_service_token,
        timeout: Duration::from_secs(timeout_seconds),
    })
}

/// Execute all configured checks concurrently and return ordered reports.
pub async fn run_checks(config: CheckConfig, docker_probe: DockerProbe) -> Vec<CheckReport> {
    let client = match Client::builder().timeout(config.timeout).build() {
        Ok(client) => client,
        Err(error) => {
            return vec![CheckReport {
                name: "HTTP client",
                status: CheckStatus::Failed(format!("failed to build HTTP client: {error}")),
            }];
        }
    };

    let docker = check_docker(docker_probe);
    let api_health = check_api_health(client.clone(), config.api_base_url.clone());
    let challenge_catalog = check_challenge_catalog(client.clone(), config.api_base_url.clone());
    let admin_capacity = check_optional_admin_capacity(
        client.clone(),
        config.api_base_url.clone(),
        config.admin_service_token.clone(),
    );
    let admin_heartbeats = check_optional_admin_heartbeats(
        client.clone(),
        config.api_base_url.clone(),
        config.admin_service_token,
    );
    let web = check_optional_web(client, config.web_base_url);

    let (docker, api_health, challenge_catalog, admin_capacity, admin_heartbeats, web) = tokio::join!(
        docker,
        api_health,
        challenge_catalog,
        admin_capacity,
        admin_heartbeats,
        web
    );
    vec![
        docker,
        api_health,
        challenge_catalog,
        admin_capacity,
        admin_heartbeats,
        web,
    ]
}

async fn check_optional_admin_capacity(
    client: Client,
    api_base_url: Url,
    admin_service_token: Option<SecretString>,
) -> CheckReport {
    match admin_service_token {
        Some(service_token) => check_admin_capacity(client, api_base_url, service_token).await,
        None => skipped(
            "admin capacity",
            "AGENTICS_ADMIN_SERVICE_TOKEN is unset and --admin-service-token-stdin was not used",
        ),
    }
}

async fn check_optional_admin_heartbeats(
    client: Client,
    api_base_url: Url,
    admin_service_token: Option<SecretString>,
) -> CheckReport {
    match admin_service_token {
        Some(service_token) => check_admin_heartbeats(client, api_base_url, service_token).await,
        None => skipped(
            "admin heartbeats",
            "AGENTICS_ADMIN_SERVICE_TOKEN is unset and --admin-service-token-stdin was not used",
        ),
    }
}

async fn check_optional_web(client: Client, web_base_url: Option<Url>) -> CheckReport {
    match web_base_url {
        Some(web_base_url) => check_web(client, web_base_url).await,
        None => skipped(
            "web frontend",
            "AGENTICS_WEB_BASE_URL and --web-base-url are unset",
        ),
    }
}

fn read_stdin_service_token(enabled: bool) -> Result<Option<SecretString>, CheckError> {
    if !enabled {
        return Ok(None);
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(CheckError::Stdin)?;
    let trimmed = input.trim_end_matches(['\r', '\n']).trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(SecretString::from(trimmed.to_string())))
}

fn non_empty_owned(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_non_empty<'a>(primary: Option<&'a str>, fallback: Option<&'a str>) -> Option<&'a str> {
    primary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| fallback.map(str::trim).filter(|value| !value.is_empty()))
}

fn resolve_api_port(flag: Option<u16>, env_value: Option<u16>) -> u16 {
    flag.or(env_value).unwrap_or(DEFAULT_API_PORT)
}

fn resolve_timeout_seconds(flag: Option<u64>, env_value: Option<u64>) -> Result<u64, CheckError> {
    let timeout = flag.or(env_value).unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    if timeout == 0 {
        return Err(CheckError::InvalidConfig {
            field: "timeout seconds",
            message: "must be greater than zero".to_string(),
        });
    }
    Ok(timeout)
}

fn parse_base_url(field: &'static str, value: &str) -> Result<Url, CheckError> {
    let trimmed = value.trim();
    let mut url = Url::parse(trimmed).map_err(|error| CheckError::InvalidConfig {
        field,
        message: format!("`{trimmed}` is not a valid URL: {error}"),
    })?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(CheckError::InvalidConfig {
                field,
                message: format!("must use http or https, got `{scheme}`"),
            });
        }
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(CheckError::InvalidConfig {
            field,
            message: "must not include a query string or fragment".to_string(),
        });
    }
    if !url.path().ends_with('/') {
        let mut path = url.path().to_string();
        path.push('/');
        url.set_path(&path);
    }
    Ok(url)
}

fn docker_daemon_probe() -> DockerProbe {
    Arc::new(|| {
        Box::pin(async {
            let docker = Docker::connect_with_defaults()
                .map_err(|error| format!("failed to connect to Docker daemon: {error}"))?;
            let info = docker
                .info()
                .await
                .map_err(|error| format!("failed to query Docker daemon info: {error}"))?;
            let name = info.name.unwrap_or_else(|| "unknown daemon".to_string());
            Ok(format!("Docker daemon reachable: {name}"))
        })
    })
}

async fn check_docker(docker_probe: DockerProbe) -> CheckReport {
    match docker_probe().await {
        Ok(message) => passed("Docker daemon", &message),
        Err(message) => failed("Docker daemon", message),
    }
}

async fn check_api_health(client: Client, base_url: Url) -> CheckReport {
    match get_json::<HealthResponse>(&client, &base_url, "healthz").await {
        Ok(payload) => {
            if payload.status != "ok" {
                return failed(
                    "API health",
                    format!("health status is not ok: {}", payload.status),
                );
            }
            if !payload.database.connected {
                return failed("API health", "database is not connected".to_string());
            }
            passed("API health", "status ok and database connected")
        }
        Err(message) => failed("API health", message),
    }
}

async fn check_challenge_catalog(client: Client, base_url: Url) -> CheckReport {
    match get_json::<ChallengeListResponse>(&client, &base_url, "api/public/challenges").await {
        Ok(payload) => passed(
            "public challenge catalog",
            &format!("public challenges: {}", payload.items.len()),
        ),
        Err(message) => failed("public challenge catalog", message),
    }
}

async fn check_admin_capacity(
    client: Client,
    base_url: Url,
    service_token: SecretString,
) -> CheckReport {
    match get_json_admin::<AdminCapacityResponse>(
        &client,
        &base_url,
        "admin/capacity",
        &service_token,
    )
    .await
    {
        Ok(payload) => passed(
            "admin capacity",
            &format!(
                "active agents: {}, validation jobs: {}, official jobs: {}",
                payload.usage.active_agents,
                payload.usage.active_validation_jobs,
                payload.usage.active_official_jobs
            ),
        ),
        Err(message) => failed("admin capacity", message),
    }
}

async fn check_admin_heartbeats(
    client: Client,
    base_url: Url,
    service_token: SecretString,
) -> CheckReport {
    match get_json_admin::<AdminServiceHeartbeatListResponse>(
        &client,
        &base_url,
        "admin/service-heartbeats",
        &service_token,
    )
    .await
    {
        Ok(payload) => passed(
            "admin heartbeats",
            &format!("service heartbeats: {}", payload.items.len()),
        ),
        Err(message) => failed("admin heartbeats", message),
    }
}

async fn check_web(client: Client, web_base_url: Url) -> CheckReport {
    match client.get(web_base_url.clone()).send().await {
        Ok(response) if response.status().is_success() => {
            passed("web frontend", &format!("reachable at {web_base_url}"))
        }
        Ok(response) => failed(
            "web frontend",
            format_http_status("web frontend", response.status()),
        ),
        Err(error) => failed("web frontend", format!("request failed: {error}")),
    }
}

async fn get_json<T>(client: &Client, base_url: &Url, path: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let url = endpoint(base_url, path)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    parse_json_response(path, response).await
}

async fn get_json_admin<T>(
    client: &Client,
    base_url: &Url,
    path: &str,
    service_token: &SecretString,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let url = endpoint(base_url, path)?;
    let response = client
        .get(url)
        .bearer_auth(service_token.expose_secret())
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    parse_json_response(path, response).await
}

async fn parse_json_response<T>(path: &str, response: reqwest::Response) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        return Err(format_http_status(path, status));
    }
    response.json::<T>().await.map_err(|error| {
        format!("failed to decode successful response from {path} as JSON: {error}")
    })
}

fn endpoint(base_url: &Url, path: &str) -> Result<Url, String> {
    base_url
        .join(path.trim_start_matches('/'))
        .map_err(|error| format!("failed to build endpoint for {path}: {error}"))
}

fn format_http_status(label: &str, status: StatusCode) -> String {
    format!(
        "{label} returned HTTP {} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("error")
    )
}

fn passed(name: &'static str, message: &str) -> CheckReport {
    CheckReport {
        name,
        status: CheckStatus::Passed(message.to_string()),
    }
}

fn skipped(name: &'static str, message: &str) -> CheckReport {
    CheckReport {
        name,
        status: CheckStatus::Skipped(message.to_string()),
    }
}

fn failed(name: &'static str, message: String) -> CheckReport {
    CheckReport {
        name,
        status: CheckStatus::Failed(message),
    }
}

fn print_reports(reports: &[CheckReport]) {
    for report in reports {
        let line = format!(
            "[agentics-check] {} {}: {}",
            report.status.label(),
            report.name,
            report.status.message()
        );
        if report.status.is_failed() {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn cli() -> Cli {
        Cli {
            api_base_url: None,
            api_port: None,
            web_base_url: None,
            admin_service_token_stdin: false,
            timeout_seconds: None,
        }
    }

    fn passing_docker_probe() -> DockerProbe {
        Arc::new(|| Box::pin(async { Ok("Docker daemon reachable: test".to_string()) }))
    }

    fn failing_docker_probe() -> DockerProbe {
        Arc::new(|| Box::pin(async { Err("docker unavailable".to_string()) }))
    }

    fn status_at(reports: &[CheckReport], index: usize) -> &CheckStatus {
        reports.get(index).expect("report index").status()
    }

    #[test]
    fn resolves_default_config() {
        let config = resolve_config(&cli(), &CheckEnv::default(), None)
            .expect("default config should resolve");

        assert_eq!(config.api_base_url.as_str(), "http://127.0.0.1:3100/");
        assert!(config.web_base_url.is_none());
        assert!(config.admin_service_token.is_none());
        assert_eq!(config.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECONDS));
    }

    #[test]
    fn resolves_flag_and_env_precedence() {
        let mut args = cli();
        args.api_base_url = Some("http://flag.example".to_string());
        args.web_base_url = Some("http://web.example".to_string());
        args.timeout_seconds = Some(9);
        let env = CheckEnv {
            api_base_url: Some("http://env.example".to_string()),
            api_port: Some(9999),
            web_base_url: Some("http://env-web.example".to_string()),
            admin_service_token: Some(SecretString::from("env-secret")),
            timeout_seconds: Some(30),
        };

        let config = resolve_config(&args, &env, Some(SecretString::from("stdin-secret")))
            .expect("config should resolve");

        assert_eq!(config.api_base_url.as_str(), "http://flag.example/");
        assert_eq!(
            config.web_base_url.as_ref().map(Url::as_str),
            Some("http://web.example/")
        );
        assert_eq!(
            config
                .admin_service_token
                .as_ref()
                .map(ExposeSecret::expose_secret),
            Some("stdin-secret")
        );
        assert_eq!(config.timeout, Duration::from_secs(9));
    }

    #[test]
    fn resolves_api_port_fallback() {
        let env = CheckEnv {
            api_port: Some(4123),
            ..CheckEnv::default()
        };

        let config = resolve_config(&cli(), &env, None).expect("config should resolve");

        assert_eq!(config.api_base_url.as_str(), "http://127.0.0.1:4123/");
    }

    #[test]
    fn raw_env_uses_typed_numeric_deserialization() {
        let raw = envy::prefixed("AGENTICS_")
            .from_iter::<_, RawCheckEnv>([
                ("AGENTICS_API_PORT".to_string(), "4123".to_string()),
                (
                    "AGENTICS_CHECK_TIMEOUT_SECONDS".to_string(),
                    "30".to_string(),
                ),
            ])
            .expect("numeric env values should deserialize");
        assert_eq!(raw.api_port, Some(4123));
        assert_eq!(raw.check_timeout_seconds, Some(30));

        let error = envy::prefixed("AGENTICS_")
            .from_iter::<_, RawCheckEnv>([(
                "AGENTICS_CHECK_TIMEOUT_SECONDS".to_string(),
                "bad".to_string(),
            )])
            .expect_err("invalid timeout should fail during raw env parsing");
        assert!(error.to_string().contains("CHECK_TIMEOUT_SECONDS"));
    }

    #[test]
    fn rejects_invalid_urls_and_timeout() {
        let mut args = cli();
        args.api_base_url = Some("file:///tmp/api".to_string());
        assert!(resolve_config(&args, &CheckEnv::default(), None).is_err());

        let mut args = cli();
        args.timeout_seconds = Some(0);
        assert!(resolve_config(&args, &CheckEnv::default(), None).is_err());
    }

    #[tokio::test]
    async fn successful_run_reports_all_configured_checks() {
        let server = MockServer::start().await;
        mount_required_api_mocks(&server).await;
        mount_admin_mocks(&server, "agentics_admin_secret").await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let config = CheckConfig {
            api_base_url: Url::parse(&server.uri()).expect("server URL"),
            web_base_url: Some(Url::parse(&server.uri()).expect("server URL")),
            admin_service_token: Some(SecretString::from("agentics_admin_secret")),
            timeout: Duration::from_secs(5),
        };

        let reports = run_checks(config, passing_docker_probe()).await;

        assert_eq!(reports.len(), 6);
        assert!(reports.iter().all(|report| !report.status().is_failed()));
        assert!(matches!(status_at(&reports, 3), CheckStatus::Passed(_)));
        assert!(matches!(status_at(&reports, 4), CheckStatus::Passed(_)));
        assert!(matches!(status_at(&reports, 5), CheckStatus::Passed(_)));
    }

    #[tokio::test]
    async fn skips_admin_and_web_when_not_configured() {
        let server = MockServer::start().await;
        mount_required_api_mocks(&server).await;
        let config = CheckConfig {
            api_base_url: Url::parse(&server.uri()).expect("server URL"),
            web_base_url: None,
            admin_service_token: None,
            timeout: Duration::from_secs(5),
        };

        let reports = run_checks(config, passing_docker_probe()).await;

        assert!(matches!(status_at(&reports, 3), CheckStatus::Skipped(_)));
        assert!(matches!(status_at(&reports, 4), CheckStatus::Skipped(_)));
        assert!(matches!(status_at(&reports, 5), CheckStatus::Skipped(_)));
    }

    #[tokio::test]
    async fn health_failure_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "error",
                "service": "agentics-api",
                "environment": "test",
                "database": {
                    "connected": true,
                    "current_time": "now"
                }
            })))
            .mount(&server)
            .await;
        mount_catalog_mock(&server).await;

        let reports = run_checks(required_only_config(&server), passing_docker_probe()).await;

        assert!(
            matches!(status_at(&reports, 1), CheckStatus::Failed(message) if message.contains("not ok"))
        );
    }

    #[tokio::test]
    async fn catalog_shape_failure_is_reported() {
        let server = MockServer::start().await;
        mount_health_mock(&server).await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": {} })))
            .mount(&server)
            .await;

        let reports = run_checks(required_only_config(&server), passing_docker_probe()).await;

        assert!(
            matches!(status_at(&reports, 2), CheckStatus::Failed(message) if message.contains("decode"))
        );
    }

    #[tokio::test]
    async fn admin_failure_is_reported_when_service_token_configured() {
        let server = MockServer::start().await;
        mount_required_api_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path("/admin/capacity"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/admin/service-heartbeats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
            .mount(&server)
            .await;
        let config = CheckConfig {
            api_base_url: Url::parse(&server.uri()).expect("server URL"),
            web_base_url: None,
            admin_service_token: Some(SecretString::from("agentics_admin_secret")),
            timeout: Duration::from_secs(5),
        };

        let reports = run_checks(config, passing_docker_probe()).await;

        assert!(
            matches!(status_at(&reports, 3), CheckStatus::Failed(message) if message.contains("500"))
        );
        assert!(matches!(status_at(&reports, 4), CheckStatus::Passed(_)));
    }

    #[tokio::test]
    async fn web_failure_is_reported_when_configured() {
        let server = MockServer::start().await;
        mount_required_api_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path("/web"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let mut config = required_only_config(&server);
        config.web_base_url = Some(Url::parse(&format!("{}/web", server.uri())).expect("web URL"));

        let reports = run_checks(config, passing_docker_probe()).await;

        assert!(
            matches!(status_at(&reports, 5), CheckStatus::Failed(message) if message.contains("404"))
        );
    }

    #[tokio::test]
    async fn multiple_failures_are_collected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/healthz"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let reports = run_checks(required_only_config(&server), failing_docker_probe()).await;

        assert_eq!(
            reports
                .iter()
                .filter(|report| report.status().is_failed())
                .count(),
            3
        );
    }

    #[tokio::test]
    async fn checks_run_concurrently() {
        let server = MockServer::start().await;
        mount_required_api_mocks(&server).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let docker_calls = Arc::clone(&calls);
        let docker_probe: DockerProbe = Arc::new(move || {
            let docker_calls = Arc::clone(&docker_calls);
            Box::pin(async move {
                docker_calls.fetch_add(1, Ordering::SeqCst);
                Ok("Docker daemon reachable: test".to_string())
            })
        });

        let reports = run_checks(required_only_config(&server), docker_probe).await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(reports.len(), 6);
    }

    fn required_only_config(server: &MockServer) -> CheckConfig {
        CheckConfig {
            api_base_url: Url::parse(&server.uri()).expect("server URL"),
            web_base_url: None,
            admin_service_token: None,
            timeout: Duration::from_secs(5),
        }
    }

    async fn mount_required_api_mocks(server: &MockServer) {
        mount_health_mock(server).await;
        mount_catalog_mock(server).await;
    }

    async fn mount_health_mock(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/healthz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "service": "agentics-api",
                "environment": "test",
                "database": {
                    "connected": true,
                    "current_time": "now"
                }
            })))
            .mount(server)
            .await;
    }

    async fn mount_catalog_mock(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/api/public/challenges"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [],
                "total_count": 0,
                "limit": 50,
                "offset": 0,
                "has_more": false
            })))
            .mount(server)
            .await;
    }

    async fn mount_admin_mocks(server: &MockServer, service_token: &str) {
        Mock::given(method("GET"))
            .and(path("/admin/capacity"))
            .and(header("authorization", format!("Bearer {service_token}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "quota_window_seconds": 86400,
                "quotas": {
                    "validation_runs_per_agent_challenge_day": 20,
                    "official_runs_per_agent_challenge_day": 5,
                    "max_active_official_jobs": 20,
                    "max_active_agents": 1000
                },
                "usage": {
                    "active_agents": 1,
                    "active_validation_jobs": 2,
                    "active_official_jobs": 3
                }
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path("/admin/service-heartbeats"))
            .and(header("authorization", format!("Bearer {service_token}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "service_name": "worker",
                        "last_seen_at": "now",
                        "payload": {}
                    }
                ]
            })))
            .mount(server)
            .await;
    }
}
