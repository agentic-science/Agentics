//! Helpers for loading and validating filesystem challenge bundles.
//!
//! Challenge bundles are the public contract between seeded/admin-authored
//! challenges and the runner. Validation accepts the relaxed JSON shape used by
//! the platform: optional nullable fields may be omitted, but contract names are
//! kept explicit and canonical.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::error::{AppError, Result};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengePrepareSpec, ChallengeRunInputFile, ChallengeRunManifest,
    ChallengeRunSpec, ChallengeSolutionPublicationPolicy, PrivateBenchmarkPolicy,
};
use crate::models::paths::BundleRelativePath;
use crate::zip_project::{ZIP_PROJECT_MANIFEST_FILE, ZIP_PROJECT_PROTOCOL};

mod community;
mod filesystem;
mod images;

pub use filesystem::{challenge_bundle_tree_sha256, copy_challenge_bundle_dir};

/// Read `spec.json` from a bundle directory and validate its contract fields.
pub async fn read_challenge_bundle_spec(bundle_dir: &Path) -> Result<ChallengeBundleSpec> {
    let spec_path = bundle_dir.join("spec.json");
    let raw = tokio::fs::read_to_string(&spec_path).await?;
    let spec: ChallengeBundleSpec = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid spec.json: {e}")))?;
    validate_challenge_bundle_spec(&spec)?;
    Ok(spec)
}

/// Read and validate one challenge-owned run manifest from a bundle directory.
pub async fn read_challenge_run_manifest(
    bundle_dir: &Path,
    manifest_path: &BundleRelativePath,
) -> Result<ChallengeRunManifest> {
    read_challenge_run_manifest_file(
        &bundle_dir.join(manifest_path.as_path()),
        &format!("run manifest {manifest_path}"),
    )
    .await
}

/// Read and validate a challenge-owned run manifest from an already resolved path.
pub async fn read_challenge_run_manifest_file(
    manifest_file: &Path,
    label: &str,
) -> Result<ChallengeRunManifest> {
    let raw = tokio::fs::read_to_string(manifest_file).await?;
    let manifest: ChallengeRunManifest = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid {label}: {e}")))?;
    validate_challenge_run_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate that a challenge bundle has the required files and declared data directories.
pub async fn validate_challenge_bundle(bundle_dir: &Path) -> Result<()> {
    let spec = read_challenge_bundle_spec(bundle_dir).await?;
    let spec_path = bundle_dir.join("spec.json");
    let statement_path = bundle_dir.join("statement.md");
    let public_dir = bundle_dir.join(spec.datasets.public_dir.as_path());

    assert_path_type(&spec_path, "file", "spec.json").await?;
    assert_path_type(&statement_path, "file", "statement.md").await?;
    if let Some(script_path) = declared_scorer_script(&spec.scorer.command) {
        assert_path_type(&bundle_dir.join(script_path), "file", "scorer script").await?;
    }
    for (label, prepare) in [
        (
            "validation prepare script",
            spec.execution.validation_prepare.as_ref(),
        ),
        (
            "official prepare script",
            spec.execution.official_prepare.as_ref(),
        ),
    ] {
        if let Some(prepare) = prepare
            && let Some(script_path) = declared_scorer_script(&prepare.command)
        {
            assert_path_type(&bundle_dir.join(script_path), "file", label).await?;
        }
    }
    assert_path_type(&public_dir, "directory", "public data dir").await?;

    if spec.targets.iter().any(|target| target.validation_enabled)
        && let Some(validation_runs) = spec.execution.validation_runs.as_ref()
    {
        assert_path_type(
            &bundle_dir.join(validation_runs.as_path()),
            "file",
            "validation run manifest",
        )
        .await?;
        let manifest = read_challenge_run_manifest(bundle_dir, validation_runs).await?;
        validate_challenge_run_manifest_sources(bundle_dir, &manifest).await?;
    }

    if spec.datasets.private_benchmark_enabled {
        if let Some(ref private_benchmark_dir) = spec.datasets.private_benchmark_dir {
            assert_path_type(
                &bundle_dir.join(private_benchmark_dir.as_path()),
                "directory",
                "private benchmark data dir",
            )
            .await?;
        }
        if let Some(official_runs) = spec.execution.official_runs.as_ref() {
            assert_path_type(
                &bundle_dir.join(official_runs.as_path()),
                "file",
                "official run manifest",
            )
            .await?;
            let manifest = read_challenge_run_manifest(bundle_dir, official_runs).await?;
            validate_challenge_run_manifest_sources(bundle_dir, &manifest).await?;
        }
    }

    Ok(())
}

async fn assert_path_type(path: &Path, kind: &str, label: &str) -> Result<()> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| {
        AppError::Validation(format!("{} does not exist: {}", label, path.display()))
    })?;

    if kind == "file" && !meta.is_file() {
        return Err(AppError::Validation(format!(
            "{} is not a file: {}",
            label,
            path.display()
        )));
    }
    if kind == "directory" && !meta.is_dir() {
        return Err(AppError::Validation(format!(
            "{} is not a directory: {}",
            label,
            path.display()
        )));
    }

    Ok(())
}

