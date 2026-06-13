//! Production rehearsal harness for staging deployments.
//!
//! The normal repository test suite proves code behavior in a controlled test
//! harness. This command proves that a production-like deployment can survive a
//! small end-to-end release rehearsal from outside the API while using ops-only
//! DB/storage access to seed disposable challenge fixtures.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use agentics_config::Config;
use agentics_storage::build_storage;
use clap::{Parser, Subcommand, ValueEnum};
use cleanup::run_cleanup_phase;
use reqwest::{Client, Url};
use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::PgPoolOptions;

use crate::support::run_with_ctrl_c;

mod browser;
mod cleanup;
mod error;
mod fixtures;
mod http;
mod report;
mod runtime;
mod submissions;

use browser::run_browser_phase;
pub use error::ProductionRehearsalError;
use fixtures::{
    RehearsalImageConfig, coexecuted_solution_zip_base64, network_probe_zip_base64,
    oversized_note_zip_base64, piped_stdio_solution_zip_base64, private_data_probe_zip_base64,
    separated_solution_zip_base64, traversal_zip_base64, write_rehearsal_fixtures,
};
use http::{admin_get_json, admin_post_json, get_json, join_url, response_to_json};
use report::{
    CheckEvidence, PhaseEvidence, RehearsalChallengeEvidence, RehearsalReport, RehearsalStatus,
    print_report_summary,
};
use runtime::{registration_code, resolve_run_config};
use submissions::{
    create_agent_submission, expect_submission_rejected, public_projection_check,
    wait_for_submission,
};

const PREFIX: &str = "agentics-rehearse";
const DEFAULT_ENV_FILE: &str = "deploy/compose/env/prod.env";
const DEFAULT_WAIT_TIMEOUT_SECONDS: u64 = 240;
const DEFAULT_CPU_IMAGE_SOURCE: &str = "registry";
const DEFAULT_CPU_IMAGE_REFERENCE: &str = "ghcr.io/agentic-science/agentics-linux-arm64-cpu:ubuntu26.04-v0.2.5@sha256:7ba1dbfb4de62ce7c8716fbdf6fa9e840004cc2d231ac9c0adfd655cd275a537";

/// CLI for the production rehearsal harness.
#[derive(Debug, Parser)]
#[command(
    about = "Run Agentics production rehearsal checks.",
    long_about = "Runs an operator-facing rehearsal against a production-like staging deployment. Without --confirm-disposable-staging it performs read-only preflight checks only. Mutating phases seed run-id-scoped fixtures through DB/storage and exercise the deployed API, worker, runner Docker, and web frontend."
)]
pub struct Cli {
    #[command(subcommand)]
    command: RehearsalCommand,
}

/// Rehearsal subcommands.
#[derive(Debug, Subcommand)]
pub enum RehearsalCommand {
    /// Run one production rehearsal.
    Run(RunArgs),
}

/// Arguments for one rehearsal run.
#[derive(Debug, Parser)]
pub struct RunArgs {
    /// Env file loaded before Config parsing. Defaults to deploy/compose/env/prod.env when present.
    #[arg(long)]
    env_file: Option<PathBuf>,
    /// API base URL. Falls back to AGENTICS_API_BASE_URL, then local API defaults.
    #[arg(long)]
    api_base_url: Option<String>,
    /// Web base URL. Falls back to AGENTICS_WEB_BASE_URL. Browser phase is skipped when absent.
    #[arg(long)]
    web_base_url: Option<String>,
    /// Read admin service token from stdin instead of AGENTICS_ADMIN_SERVICE_TOKEN.
    #[arg(long)]
    admin_service_token_stdin: bool,
    /// Output directory for report and evidence.
    #[arg(long)]
    output_dir: Option<PathBuf>,
    /// Stable run id. Defaults to a generated short id.
    #[arg(long)]
    run_id: Option<String>,
    /// Challenge fixture CPU image source, normally registry in staging.
    #[arg(long)]
    cpu_image_source: Option<String>,
    /// Challenge fixture CPU image reference.
    #[arg(long)]
    cpu_image_reference: Option<String>,
    /// GPU rehearsal mode.
    #[arg(long, value_enum, default_value_t = GpuMode::Auto)]
    gpu: GpuMode,
    /// Allow safe-destructive staging mutations.
    #[arg(long)]
    confirm_disposable_staging: bool,
    /// Skip the Playwright browser phase.
    #[arg(long)]
    no_browser: bool,
    /// Keep seeded fixtures active and preserve working files.
    #[arg(long)]
    keep_artifacts: bool,
    /// Per-submission wait timeout in seconds.
    #[arg(long)]
    wait_timeout_seconds: Option<u64>,
}

