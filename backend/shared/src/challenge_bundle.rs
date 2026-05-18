//! Helpers for loading and validating filesystem challenge bundles.
//!
//! Challenge bundles are the public contract between seeded/admin-authored
//! challenges and the runner. Validation keeps contract names explicit and
//! rejects unknown or stale fields before a bundle can be published.

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

/// Handles assert path type for this module.
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

/// Validates challenge bundle spec invariants for this contract.
fn validate_challenge_bundle_spec(spec: &ChallengeBundleSpec) -> Result<()> {
    require_non_empty(&spec.challenge_title, "challenge_title")?;
    require_non_empty(&spec.summary.en, "summary.en")?;
    require_non_empty(&spec.summary.zh, "summary.zh")?;

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

/// Validates scorer command invariants for this contract.
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

/// Validates prepare command invariants for this contract.
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

/// Handles declared scorer script for this module.
fn declared_scorer_script(command: &[String]) -> Option<&str> {
    command
        .iter()
        .find(|part| is_safe_relative_path(part) && part.ends_with(".py"))
        .map(String::as_str)
}

/// Validates targets invariants for this contract.
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

/// Validates challenge policy invariants for this contract.
fn validate_challenge_policy(spec: &ChallengeBundleSpec) -> Result<()> {
    let starts_at = parse_required_rfc3339(&spec.starts_at, "starts_at")?;
    let closes_at = parse_optional_rfc3339(spec.closes_at.as_deref(), "closes_at")?;
    if let Some(closes_at) = closes_at
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

/// Parses required rfc3339 from an external boundary string.
fn parse_required_rfc3339(value: &str, field: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|e| AppError::Validation(format!("{field} must be RFC3339: {e}")))
}

/// Parses optional rfc3339 from an external boundary string.
fn parse_optional_rfc3339(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("{field} must be RFC3339: {e}")))
        })
        .transpose()
}

/// Validates optional positive limit invariants for this contract.
fn validate_optional_positive_limit(value: Option<i64>, field: &str) -> Result<()> {
    if let Some(value) = value
        && value <= 0
    {
        return Err(AppError::Validation(format!("{field} must be positive")));
    }
    Ok(())
}

/// Validates execution invariants for this contract.
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

/// Validates prepare spec invariants for this contract.
fn validate_prepare_spec(prepare: &ChallengePrepareSpec, field: &str) -> Result<()> {
    validate_prepare_command(&prepare.command, &format!("{field}.command"))?;
    if let Some(notes) = &prepare.reproducibility_notes {
        require_non_empty(notes, &format!("{field}.reproducibility_notes"))?;
    }

    Ok(())
}

/// Validates challenge run manifest invariants for this contract.
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

/// Validates challenge run invariants for this contract.
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

/// Validates run input file invariants for this contract.
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

/// Validates metric schema invariants for this contract.
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

/// Requires non empty and reports a domain error otherwise.
fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }

    Ok(())
}

/// Validates positive u64 invariants for this contract.
fn validate_positive_u64(value: u64, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

/// Validates positive u32 invariants for this contract.
fn validate_positive_u32(value: u32, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests;