/// Return whether `value` can be safely joined under a bundle root.
pub fn is_safe_relative_path(value: &str) -> bool {
    if value.starts_with('/') {
        return false;
    }
    value.split(['/', '\\']).all(|s| !s.is_empty() && s != "..")
}

fn validate_challenge_bundle_spec(spec: &ChallengeBundleSpec) -> Result<()> {
    require_non_empty(&spec.challenge_title, "challenge_title")?;
    require_non_empty(&spec.challenge_summary, "challenge_summary")?;

    if spec.schema_version != 1 {
        return Err(AppError::Validation("schema_version must be 1".to_string()));
    }
    if spec.solution.protocol != ZIP_PROJECT_PROTOCOL {
        return Err(AppError::Validation(format!(
            "solution.protocol must be {ZIP_PROJECT_PROTOCOL}"
        )));
    }
    if spec.solution.manifest_file.as_str() != ZIP_PROJECT_MANIFEST_FILE {
        return Err(AppError::Validation(format!(
            "solution.manifest_file must be {ZIP_PROJECT_MANIFEST_FILE}"
        )));
    }
    validate_scorer_command(&spec.scorer.command)?;
    validate_targets(spec)?;
    validate_challenge_policy(spec)?;
    validate_execution(spec)?;

    if spec.datasets.private_benchmark_policy != PrivateBenchmarkPolicy::ScoreOnly {
        return Err(AppError::Validation(
            "datasets.private_benchmark_policy must be score_only".to_string(),
        ));
    }

    // Challenge authors may stage private benchmark data before enabling
    // official runs. Static official run manifests need a private directory,
    // while prepare-generated official runs may only need private seeds.
    match (
        spec.datasets.private_benchmark_enabled,
        spec.datasets.private_benchmark_dir.as_ref(),
        spec.execution.official_runs.is_some(),
    ) {
        (true, Some(_), _) => {}
        (true, None, true) => {
            return Err(AppError::Validation(
                "datasets.private_benchmark_dir is required when private_benchmark_enabled uses static official_runs"
                    .to_string(),
            ));
        }
        (true, None, false) => {}
        (false, Some(_), _) => {}
        (false, None, _) => {}
    }

    validate_metric_schema(spec)?;
    community::validate_community(spec)?;

    Ok(())
}

/// Require immutable Docker image references for hosted or audited execution.
pub fn validate_digest_pinned_images(spec: &ChallengeBundleSpec) -> Result<()> {
    for (index, target) in spec.targets.iter().enumerate() {
        let field = format!("targets[{index}].resource_profile");
        images::require_image_digest_reference(
            &target.resource_profile.solution_image,
            &format!("{field}.solution_image"),
        )?;
        images::require_image_digest_reference(
            &target.resource_profile.scorer_image,
            &format!("{field}.scorer_image"),
        )?;
    }

    Ok(())
}

fn validate_scorer_command(command: &[String]) -> Result<()> {
    if command.is_empty() {
        return Err(AppError::Validation(
            "scorer.command must not be empty".to_string(),
        ));
    }
    for (index, part) in command.iter().enumerate() {
        require_non_empty(part, &format!("scorer.command[{index}]"))?;
        if part.contains('\0') {
            return Err(AppError::Validation(format!(
                "scorer.command[{index}] must not contain NUL bytes"
            )));
        }
    }

    Ok(())
}

fn validate_prepare_command(command: &[String], field: &str) -> Result<()> {
    if command.is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    for (index, part) in command.iter().enumerate() {
        require_non_empty(part, &format!("{field}[{index}]"))?;
        if part.contains('\0') {
            return Err(AppError::Validation(format!(
                "{field}[{index}] must not contain NUL bytes"
            )));
        }
    }

    Ok(())
}

fn declared_scorer_script(command: &[String]) -> Option<&str> {
    command
        .iter()
        .find(|part| is_safe_relative_path(part) && part.ends_with(".py"))
        .map(String::as_str)
}