/// GPU coverage mode for the rehearsal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GpuMode {
    /// Include GPU checks when the environment advertises GPU support.
    Auto,
    /// Require GPU checks.
    Require,
    /// Skip GPU-specific checks.
    Skip,
}

impl GpuMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Require => "require",
            Self::Skip => "skip",
        }
    }
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match cli.command {
            RehearsalCommand::Run(args) => match run(args).await {
                Ok(report) => {
                    print_report_summary(PREFIX, &report);
                    if report.failed() {
                        ExitCode::from(1)
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(error) => {
                    eprintln!("[{PREFIX}] ERROR: {error}");
                    ExitCode::from(2)
                }
            },
        }
    })
    .await
}

async fn run(args: RunArgs) -> Result<RehearsalReport, ProductionRehearsalError> {
    let resolved = resolve_run_config(&args)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(ProductionRehearsalError::HttpClient)?;
    let mut report = RehearsalReport::new(
        resolved.run_id.clone(),
        resolved.api_base_url.to_string(),
        resolved.web_base_url.as_ref().map(ToString::to_string),
        args.confirm_disposable_staging,
        args.gpu.as_str().to_string(),
        resolved.output_dir.clone(),
    );
    let mut state = RehearsalState::default();

    let preflight = run_preflight_phase(
        &client,
        &resolved.api_base_url,
        resolved.web_base_url.as_ref(),
        &resolved.admin_service_token,
        args.gpu,
    )
    .await;
    report.phases.push(preflight);

    if report.failed() {
        report.phases.push(PhaseEvidence::from_checks(
            "mutation-gate",
            Duration::ZERO,
            vec![CheckEvidence::skipped(
                "safe destructive phases",
                "preflight failed, so mutating rehearsal phases were not started",
            )],
        ));
        report.finish();
        report.write().await?;
        return Ok(report);
    }

    if !args.confirm_disposable_staging {
        report.phases.push(PhaseEvidence::from_checks(
            "mutation-gate",
            Duration::ZERO,
            vec![CheckEvidence::skipped(
                "safe destructive phases",
                "pass --confirm-disposable-staging to seed fixtures and run submissions",
            )],
        ));
        report.finish();
        report.write().await?;
        return Ok(report);
    }

    let identity = run_identity_phase(
        &client,
        &resolved.api_base_url,
        &resolved.admin_service_token,
        &resolved.run_id,
        &mut state,
    )
    .await;
    report.phases.push(identity);

    let fixtures = run_fixture_phase(
        &resolved.config,
        &resolved.run_id,
        &resolved.output_dir,
        &resolved.image_config,
        &mut report,
    )
    .await;
    report.phases.push(fixtures);

    let happy_path = run_happy_path_phase(
        &client,
        &resolved.api_base_url,
        &state,
        &report.challenges,
        &mut report.submissions,
        resolved.wait_timeout,
    )
    .await;
    report.phases.push(happy_path);

    let adversarial = run_adversarial_phase(
        &client,
        &resolved.api_base_url,
        &state,
        &report.challenges,
        resolved.wait_timeout,
    )
    .await;
    report.phases.push(adversarial);

    let browser = if report.failed() {
        PhaseEvidence::from_checks(
            "browser",
            Duration::ZERO,
            vec![CheckEvidence::skipped(
                "Playwright",
                "a previous required phase failed, so browser checks were not started",
            )],
        )
    } else {
        run_browser_phase(&args, &report).await
    };
    report.phases.push(browser);

    let cleanup = run_cleanup_phase(&client, &resolved, &args, &report, &state).await;
    report.phases.push(cleanup);

    report.finish();
    report.write().await?;
    Ok(report)
}

