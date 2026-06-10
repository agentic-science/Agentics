//! crates.io-aware workspace publish orchestration.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use reqwest::header::{RETRY_AFTER, USER_AGENT};
use serde::Deserialize;
use tokio::time::{Instant, sleep};
use url::Url;

const DEFAULT_CRATES_IO_API_BASE: &str = "https://crates.io/api/v1";
const DEFAULT_REQUEST_DELAY_MS: u64 = 1100;
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const DEFAULT_POLL_TIMEOUT_SECS: u64 = 300;
const MAX_API_RETRIES: usize = 5;

const PUBLISH_ALLOWLIST: &[&str] = &[
    "agentics-error",
    "agentics-domain",
    "agentics-contracts",
    "agentics-storage",
    "agentics-config",
    "agentics-persistence",
    "agentics-services",
    "agentics-runner",
    "agentics",
    "agentics-api-server",
    "agentics-worker",
];

const NON_PUBLISHABLE_PACKAGES: &[&str] = &[
    "agentics-ops",
    "agentics-pre-commit",
    "agentics-dev-checks",
    "integration-tests",
];

#[derive(Debug, Parser)]
#[command(about = "Publish Agentics workspace packages with crates.io API checks")]
pub struct PublishArgs {
    /// Release version to publish.
    #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
    version: String,

    /// Check availability and run cargo publish --dry-run.
    #[arg(long, conflicts_with = "execute")]
    dry_run: bool,

    /// Publish missing allowlisted packages and poll crates.io visibility.
    #[arg(long)]
    execute: bool,

    /// crates.io API base URL. Tests may point this at a local mock server.
    #[arg(long, default_value = DEFAULT_CRATES_IO_API_BASE)]
    crates_io_api_base: String,

    /// User-Agent sent to crates.io.
    #[arg(long)]
    user_agent: Option<String>,

    /// Delay between crates.io API requests.
    #[arg(long, default_value_t = DEFAULT_REQUEST_DELAY_MS)]
    request_delay_ms: u64,

    /// Poll interval after publishing with --execute.
    #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_SECS)]
    poll_interval_secs: u64,