fn validate_targets(spec: &ChallengeBundleSpec) -> Result<()> {
    if spec.targets.is_empty() {
        return Err(AppError::Validation(
            "targets must not be empty".to_string(),
        ));
    }

    let mut target_names = HashSet::with_capacity(spec.targets.len());
    for (index, target) in spec.targets.iter().enumerate() {
        let field = format!("targets[{index}]");
        images::validate_target(target, &field)?;
        if !target_names.insert(target.name.as_str()) {
            return Err(AppError::Validation(format!(
                "targets contains duplicate name `{}`",
                target.name
            )));
        }
    }

    Ok(())
}

fn validate_challenge_policy(spec: &ChallengeBundleSpec) -> Result<()> {
    let starts_at = parse_optional_rfc3339(spec.starts_at.as_deref(), "starts_at")?;
    let closes_at = parse_optional_rfc3339(spec.closes_at.as_deref(), "closes_at")?;
    if let (Some(starts_at), Some(closes_at)) = (starts_at, closes_at)
        && closes_at <= starts_at
    {
        return Err(AppError::Validation(
            "closes_at must be later than starts_at".to_string(),
        ));
    }
    if spec.solution_publication == ChallengeSolutionPublicationPolicy::PublicAfterClose
        && closes_at.is_none()
    {
        return Err(AppError::Validation(
            "closes_at is required when solution_publication is public_after_close".to_string(),
        ));
    }
    validate_optional_positive_limit(
        spec.validation_submission_limit,
        "validation_submission_limit",
    )?;
    validate_optional_positive_limit(spec.official_submission_limit, "official_submission_limit")?;

    Ok(())
}

fn parse_optional_rfc3339(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("{field} must be RFC3339: {e}")))
        })
        .transpose()
}

fn validate_optional_positive_limit(value: Option<i64>, field: &str) -> Result<()> {
    if let Some(value) = value
        && value <= 0
    {
        return Err(AppError::Validation(format!("{field} must be positive")));
    }
    Ok(())
}

fn validate_execution(spec: &ChallengeBundleSpec) -> Result<()> {
    if let Some(prepare) = &spec.execution.validation_prepare {
        validate_prepare_spec(prepare, "execution.validation_prepare")?;
    }
    if let Some(prepare) = &spec.execution.official_prepare {
        validate_prepare_spec(prepare, "execution.official_prepare")?;
    }
    if spec.execution.validation_runs.is_some() && spec.execution.validation_prepare.is_some() {
        return Err(AppError::Validation(
            "execution must not declare both validation_runs and validation_prepare".to_string(),
        ));
    }
    if spec.execution.official_runs.is_some() && spec.execution.official_prepare.is_some() {
        return Err(AppError::Validation(
            "execution must not declare both official_runs and official_prepare".to_string(),
        ));
    }
    if spec.targets.iter().any(|target| target.validation_enabled)
        && spec.execution.validation_runs.is_none()
        && spec.execution.validation_prepare.is_none()
    {
        return Err(AppError::Validation(
            "execution.validation_runs or execution.validation_prepare is required when any target has validation_enabled true"
                .to_string(),
        ));
    }
    if spec.datasets.private_benchmark_enabled
        && spec.execution.official_runs.is_none()
        && spec.execution.official_prepare.is_none()
    {
        return Err(AppError::Validation(
            "execution.official_runs or execution.official_prepare is required when private_benchmark_enabled is true"
                .to_string(),
        ));
    }

    Ok(())
}

fn validate_prepare_spec(prepare: &ChallengePrepareSpec, field: &str) -> Result<()> {
    validate_prepare_command(&prepare.command, &format!("{field}.command"))?;
    if let Some(notes) = &prepare.reproducibility_notes {
        require_non_empty(notes, &format!("{field}.reproducibility_notes"))?;
    }
    for (index, data) in prepare.external_data.iter().enumerate() {
        let data_field = format!("{field}.external_data[{index}]");
        if let Some(digest) = &data.digest {
            require_non_empty(digest, &format!("{data_field}.digest"))?;
        }
        if let Some(version) = &data.version {
            require_non_empty(version, &format!("{data_field}.version"))?;
        }
    }
    if let Some(cache_key_hint) = &prepare.cache_key_hint {
        require_non_empty(cache_key_hint, &format!("{field}.cache_key_hint"))?;
    }

    Ok(())
}

fn validate_challenge_run_manifest(manifest: &ChallengeRunManifest) -> Result<()> {
    if manifest.runs.is_empty() {
        return Err(AppError::Validation(
            "run manifest must declare at least one run".to_string(),
        ));
    }

    let mut run_names = HashSet::with_capacity(manifest.runs.len());
    for run in &manifest.runs {
        validate_challenge_run(run)?;
        if !run_names.insert(run.run_name.as_str()) {
            return Err(AppError::Validation(format!(
                "run manifest contains duplicate run_name `{}`",
                run.run_name
            )));
        }
    }

    Ok(())
}