#[derive(Debug, Default)]
struct RehearsalState {
    agent_token: Option<SecretString>,
    pioneer_code_id: Option<String>,
}

async fn run_preflight_phase(
    client: &Client,
    api_base_url: &Url,
    web_base_url: Option<&Url>,
    admin_service_token: &secrecy::SecretString,
    gpu_mode: GpuMode,
) -> PhaseEvidence {
    let start = Instant::now();
    let mut checks = Vec::new();

    checks.push(match get_json(client, api_base_url, "healthz").await {
        Ok(value)
            if value.get("status").and_then(serde_json::Value::as_str) == Some("ok")
                && value
                    .pointer("/database/connected")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true) =>
        {
            CheckEvidence::passed("API health", "healthz reports ok and database connected")
        }
        Ok(value) => {
            CheckEvidence::failed("API health", format!("unexpected health payload: {value}"))
        }
        Err(error) => CheckEvidence::failed("API health", error.to_string()),
    });

    checks.push(
        match get_json(client, api_base_url, "api/public/stats").await {
            Ok(value) if value.get("challenge_count").is_some() => {
                CheckEvidence::passed("public stats", "public stats endpoint returned counters")
            }
            Ok(value) => {
                CheckEvidence::failed("public stats", format!("unexpected stats payload: {value}"))
            }
            Err(error) => CheckEvidence::failed("public stats", error.to_string()),
        },
    );

    checks.push(
        match admin_get_json(client, api_base_url, "admin/capacity", admin_service_token).await {
            Ok(value) if value.get("usage").is_some() => CheckEvidence::passed(
                "admin capacity",
                "admin capacity endpoint returned quota usage",
            ),
            Ok(value) => CheckEvidence::failed(
                "admin capacity",
                format!("unexpected capacity payload: {value}"),
            ),
            Err(error) => CheckEvidence::failed("admin capacity", error.to_string()),
        },
    );

    checks.push(
        match admin_get_json(
            client,
            api_base_url,
            "admin/service-heartbeats",
            admin_service_token,
        )
        .await
        {
            Ok(value) => heartbeat_check(value, gpu_mode),
            Err(error) => CheckEvidence::failed("service heartbeats", error.to_string()),
        },
    );

    if let Some(web_base_url) = web_base_url {
        checks.push(match client.get(web_base_url.clone()).send().await {
            Ok(response) if response.status().is_success() => CheckEvidence::passed(
                "web reachability",
                format!("web returned {}", response.status()),
            ),
            Ok(response) => CheckEvidence::failed(
                "web reachability",
                format!("web returned {}", response.status()),
            ),
            Err(error) => CheckEvidence::failed("web reachability", error.to_string()),
        });
    } else {
        checks.push(CheckEvidence::skipped(
            "web reachability",
            "AGENTICS_WEB_BASE_URL or --web-base-url was not provided",
        ));
    }

    PhaseEvidence::from_checks("preflight", start.elapsed(), checks)
}