    /// Maximum time to wait for crates.io visibility after --execute.
    #[arg(long, default_value_t = DEFAULT_POLL_TIMEOUT_SECS)]
    poll_timeout_secs: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    id: String,
    publish: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishMode {
    DryRun,
    Execute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CrateAvailability {
    Present,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublishPlan {
    version: String,
    missing_allowlisted_packages: Vec<String>,
    already_published_packages: Vec<String>,
    cargo_excludes: Vec<String>,
}

/// Runs the publish command from process arguments.
pub async fn run_from_process() -> std::process::ExitCode {
    match run(PublishArgs::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(args: PublishArgs) -> Result<()> {
    let mode = if args.execute {
        PublishMode::Execute
    } else {
        PublishMode::DryRun
    };
    if matches!(mode, PublishMode::Execute) && std::env::var_os("CARGO_REGISTRY_TOKEN").is_none() {
        bail!("--execute requires CARGO_REGISTRY_TOKEN in the environment");
    }

    let metadata = load_cargo_metadata()?;
    let client = crates_io_client(args.user_agent.as_deref())?;
    let availability = check_allowlist_availability(
        &client,
        &args.crates_io_api_base,
        &args.version,
        Duration::from_millis(args.request_delay_ms),
    )
    .await?;
    let plan = build_publish_plan(&metadata, &args.version, &availability)?;
    print_plan(&plan, mode);

    if plan.missing_allowlisted_packages.is_empty() {
        println!("all allowlisted packages are already visible on crates.io; nothing to publish");
        return Ok(());
    }

    run_cargo_publish(&plan, mode)?;

    if matches!(mode, PublishMode::Execute) {
        poll_until_visible(
            &client,
            &args.crates_io_api_base,
            &args.version,
            &plan.missing_allowlisted_packages,
            Duration::from_secs(args.poll_interval_secs),
            Duration::from_secs(args.poll_timeout_secs),
        )
        .await?;
    }

    Ok(())
}

fn crates_io_client(user_agent: Option<&str>) -> Result<reqwest::Client> {
    let default_user_agent = format!(
        "agentics-publish/{} (https://github.com/agentic-science/Agentics; contact: agentics@reify.ing)",
        env!("CARGO_PKG_VERSION")
    );
    reqwest::Client::builder()
        .user_agent(user_agent.unwrap_or(&default_user_agent))
        .build()
        .context("build crates.io HTTP client")
}

fn load_cargo_metadata() -> Result<CargoMetadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("parse cargo metadata")
}

async fn check_allowlist_availability(
    client: &reqwest::Client,
    api_base: &str,
    version: &str,
    delay: Duration,
) -> Result<BTreeMap<String, CrateAvailability>> {
    let mut availability = BTreeMap::new();
    for package_name in PUBLISH_ALLOWLIST {
        let status = check_crate_version(client, api_base, package_name, version).await?;
        availability.insert((*package_name).to_owned(), status);
        sleep(delay).await;
    }
    Ok(availability)
}

async fn check_crate_version(
    client: &reqwest::Client,
    api_base: &str,
    package_name: &str,
    version: &str,
) -> Result<CrateAvailability> {
    let url = crate_version_url(api_base, package_name, version)?;
    for attempt in 0..=MAX_API_RETRIES {
        let response = client
            .get(url.clone())
            .header(USER_AGENT, client_user_agent_header())
            .send()
            .await
            .with_context(|| {
                format!("query crates.io availability for {package_name} {version}")
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::OK {
            return Ok(CrateAvailability::Present);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(CrateAvailability::Missing);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            if attempt == MAX_API_RETRIES {
                bail!("crates.io rate limit did not clear for {package_name} {version}");
            }
            sleep(retry_after_delay(&response)).await;
            continue;
        }
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable response body>".to_owned());
        bail!("unexpected crates.io response for {package_name} {version}: {status} {body}");
    }
    Err(anyhow!(
        "exhausted crates.io availability attempts for {package_name} {version}"
    ))
}

fn client_user_agent_header() -> &'static str {
    "agentics-publish"
}

fn retry_after_delay(response: &reqwest::Response) -> Duration {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(10))
}

fn crate_version_url(api_base: &str, package_name: &str, version: &str) -> Result<Url> {
    let normalized_base = format!("{}/", api_base.trim_end_matches('/'));
    let base = Url::parse(&normalized_base)
        .with_context(|| format!("parse crates.io API base URL {api_base}"))?;
    base.join(&format!("crates/{package_name}/{version}"))
        .with_context(|| format!("build crates.io API URL for {package_name} {version}"))
}

fn build_publish_plan(
    metadata: &CargoMetadata,
    version: &str,
    availability: &BTreeMap<String, CrateAvailability>,
) -> Result<PublishPlan> {
    let workspace_packages = workspace_packages(metadata);
    validate_workspace(metadata, &workspace_packages, version)?;

    let mut missing_allowlisted_packages = Vec::new();
    let mut already_published_packages = Vec::new();
    for package_name in PUBLISH_ALLOWLIST {
        match availability.get(*package_name) {
            Some(CrateAvailability::Missing) => {
                missing_allowlisted_packages.push((*package_name).to_owned());
            }
            Some(CrateAvailability::Present) => {
                already_published_packages.push((*package_name).to_owned());
            }
            None => bail!("missing crates.io availability for {package_name}"),
        }
    }

    let publish_targets: BTreeSet<_> = missing_allowlisted_packages.iter().cloned().collect();
    let cargo_excludes = workspace_packages
        .keys()
        .filter(|name| !publish_targets.contains(*name))
        .cloned()
        .collect();

    Ok(PublishPlan {
        version: version.to_owned(),
        missing_allowlisted_packages,
        already_published_packages,
        cargo_excludes,
    })
}

fn workspace_packages(metadata: &CargoMetadata) -> BTreeMap<String, CargoPackage> {
    let workspace_member_ids: BTreeSet<_> = metadata.workspace_members.iter().collect();
    metadata
        .packages
        .iter()
        .filter(|package| workspace_member_ids.contains(&package.id))
        .map(|package| (package.name.clone(), package.clone()))
        .collect()
}

fn validate_workspace(
    metadata: &CargoMetadata,
    workspace_packages: &BTreeMap<String, CargoPackage>,
    version: &str,
) -> Result<()> {
    if metadata.workspace_members.is_empty() {
        bail!("cargo metadata did not report workspace members");
    }

    for package_name in PUBLISH_ALLOWLIST {
        let package = workspace_packages
            .get(*package_name)
            .with_context(|| format!("allowlisted package {package_name} is missing locally"))?;
        if package.version != version {
            bail!(
                "allowlisted package {package_name} has version {}, expected {version}",
                package.version
            );
        }
        if matches!(package.publish.as_deref(), Some([])) {
            bail!("allowlisted package {package_name} is marked publish = false");
        }
    }

    for package_name in NON_PUBLISHABLE_PACKAGES {
        let package = workspace_packages.get(*package_name).with_context(|| {
            format!("non-publishable package {package_name} is missing locally")
        })?;
        if !matches!(package.publish.as_deref(), Some([])) {
            bail!("non-publishable package {package_name} must be marked publish = false");
        }
    }

    Ok(())
}

fn print_plan(plan: &PublishPlan, mode: PublishMode) {
    let mode_label = match mode {
        PublishMode::DryRun => "dry-run",
        PublishMode::Execute => "execute",
    };
    println!("Agentics publish plan ({mode_label}) for {}", plan.version);
    println!("missing allowlisted packages:");
    for package_name in &plan.missing_allowlisted_packages {
        println!("  - {package_name}");
    }
    println!("already published packages:");
    for package_name in &plan.already_published_packages {
        println!("  - {package_name}");
    }
}

fn run_cargo_publish(plan: &PublishPlan, mode: PublishMode) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args(["publish", "--workspace", "--locked"]);
    if matches!(mode, PublishMode::DryRun) {
        command.arg("--dry-run");
    }
    for package_name in &plan.cargo_excludes {
        command.args(["--exclude", package_name]);
    }
    let status = command.status().context("run cargo publish")?;
    if !status.success() {
        bail!("cargo publish failed with status {status}");
    }
    Ok(())
}

async fn poll_until_visible(
    client: &reqwest::Client,
    api_base: &str,
    version: &str,
    package_names: &[String],
    interval: Duration,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("publish visibility timeout is too large")?;
    let mut pending: BTreeSet<_> = package_names.iter().cloned().collect();

    while !pending.is_empty() {
        let mut newly_visible = Vec::new();
        for package_name in &pending {
            if matches!(
                check_crate_version(client, api_base, package_name, version).await?,
                CrateAvailability::Present
            ) {
                newly_visible.push(package_name.clone());
            }
        }
        for package_name in newly_visible {
            pending.remove(&package_name);
        }
        if pending.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for crates.io visibility for: {}",
                pending.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        sleep(interval).await;
    }

    println!("all published packages are visible on crates.io");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, version: &str, publish: Option<Vec<String>>) -> CargoPackage {
        CargoPackage {
            name: name.to_owned(),
            version: version.to_owned(),
            id: format!("path+file:///repo/{name}#{name}@{version}"),
            publish,
        }
    }

    fn metadata_for(names: &[&str], version: &str) -> CargoMetadata {
        let mut packages = Vec::new();
        let mut workspace_members = Vec::new();
        for name in names {
            let publish = if NON_PUBLISHABLE_PACKAGES.contains(name) {
                Some(Vec::new())
            } else {
                None
            };
            let cargo_package = package(name, version, publish);
            workspace_members.push(cargo_package.id.clone());
            packages.push(cargo_package);
        }
        CargoMetadata {
            packages,
            workspace_members,
        }
    }

    fn all_workspace_names() -> Vec<&'static str> {
        PUBLISH_ALLOWLIST
            .iter()
            .chain(NON_PUBLISHABLE_PACKAGES.iter())
            .copied()
            .collect()
    }

    #[test]
    fn plan_excludes_already_published_and_non_publishable_packages() {
        let metadata = metadata_for(&all_workspace_names(), "0.3.0");
        let availability = PUBLISH_ALLOWLIST
            .iter()
            .map(|name| {
                let status = if *name == "agentics-domain" {
                    CrateAvailability::Present
                } else {
                    CrateAvailability::Missing
                };
                ((*name).to_owned(), status)
            })
            .collect();

        let plan =
            build_publish_plan(&metadata, "0.3.0", &availability).expect("plan should build");

        assert!(
            !plan
                .missing_allowlisted_packages
                .contains(&"agentics-domain".to_owned())
        );
        assert!(
            plan.already_published_packages
                .contains(&"agentics-domain".to_owned())
        );
        assert!(plan.cargo_excludes.contains(&"agentics-domain".to_owned()));
        assert!(plan.cargo_excludes.contains(&"agentics-ops".to_owned()));
    }

    #[test]
    fn plan_rejects_publishable_helper_package() {
        let mut metadata = metadata_for(&all_workspace_names(), "0.3.0");
        let helper = metadata
            .packages
            .iter_mut()
            .find(|package| package.name == "agentics-ops")
            .expect("helper package exists");
        helper.publish = None;
        let availability = PUBLISH_ALLOWLIST
            .iter()
            .map(|name| ((*name).to_owned(), CrateAvailability::Missing))
            .collect();

        let error = build_publish_plan(&metadata, "0.3.0", &availability)
            .expect_err("helper package must be publish=false");

        assert!(error.to_string().contains("agentics-ops"));
    }

    #[test]
    fn crate_version_url_keeps_api_v1_path() {
        let url = crate_version_url("https://crates.io/api/v1", "agentics", "0.3.0")
            .expect("url should parse");

        assert_eq!(
            url.as_str(),
            "https://crates.io/api/v1/crates/agentics/0.3.0"
        );
    }
}