fn validate_challenge_run(run: &ChallengeRunSpec) -> Result<()> {
    if run.stdin_json.is_some() && run.stdin_text.is_some() {
        return Err(AppError::Validation(
            "runs[].stdin_json and runs[].stdin_text cannot both be present".to_string(),
        ));
    }
    for input in &run.input_files {
        validate_run_input_file(input)?;
    }
    let mut output_paths = HashSet::with_capacity(run.output_files.len());
    for path in &run.output_files {
        if !output_paths.insert(path.as_str()) {
            return Err(AppError::Validation(format!(
                "runs[].output_files contains duplicate path `{path}`"
            )));
        }
    }

    Ok(())
}

fn validate_run_input_file(input: &ChallengeRunInputFile) -> Result<()> {
    let source_count = [
        input.source_path.is_some(),
        input.content.is_some(),
        input.content_json.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if source_count > 1 {
        return Err(AppError::Validation(
            "runs[].input_files[] must declare only one of source_path, content, or content_json"
                .to_string(),
        ));
    }
    if source_count == 0 {
        return Err(AppError::Validation(
            "runs[].input_files[] must declare source_path, content, or content_json".to_string(),
        ));
    }

    Ok(())
}

/// Validate that source-backed run inputs exist under the bundle root.
pub async fn validate_challenge_run_manifest_sources(
    bundle_dir: &Path,
    manifest: &ChallengeRunManifest,
) -> Result<()> {
    for run in &manifest.runs {
        for input in &run.input_files {
            if let Some(source_path) = &input.source_path {
                let full_path = bundle_dir.join(source_path.as_path());
                let meta = tokio::fs::symlink_metadata(&full_path).await.map_err(|_| {
                    AppError::Validation(format!(
                        "runs[].input_files[].source_path does not exist: {}",
                        full_path.display()
                    ))
                })?;
                if meta.file_type().is_symlink() {
                    return Err(AppError::Validation(format!(
                        "runs[].input_files[].source_path must not be a symlink: {}",
                        full_path.display()
                    )));
                }
                if !meta.is_file() {
                    return Err(AppError::Validation(format!(
                        "runs[].input_files[].source_path is not a file: {}",
                        full_path.display()
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_metric_schema(spec: &ChallengeBundleSpec) -> Result<()> {
    let schema = &spec.metric_schema;
    if schema.metrics.is_empty() {
        return Err(AppError::Validation(
            "metric_schema.metrics must not be empty".to_string(),
        ));
    }

    let mut names = HashSet::with_capacity(schema.metrics.len());
    for metric in &schema.metrics {
        require_non_empty(&metric.label, "metric_schema.metrics[].label")?;
        if let Some(unit) = &metric.unit {
            require_non_empty(unit, "metric_schema.metrics[].unit")?;
        }
        if let Some(metric_description) = &metric.metric_description {
            require_non_empty(
                metric_description,
                "metric_schema.metrics[].metric_description",
            )?;
        }
        if !names.insert(metric.name.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.metrics contains duplicate name `{}`",
                metric.name
            )));
        }
    }

    if !names.contains(schema.ranking.primary_metric_name.as_str()) {
        return Err(AppError::Validation(format!(
            "metric_schema.ranking.primary_metric_name references unknown metric `{}`",
            schema.ranking.primary_metric_name
        )));
    }

    let mut tie_breakers = HashSet::with_capacity(schema.ranking.tie_breaker_metric_names.len());
    for metric_name in &schema.ranking.tie_breaker_metric_names {
        if metric_name == &schema.ranking.primary_metric_name {
            return Err(AppError::Validation(
                "metric_schema.ranking.tie_breaker_metric_names must not repeat the primary metric"
                    .to_string(),
            ));
        }
        if !names.contains(metric_name.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_names references unknown metric `{metric_name}`"
            )));
        }
        if !tie_breakers.insert(metric_name.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_names contains duplicate metric `{metric_name}`"
            )));
        }
    }

    Ok(())
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }

    Ok(())
}