fn heartbeat_check(value: serde_json::Value, gpu_mode: GpuMode) -> CheckEvidence {
    let Some(items) = value.get("items").and_then(serde_json::Value::as_array) else {
        return CheckEvidence::failed("service heartbeats", "response did not contain items array");
    };
    let workers = items
        .iter()
        .filter(|item| {
            item.get("service_name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.contains("worker"))
        })
        .collect::<Vec<_>>();
    if workers.is_empty() {
        return CheckEvidence::failed("service heartbeats", "no worker heartbeat found");
    }
    if gpu_mode == GpuMode::Require && !workers.iter().any(|worker| worker_advertises_gpu(worker)) {
        return CheckEvidence::failed(
            "service heartbeats",
            "GPU mode is required but no worker heartbeat advertised gpu capability",
        );
    }
    CheckEvidence::passed(
        "service heartbeats",
        format!("{} worker heartbeat(s) present", workers.len()),
    )
}

fn worker_advertises_gpu(worker: &serde_json::Value) -> bool {
    worker
        .get("payload")
        .and_then(|payload| payload.get("accelerators"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|accelerators| {
            accelerators
                .iter()
                .any(|accelerator| accelerator.as_str() == Some("gpu"))
        })
}

async fn run_identity_phase(
    client: &Client,
    api_base_url: &Url,
    admin_service_token: &secrecy::SecretString,
    run_id: &str,
    state: &mut RehearsalState,
) -> PhaseEvidence {
    let start = Instant::now();
    let mut checks = Vec::new();
    let code = registration_code();

    match admin_post_json(
        client,
        api_base_url,
        "admin/pioneer-codes",
        admin_service_token,
        &serde_json::json!({
            "label": "reh",
            "code": code.as_str(),
            "note": format!("created by production rehearsal {run_id}"),
            "max_uses": 1
        }),
    )
    .await
    {
        Ok(value) => {
            state.pioneer_code_id = value
                .pointer("/code/id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            checks.push(CheckEvidence::passed(
                "create pioneer code",
                "created a one-use rehearsal registration code",
            ));
        }
        Err(error) => checks.push(CheckEvidence::failed(
            "create pioneer code",
            error.to_string(),
        )),
    }

    match client
        .post(match join_url(api_base_url, "api/agents/register") {
            Ok(url) => url,
            Err(error) => {
                checks.push(CheckEvidence::failed(
                    "register rehearsal agent",
                    error.to_string(),
                ));
                return PhaseEvidence::from_checks("identity", start.elapsed(), checks);
            }
        })
        .json(&serde_json::json!({
            "display_name": format!("rehearsal-agent-{run_id}"),
            "pioneer_code": code.as_str(),
            "agent_description": "production rehearsal agent"
        }))
        .send()
        .await
        .map_err(ProductionRehearsalError::HttpClient)
    {
        Ok(response) => match response_to_json(response).await {
            Ok(value) => {
                state.agent_token = value
                    .get("token")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| SecretString::from(value.to_string()));
                checks.push(CheckEvidence::passed(
                    "register rehearsal agent",
                    "registered agent and received bearer token",
                ));
            }
            Err(error) => checks.push(CheckEvidence::failed(
                "register rehearsal agent",
                error.to_string(),
            )),
        },
        Err(error) => checks.push(CheckEvidence::failed(
            "register rehearsal agent",
            error.to_string(),
        )),
    }

    PhaseEvidence::from_checks("identity", start.elapsed(), checks)
}

async fn run_fixture_phase(
    config: &Config,
    run_id: &str,
    output_dir: &std::path::Path,
    image_config: &RehearsalImageConfig,
    report: &mut RehearsalReport,
) -> PhaseEvidence {
    let start = Instant::now();
    let mut checks = Vec::new();
    let work_root = output_dir.join("fixtures");

    match write_rehearsal_fixtures(&work_root, run_id, image_config) {
        Ok(fixtures) => {
            report.challenges = fixtures.cpu_challenges();
            let mut seed_config = config.clone();
            let storage_work_root = output_dir.join("storage-work");
            let Some(storage_work_root) = storage_work_root.to_str() else {
                checks.push(CheckEvidence::failed(
                    "seed challenge fixtures",
                    format!(
                        "output storage work root is not UTF-8: {}",
                        storage_work_root.display()
                    ),
                ));
                return PhaseEvidence::from_checks("fixtures", start.elapsed(), checks);
            };
            seed_config.storage.work_root = Some(storage_work_root.to_string());
            match seed_fixtures(&seed_config, &fixtures.root).await {
                Ok(count) => checks.push(CheckEvidence::passed(
                    "seed challenge fixtures",
                    format!("seeded {count} rehearsal challenge bundle(s)"),
                )),
                Err(error) => checks.push(CheckEvidence::failed(
                    "seed challenge fixtures",
                    error.to_string(),
                )),
            }
        }
        Err(error) => checks.push(CheckEvidence::failed(
            "write challenge fixtures",
            error.to_string(),
        )),
    }

    PhaseEvidence::from_checks("fixtures", start.elapsed(), checks)
}

async fn seed_fixtures(
    config: &Config,
    fixture_root: &std::path::Path,
) -> Result<usize, ProductionRehearsalError> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(config.database.url.expose_secret())
        .await?;
    let storage = build_storage(config.storage_factory_options()?).await?;
    let fixture_root = fixture_root.to_str().ok_or_else(|| {
        ProductionRehearsalError::InvalidResponse(format!(
            "fixture root path is not UTF-8: {}",
            fixture_root.display()
        ))
    })?;
    let count = agentics_services::maintenance::ensure_challenges_seeded_from_root(
        &pool,
        config,
        storage.as_ref(),
        fixture_root,
    )
    .await?;
    Ok(count)
}

async fn run_happy_path_phase(
    client: &Client,
    api_base_url: &Url,
    state: &RehearsalState,
    challenges: &[RehearsalChallengeEvidence],
    submissions: &mut report::RehearsalSubmissionEvidence,
    wait_timeout: Duration,
) -> PhaseEvidence {
    let start = Instant::now();
    let mut checks = Vec::new();
    let Some(token) = state.agent_token.as_ref() else {
        return PhaseEvidence::from_checks(
            "happy-path",
            start.elapsed(),
            vec![CheckEvidence::failed(
                "agent token",
                "identity phase did not produce an agent token",
            )],
        );
    };

    for challenge in challenges {
        let artifact = match challenge.mode.as_str() {
            "separated_evaluator" => separated_solution_zip_base64(),
            "piped_stdio" => piped_stdio_solution_zip_base64(),
            "coexecuted_benchmark" => coexecuted_solution_zip_base64(),
            _ => continue,
        };
        let artifact = match artifact {
            Ok(value) => value,
            Err(error) => {
                checks.push(CheckEvidence::failed(
                    format!("{} artifact", challenge.mode),
                    error.to_string(),
                ));
                continue;
            }
        };

        let validation = create_agent_submission(
            client,
            api_base_url,
            token.expose_secret(),
            "api/agent/validation-runs",
            challenge,
            &artifact,
            "production rehearsal validation",
        )
        .await;
        match validation {
            Ok(id) => {
                assign_submission_id(submissions, challenge.mode.as_str(), true, &id);
                checks.push(
                    wait_for_submission(
                        client,
                        api_base_url,
                        token.expose_secret(),
                        &format!("api/agent/validation-runs/{id}"),
                        &format!("{} validation", challenge.mode),
                        wait_timeout,
                    )
                    .await,
                );
            }
            Err(error) => checks.push(CheckEvidence::failed(
                format!("{} validation create", challenge.mode),
                error.to_string(),
            )),
        }

        let official = create_agent_submission(
            client,
            api_base_url,
            token.expose_secret(),
            "api/agent/solution-submissions",
            challenge,
            &artifact,
            "production rehearsal official",
        )
        .await;
        match official {
            Ok(id) => {
                assign_submission_id(submissions, challenge.mode.as_str(), false, &id);
                checks.push(
                    wait_for_submission(
                        client,
                        api_base_url,
                        token.expose_secret(),
                        &format!("api/agent/solution-submissions/{id}"),
                        &format!("{} official", challenge.mode),
                        wait_timeout,
                    )
                    .await,
                );
                checks.push(public_projection_check(client, api_base_url, challenge, &id).await);
            }
            Err(error) => checks.push(CheckEvidence::failed(
                format!("{} official create", challenge.mode),
                error.to_string(),
            )),
        }
    }

    PhaseEvidence::from_checks("happy-path", start.elapsed(), checks)
}