fn validate_positive_u64(value: u64, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

fn validate_positive_u32(value: u32, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::models::challenge::{
        ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
        ChallengeExecutionSpec, ChallengePrepareSpec, ChallengeResultDetailVisibility,
        ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
        ChallengeVisibilitySpec, CommunitySpec, DatasetsSpec, DockerPlatform, HardwareProfileSpec,
        MetricDirection, MetricSchemaSpec, MetricVisibility, PrivateBenchmarkPolicy,
        ResourceProfileSpec, ScorerSpec, SolutionSpec, TargetAccelerator,
    };
    use crate::models::evaluation::ScoreVisibility;
    use crate::models::hashes::OciSha256Digest;
    use crate::models::names::{ChallengeName, MetricName, ResourceProfileName, TargetName};
    use crate::models::paths::BundleRelativePath;
    use crate::models::urls::MoltbookSubmoltUrl;
    use crate::zip_project::ZipProjectNetworkAccess;

    use super::{
        validate_challenge_bundle, validate_challenge_bundle_spec, validate_digest_pinned_images,
    };

    fn test_digest() -> OciSha256Digest {
        OciSha256Digest::try_new(format!("sha256:{}", "a".repeat(64)))
            .expect("test OCI digest is valid")
    }

    fn base_spec() -> ChallengeBundleSpec {
        ChallengeBundleSpec {
            schema_version: 1,
            challenge_name: challenge_name("sample-sum"),
            challenge_title: "Sample Sum".to_string(),
            challenge_summary: "Add numbers from worker-managed runs.".to_string(),
            solution: SolutionSpec {
                protocol: "zip_project".to_string(),
                manifest_file: bundle_path("agentics.solution.json"),
            },
            scorer: ScorerSpec {
                command: vec!["python".to_string(), "scorer/run.py".to_string()],
                result_file: bundle_path("result.json"),
            },
            targets: vec![ChallengeTargetSpec {
                name: target_name("linux-arm64-cpu"),
                docker_platform: DockerPlatform::LinuxArm64,
                accelerator: TargetAccelerator::Cpu,
                validation_enabled: true,
                resource_profile: ResourceProfileSpec {
                    name: resource_profile_name("agentics-cpu-small"),
                    resource_description: None,
                    solution_image: "agentics-linux-arm64-cpu:ubuntu26.04-local".to_string(),
                    solution_image_digest: None,
                    scorer_image: "agentics-linux-arm64-cpu:ubuntu26.04-local".to_string(),
                    scorer_image_digest: None,
                    timeout_sec: 30,
                    memory_limit_mb: 512,
                    cpu_limit_millis: 1000,
                    disk_limit_mb: 1024,
                    setup_network_access: ZipProjectNetworkAccess::Enabled,
                    build_network_access: ZipProjectNetworkAccess::Disabled,
                    run_network_access: ZipProjectNetworkAccess::Disabled,
                    scorer_network_access: ZipProjectNetworkAccess::Disabled,
                    hardware: None,
                },
            }],
            starts_at: None,
            closes_at: None,
            eligibility: ChallengeEligibilitySpec {
                eligibility_type: ChallengeEligibilityType::Open,
            },
            validation_submission_limit: None,
            official_submission_limit: None,
            visibility: ChallengeVisibilitySpec {
                leaderboard: ChallengeVisibility::PublicLive,
                score_distribution: ChallengeVisibility::PublicLive,
                result_detail: ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose,
            },
            solution_publication: ChallengeSolutionPublicationPolicy::Public,
            execution: ChallengeExecutionSpec {
                validation_runs: Some(bundle_path("public/runs.json")),
                validation_prepare: None,
                official_runs: Some(bundle_path("private-benchmark/runs.json")),
                official_prepare: None,
            },
            datasets: DatasetsSpec {
                public_dir: bundle_path("public"),
                private_benchmark_dir: Some(bundle_path("private-benchmark")),
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: true,
            },
            community: None,
            metric_schema: MetricSchemaSpec::default(),
        }
    }

    fn challenge_name(value: &str) -> ChallengeName {
        ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
    }

    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("test target is valid")
    }

    fn metric_name(value: &str) -> MetricName {
        MetricName::try_new(value.to_string()).expect("test metric name is valid")
    }

    fn resource_profile_name(value: &str) -> ResourceProfileName {
        ResourceProfileName::try_new(value.to_string())
            .expect("test resource profile name is valid")
    }

    fn bundle_path(value: &str) -> BundleRelativePath {
        BundleRelativePath::try_new(value).expect("test bundle path is valid")
    }

    fn pin_images(spec: &mut ChallengeBundleSpec) {
        let digest = test_digest();
        for target in &mut spec.targets {
            let image = format!("agentics-linux-arm64-cpu:ubuntu26.04-local@{digest}");
            target.resource_profile.solution_image = image.clone();
            target.resource_profile.solution_image_digest = Some(digest.clone());
            target.resource_profile.scorer_image = image;
            target.resource_profile.scorer_image_digest = Some(digest.clone());
        }
    }

    fn use_cuda_target(target: &mut ChallengeTargetSpec, cuda_variant: &str) {
        target.name = target_name("linux-arm64-cuda");
        target.accelerator = TargetAccelerator::Gpu;
        target.resource_profile.hardware = Some(cuda_hardware());
        let image = format!("agentics-linux-arm64-cuda:{cuda_variant}-ubuntu24.04-local");
        target.resource_profile.solution_image = image.clone();
        target.resource_profile.scorer_image = image;
    }

    fn cuda_hardware() -> HardwareProfileSpec {
        HardwareProfileSpec {
            kind: "cuda".to_string(),
            gpu_model: Some("NVIDIA GB10".to_string()),
            gpu_count: Some(1),
            gpu_memory_gb: Some(128),
            cuda_variant: Some("cu130".to_string()),
            cuda_version: Some("13.0".to_string()),
            driver_minimum: Some(">=580".to_string()),
        }
    }

    #[test]
    fn legacy_rounds_field_is_rejected() {
        let mut spec_json = serde_json::to_value(base_spec()).expect("spec should serialize");
        spec_json["rounds"] = serde_json::json!([
            {
                "id": "main",
                "title": "Main",
                "eligibility": { "type": "open" },
                "visibility": {
                    "leaderboard": "public_live",
                    "score_distribution": "public_live",
                    "result_detail": "submitter_live_public_after_close"
                },
                "solution_publication": "public"
            }
        ]);

        let error = serde_json::from_value::<ChallengeBundleSpec>(spec_json)
            .expect_err("legacy rounds should be an unknown field");
        assert!(error.to_string().contains("rounds"));
    }

    #[test]
    fn targets_are_required() {
        let mut spec = base_spec();
        spec.targets.clear();

        let error = validate_challenge_bundle_spec(&spec).expect_err("empty targets should fail");
        assert!(error.to_string().contains("targets"));
    }

    #[test]
    fn target_name_is_independent_from_docker_platform() {
        let mut spec = base_spec();
        spec.targets[0].name = target_name("main");

        validate_challenge_bundle_spec(&spec)
            .expect("target name should not be coupled to docker platform");
    }

    #[test]
    fn amd64_targets_are_reserved_for_post_mvp() {
        let mut spec = base_spec();
        spec.targets[0].name = target_name("linux-amd64-cpu");
        spec.targets[0].docker_platform = DockerPlatform::LinuxAmd64;

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("amd64 targets should be reserved for post-MVP");
        assert!(error.to_string().contains("post-MVP"));
    }

    #[test]
    fn public_after_close_solution_publication_requires_close_time() {
        let mut spec = base_spec();
        spec.solution_publication = ChallengeSolutionPublicationPolicy::PublicAfterClose;

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("public-after-close artifacts need a close time");
        assert!(error.to_string().contains("solution_publication"));

        spec.closes_at = Some("2999-01-02T00:00:00Z".to_string());
        validate_challenge_bundle_spec(&spec).expect("close time should satisfy policy");
    }

    #[test]
    fn cuda_target_requires_cuda_hardware_metadata() {
        let mut spec = base_spec();
        let target = &mut spec.targets[0];
        target.name = target_name("linux-arm64-cuda");
        target.accelerator = TargetAccelerator::Gpu;

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("missing cuda hardware should fail");
        assert!(error.to_string().contains("hardware.kind"));

        spec.targets[0].resource_profile.hardware = Some(cuda_hardware());
        let image = "agentics-linux-arm64-cuda:cu130-ubuntu24.04-local".to_string();
        spec.targets[0].resource_profile.solution_image = image.clone();
        spec.targets[0].resource_profile.scorer_image = image;
        validate_challenge_bundle_spec(&spec).expect("cuda target should validate");
    }

    #[test]
    fn cpu_target_rejects_unsupported_image_repository() {
        let mut spec = base_spec();
        spec.targets[0].resource_profile.solution_image = "python:3.12-slim-bookworm".to_string();

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("unsupported image repository should fail");

        assert!(
            error
                .to_string()
                .contains("supported Agentics image repository")
        );
    }

    #[test]
    fn cpu_target_rejects_unsupported_image_tag() {
        let mut spec = base_spec();
        let image = "agentics-linux-arm64-cpu:bookworm".to_string();
        spec.targets[0].resource_profile.solution_image = image.clone();
        spec.targets[0].resource_profile.scorer_image = image;

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("unsupported image tag should fail");

        assert!(error.to_string().contains("tag must start with"));
    }

    #[test]
    fn cuda_target_accepts_matching_supported_image() {
        let mut spec = base_spec();
        use_cuda_target(&mut spec.targets[0], "cu130");

        validate_challenge_bundle_spec(&spec).expect("matching cuda image should validate");
    }

    #[test]
    fn cuda_target_rejects_mismatched_image_variant() {
        let mut spec = base_spec();
        use_cuda_target(&mut spec.targets[0], "cu132");

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("mismatched cuda image variant should fail");

        assert!(error.to_string().contains("tag must start with `cu130-`"));
    }

    #[test]
    fn cuda_target_rejects_unsupported_cuda_variant() {
        let mut spec = base_spec();
        let target = &mut spec.targets[0];
        target.name = target_name("linux-arm64-cuda");
        target.accelerator = TargetAccelerator::Gpu;
        target.resource_profile.hardware = Some(HardwareProfileSpec {
            cuda_variant: Some("cu129".to_string()),
            cuda_version: Some("12.9".to_string()),
            ..cuda_hardware()
        });

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("unsupported cuda variant should fail");
        assert!(error.to_string().contains("supported variants"));
    }

    #[test]
    fn cuda_target_rejects_mismatched_cuda_version() {
        let mut spec = base_spec();
        let target = &mut spec.targets[0];
        target.name = target_name("linux-arm64-cuda");
        target.accelerator = TargetAccelerator::Gpu;
        target.resource_profile.hardware = Some(HardwareProfileSpec {
            cuda_variant: Some("cu132".to_string()),
            cuda_version: Some("13.0".to_string()),
            ..cuda_hardware()
        });

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("mismatched cuda version should fail");
        assert!(error.to_string().contains("cuda_version"));
    }

    #[test]
    fn digest_pinned_image_policy_rejects_tag_only_images() {
        let spec = base_spec();

        let error =
            validate_digest_pinned_images(&spec).expect_err("tag-only images should fail policy");

        assert!(error.to_string().contains("@sha256:<digest>"));
    }

    #[test]
    fn digest_pinned_image_policy_accepts_immutable_references() {
        let mut spec = base_spec();
        pin_images(&mut spec);

        validate_challenge_bundle_spec(&spec).expect("pinned spec should validate");
        validate_digest_pinned_images(&spec).expect("pinned images should satisfy policy");
    }

    #[test]
    fn image_digest_field_must_match_image_reference() {
        let mut spec = base_spec();
        pin_images(&mut spec);
        spec.targets[0].resource_profile.solution_image_digest = Some(
            OciSha256Digest::try_new(format!("sha256:{}", "b".repeat(64)))
                .expect("test OCI digest is valid"),
        );

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("mismatched digest should fail");

        assert!(error.to_string().contains("must match"));
    }

    #[test]
    fn challenge_summary_is_required() {
        let mut spec = base_spec();
        spec.challenge_summary.clear();

        let error = validate_challenge_bundle_spec(&spec).expect_err("empty summary should fail");
        assert!(error.to_string().contains("challenge_summary"));
    }

    #[test]
    fn disabled_private_benchmark_may_still_declare_directory() {
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn enabled_private_benchmark_requires_directory() {
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = true;
        spec.datasets.private_benchmark_dir = None;

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn validation_run_manifest_required_only_when_target_enables_validation() {
        let mut spec = base_spec();
        spec.execution.validation_runs = None;
        spec.targets[0].validation_enabled = false;

        assert!(validate_challenge_bundle_spec(&spec).is_ok());

        spec.targets[0].validation_enabled = true;
        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("target validation should require run manifest");
        assert!(error.to_string().contains("execution.validation_runs"));
    }

    #[test]
    fn validation_prepare_satisfies_validation_enabled_target() {
        let mut spec = base_spec();
        spec.execution.validation_runs = None;
        spec.execution.validation_prepare = Some(prepare_spec());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn official_prepare_satisfies_private_benchmark_execution() {
        let mut spec = base_spec();
        spec.execution.official_runs = None;
        spec.execution.official_prepare = Some(prepare_spec());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn official_prepare_may_omit_private_benchmark_directory() {
        let mut spec = base_spec();
        spec.execution.official_runs = None;
        spec.execution.official_prepare = Some(prepare_spec());
        spec.datasets.private_benchmark_dir = None;

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn prepare_and_static_runs_are_mutually_exclusive_per_mode() {
        let mut spec = base_spec();
        spec.execution.official_prepare = Some(prepare_spec());

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("official prepare and official runs should conflict");
        assert!(error.to_string().contains("official_runs"));
    }

    #[test]
    fn metric_schema_rejects_unknown_primary_metric() {
        let mut spec = base_spec();
        spec.metric_schema.ranking.primary_metric_name = metric_name("missing");

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_rejects_duplicate_metric_names() {
        let mut spec = base_spec();
        let mut duplicate = spec.metric_schema.metrics[0].clone();
        duplicate.label = "Duplicate Score".to_string();
        spec.metric_schema.metrics.push(duplicate);

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_accepts_tie_breaker_metadata() {
        let mut spec = base_spec();
        spec.metric_schema
            .metrics
            .push(crate::models::challenge::MetricDefinitionSpec {
                name: metric_name("runtime_ms"),
                label: "Runtime".to_string(),
                unit: Some("ms".to_string()),
                direction: MetricDirection::Minimize,
                visibility: MetricVisibility::Public,
                metric_description: Some("Wall-clock runtime in milliseconds.".to_string()),
            });
        spec.metric_schema
            .ranking
            .tie_breaker_metric_names
            .push(metric_name("runtime_ms"));

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn community_accepts_moltbook_submolt_metadata() {
        let mut spec = base_spec();
        spec.community = Some(CommunitySpec {
            moltbook_submolt_name: Some("agentics-sample-sum".to_string()),
            moltbook_submolt_url: Some(
                MoltbookSubmoltUrl::try_new(
                    "https://www.moltbook.com/submolts/agentics-sample-sum",
                )
                .expect("test Moltbook URL is valid"),
            ),
        });

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn community_rejects_non_moltbook_url() {
        let mut value =
            serde_json::to_value(base_spec()).expect("base spec should serialize to JSON");
        value["community"] = serde_json::json!({
            "moltbook_submolt_name": "agentics-sample-sum",
            "moltbook_submolt_url": "https://example.com/agentics-sample-sum"
        });

        let error = serde_json::from_value::<ChallengeBundleSpec>(value)
            .expect_err("invalid URL should fail during typed deserialization");
        assert!(error.to_string().contains("moltbook_submolt_url"));
    }

    #[test]
    fn community_rejects_invalid_submolt_name() {
        let mut spec = base_spec();
        spec.community = Some(CommunitySpec {
            moltbook_submolt_name: Some("agentics sample sum".to_string()),
            moltbook_submolt_url: None,
        });

        let error = validate_challenge_bundle_spec(&spec).expect_err("invalid name should fail");
        assert!(error.to_string().contains("moltbook_submolt_name"));
    }

    fn create_bundle(root: &Path, spec: &ChallengeBundleSpec) {
        std::fs::create_dir_all(root.join("scorer")).expect("failed to create scorer dir");
        std::fs::create_dir_all(root.join("public")).expect("failed to create public dir");
        std::fs::write(
            root.join("public/runs.json"),
            r#"{"runs":[{"run_name":"public-1","interface":"stdio","stdin_text":"1"}]}"#,
        )
        .expect("failed to write public runs");
        std::fs::write(
            root.join("spec.json"),
            serde_json::to_string(spec).expect("failed to serialize spec"),
        )
        .expect("failed to write spec");
        std::fs::write(root.join("statement.md"), "# Sample\n\nBody\n")
            .expect("failed to write statement");
        std::fs::write(root.join("scorer/run.py"), "print('ok')\n")
            .expect("failed to write scorer");
    }

    fn prepare_spec() -> ChallengePrepareSpec {
        ChallengePrepareSpec {
            command: vec!["python".to_string(), "scorer/prepare.py".to_string()],
            result_runs_file: bundle_path("generated/runs.json"),
            network_access: ZipProjectNetworkAccess::Disabled,
            reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
            external_data: Vec::new(),
            cache_key_hint: None,
        }
    }

    #[tokio::test]
    async fn disabled_private_benchmark_bundle_does_not_require_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-disabled-private-benchmark-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
        create_bundle(&root, &spec);

        let result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn source_backed_run_inputs_must_exist_under_bundle_root() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-source-input-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        create_bundle(&root, &spec);
        std::fs::write(
            root.join("public/runs.json"),
            r#"{"runs":[{"run_name":"public-1","interface":"file_system","input_files":[{"path":"input.txt","source_path":"public/input.txt"}],"output_files":["answer.txt"]}]}"#,
        )
        .expect("failed to write source-backed runs");

        let missing_result = validate_challenge_bundle(&root).await;
        std::fs::write(root.join("public/input.txt"), "payload\n")
            .expect("failed to write source input");
        let present_result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        assert!(missing_result.is_err());
        assert!(present_result.is_ok());
    }

    #[tokio::test]
    async fn enabled_private_benchmark_bundle_requires_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-enabled-private-benchmark-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = true;
        spec.datasets.private_benchmark_dir = Some(bundle_path("private-benchmark"));
        create_bundle(&root, &spec);

        let result = validate_challenge_bundle(&root).await;
        drop(std::fs::remove_dir_all(root));

        assert!(result.is_err());
    }
}