fn assign_submission_id(
    submissions: &mut report::RehearsalSubmissionEvidence,
    mode: &str,
    validation: bool,
    id: &str,
) {
    match (mode, validation) {
        ("separated_evaluator", true) => submissions.separated_validation_id = Some(id.to_string()),
        ("separated_evaluator", false) => submissions.separated_official_id = Some(id.to_string()),
        ("piped_stdio", true) => submissions.piped_stdio_validation_id = Some(id.to_string()),
        ("piped_stdio", false) => submissions.piped_stdio_official_id = Some(id.to_string()),
        ("coexecuted_benchmark", true) => {
            submissions.coexecuted_validation_id = Some(id.to_string())
        }
        ("coexecuted_benchmark", false) => {
            submissions.coexecuted_official_id = Some(id.to_string())
        }
        _ => {}
    }
}

async fn run_adversarial_phase(
    client: &Client,
    api_base_url: &Url,
    state: &RehearsalState,
    challenges: &[RehearsalChallengeEvidence],
    wait_timeout: Duration,
) -> PhaseEvidence {
    let start = Instant::now();
    let mut checks = Vec::new();
    let Some(token) = state.agent_token.as_ref() else {
        return PhaseEvidence::from_checks(
            "adversarial",
            start.elapsed(),
            vec![CheckEvidence::failed(
                "agent token",
                "identity phase did not produce an agent token",
            )],
        );
    };
    let Some(separated) = challenges
        .iter()
        .find(|challenge| challenge.mode == "separated_evaluator")
    else {
        return PhaseEvidence::from_checks(
            "adversarial",
            start.elapsed(),
            vec![CheckEvidence::failed(
                "separated fixture",
                "missing separated evaluator fixture",
            )],
        );
    };

    for (name, artifact) in [
        ("oversized manifest note", oversized_note_zip_base64()),
        ("archive traversal entry", traversal_zip_base64()),
    ] {
        match artifact {
            Ok(artifact) => {
                checks.push(
                    expect_submission_rejected(
                        client,
                        api_base_url,
                        token.expose_secret(),
                        separated,
                        &artifact,
                        name,
                    )
                    .await,
                );
            }
            Err(error) => checks.push(CheckEvidence::failed(name, error.to_string())),
        }
    }

    match network_probe_zip_base64() {
        Ok(artifact) => {
            match create_agent_submission(
                client,
                api_base_url,
                token.expose_secret(),
                "api/agent/validation-runs",
                separated,
                &artifact,
                "network-disabled rehearsal probe",
            )
            .await
            {
                Ok(id) => {
                    let check = wait_for_submission(
                        client,
                        api_base_url,
                        token.expose_secret(),
                        &format!("api/agent/validation-runs/{id}"),
                        "run-stage network probe",
                        wait_timeout,
                    )
                    .await;
                    checks.push(expect_terminal_failure(check, "run-stage network probe"));
                }
                Err(error) => checks.push(CheckEvidence::failed(
                    "run-stage network probe",
                    format!("probe submission was rejected before runner execution: {error}"),
                )),
            }
        }
        Err(error) => checks.push(CheckEvidence::failed(
            "run-stage network probe",
            error.to_string(),
        )),
    }

    match private_data_probe_zip_base64() {
        Ok(artifact) => {
            match create_agent_submission(
                client,
                api_base_url,
                token.expose_secret(),
                "api/agent/solution-submissions",
                separated,
                &artifact,
                "private-data absence rehearsal probe",
            )
            .await
            {
                Ok(id) => {
                    checks.push(
                        wait_for_submission(
                            client,
                            api_base_url,
                            token.expose_secret(),
                            &format!("api/agent/solution-submissions/{id}"),
                            "participant private-data probe",
                            wait_timeout,
                        )
                        .await,
                    );
                }
                Err(error) => checks.push(CheckEvidence::failed(
                    "participant private-data probe",
                    error.to_string(),
                )),
            }
        }
        Err(error) => checks.push(CheckEvidence::failed(
            "participant private-data probe",
            error.to_string(),
        )),
    }

    PhaseEvidence::from_checks("adversarial", start.elapsed(), checks)
}

fn expect_terminal_failure(check: CheckEvidence, name: &str) -> CheckEvidence {
    if check.status == RehearsalStatus::Failed && check.message.contains("status failed") {
        CheckEvidence::passed(name, "probe was accepted and failed under runner policy")
    } else {
        check
    }
}

#[cfg(test)]
mod tests;
