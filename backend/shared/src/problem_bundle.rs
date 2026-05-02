//! Helpers for loading and validating filesystem problem bundles.
//!
//! Problem bundles are the public contract between seeded/admin-authored
//! problems and the runner. Validation here intentionally mirrors the old TS
//! service while allowing omitted nullable fields in serialized JSON.

use std::collections::HashSet;
use std::path::Path;

use crate::error::{AppError, Result};
use crate::models::problem::ProblemBundleSpec;

/// Read `spec.json` from a bundle directory and validate its contract fields.
pub async fn read_problem_bundle_spec(bundle_dir: &Path) -> Result<ProblemBundleSpec> {
    let spec_path = bundle_dir.join("spec.json");
    let raw = tokio::fs::read_to_string(&spec_path).await?;
    let spec: ProblemBundleSpec = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid spec.json: {e}")))?;
    validate_problem_bundle_spec(&spec)?;
    Ok(spec)
}

/// Validate that a problem bundle has the required files and declared data directories.
///
/// Disabled heldout bundles may still declare a heldout directory for
/// compatibility with public-only bundles produced by the TS implementation.
pub async fn validate_problem_bundle(bundle_dir: &Path) -> Result<()> {
    let spec = read_problem_bundle_spec(bundle_dir).await?;
    let spec_path = bundle_dir.join("spec.json");
    let statement_path = bundle_dir.join("statement.md");
    let scorer_path = bundle_dir.join(&spec.scorer.entrypoint);
    let shown_dir = bundle_dir.join(&spec.datasets.shown_dir);
    let hidden_dir = bundle_dir.join(&spec.datasets.hidden_dir);

    assert_path_type(&spec_path, "file", "spec.json").await?;
    assert_path_type(&statement_path, "file", "statement.md").await?;
    assert_path_type(&scorer_path, "file", "scorer entrypoint").await?;
    assert_path_type(&shown_dir, "directory", "shown data dir").await?;
    assert_path_type(&hidden_dir, "directory", "hidden data dir").await?;

    if spec.datasets.heldout_enabled
        && let Some(ref heldout_dir) = spec.datasets.heldout_dir
    {
        assert_path_type(
            &bundle_dir.join(heldout_dir),
            "directory",
            "heldout data dir",
        )
        .await?;
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

/// Extract the first prose paragraph from a Markdown problem statement.
///
/// The result is used as a compact problem-list description, so headings,
/// lists, tables, block quotes, and fenced code are skipped.
pub async fn extract_problem_description(statement_path: &Path) -> Result<String> {
    let content = tokio::fs::read_to_string(statement_path).await?;
    let lines: Vec<&str> = content.lines().collect();
    let mut paragraph: Vec<String> = Vec::new();
    let mut in_code_block = false;

    for raw_line in lines {
        let line = raw_line.trim();

        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        if line.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        if line.starts_with('#')
            || line.starts_with('-')
            || line.starts_with("* ")
            || line.starts_with('>')
            || line.starts_with('|')
            || line
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
                && line.contains(". ")
        {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        paragraph.push(strip_markdown_inline(line));
    }

    Ok(paragraph.join(" ").trim().to_string())
}

fn strip_markdown_inline(value: &str) -> String {
    let mut result = value.to_string();
    // Strip inline code
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let inner = result[start + 1..start + 1 + end].to_string();
            result.replace_range(start..start + 1 + end + 1, &inner);
        } else {
            break;
        }
    }
    // Strip links
    result = regex_replace(&result, r"\[([^\]]+)\]\([^)]+\)", "$1");
    // Strip bold
    result = regex_replace(&result, r"\*\*([^*]+)\*\*", "$1");
    // Strip italic
    result = regex_replace(&result, r"\*([^*]+)\*", "$1");
    result = regex_replace(&result, r"_([^_]+)_", "$1");
    result.trim().to_string()
}

fn regex_replace(input: &str, pattern: &str, replacement: &str) -> String {
    use regex::Regex;
    Regex::new(pattern)
        .unwrap()
        .replace_all(input, replacement)
        .to_string()
}

/// Return whether `value` can be safely joined under a bundle root.
pub fn is_safe_relative_path(value: &str) -> bool {
    if value.starts_with('/') {
        return false;
    }
    let segments: Vec<&str> = value.split(['/', '\\']).collect();
    segments.iter().all(|s| !s.is_empty() && *s != "..")
}

fn validate_problem_bundle_spec(spec: &ProblemBundleSpec) -> Result<()> {
    require_non_empty(&spec.problem_id, "problem_id")?;
    require_non_empty(&spec.problem_title, "problem_title")?;
    require_non_empty(&spec.problem_version, "problem_version")?;

    if spec.schema_version != 1 {
        return Err(AppError::Validation("schema_version must be 1".to_string()));
    }
    if spec.submission.format != "python_zip_project" {
        return Err(AppError::Validation(
            "submission.format must be python_zip_project".to_string(),
        ));
    }
    if spec.submission.language != "python" {
        return Err(AppError::Validation(
            "submission.language must be python".to_string(),
        ));
    }
    require_safe_relative_path(&spec.submission.entrypoint, "submission.entrypoint")?;
    require_safe_relative_path(&spec.scorer.entrypoint, "scorer.entrypoint")?;
    require_safe_relative_path(&spec.scorer.result_file, "scorer.result_file")?;

    if spec.limits.time_limit_sec <= 0.0 || !spec.limits.time_limit_sec.is_finite() {
        return Err(AppError::Validation(
            "limits.time_limit_sec must be positive".to_string(),
        ));
    }
    if spec.limits.memory_limit_mb <= 0 {
        return Err(AppError::Validation(
            "limits.memory_limit_mb must be positive".to_string(),
        ));
    }

    require_safe_relative_path(&spec.datasets.shown_dir, "datasets.shown_dir")?;
    require_safe_relative_path(&spec.datasets.hidden_dir, "datasets.hidden_dir")?;
    if spec.datasets.hidden_policy != "score_only" {
        return Err(AppError::Validation(
            "datasets.hidden_policy must be score_only".to_string(),
        ));
    }

    // The TS-era schema allowed public-only bundles to keep heldout_dir present
    // even when heldout scoring was disabled. Keep accepting that shape while
    // still requiring the path to be safe if it exists.
    match (
        spec.datasets.heldout_enabled,
        spec.datasets.heldout_dir.as_deref(),
    ) {
        (true, Some(path)) => require_safe_relative_path(path, "datasets.heldout_dir")?,
        (true, None) => {
            return Err(AppError::Validation(
                "datasets.heldout_dir is required when heldout_enabled is true".to_string(),
            ));
        }
        (false, Some(path)) => require_safe_relative_path(path, "datasets.heldout_dir")?,
        (false, None) => {}
    }

    validate_metric_schema(spec)?;

    Ok(())
}

fn validate_metric_schema(spec: &ProblemBundleSpec) -> Result<()> {
    let schema = &spec.metric_schema;
    if schema.metrics.is_empty() {
        return Err(AppError::Validation(
            "metric_schema.metrics must not be empty".to_string(),
        ));
    }

    let mut ids = HashSet::with_capacity(schema.metrics.len());
    for metric in &schema.metrics {
        require_metric_id(&metric.id, "metric_schema.metrics[].id")?;
        require_non_empty(&metric.label, "metric_schema.metrics[].label")?;
        if let Some(unit) = &metric.unit {
            require_non_empty(unit, "metric_schema.metrics[].unit")?;
        }
        if let Some(description) = &metric.description {
            require_non_empty(description, "metric_schema.metrics[].description")?;
        }
        if !ids.insert(metric.id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.metrics contains duplicate id `{}`",
                metric.id
            )));
        }
    }

    require_metric_id(
        &schema.ranking.primary_metric_id,
        "metric_schema.ranking.primary_metric_id",
    )?;
    if !ids.contains(schema.ranking.primary_metric_id.as_str()) {
        return Err(AppError::Validation(format!(
            "metric_schema.ranking.primary_metric_id references unknown metric `{}`",
            schema.ranking.primary_metric_id
        )));
    }

    let mut tie_breakers = HashSet::with_capacity(schema.ranking.tie_breaker_metric_ids.len());
    for metric_id in &schema.ranking.tie_breaker_metric_ids {
        require_metric_id(metric_id, "metric_schema.ranking.tie_breaker_metric_ids[]")?;
        if metric_id == &schema.ranking.primary_metric_id {
            return Err(AppError::Validation(
                "metric_schema.ranking.tie_breaker_metric_ids must not repeat the primary metric"
                    .to_string(),
            ));
        }
        if !ids.contains(metric_id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_ids references unknown metric `{metric_id}`"
            )));
        }
        if !tie_breakers.insert(metric_id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_ids contains duplicate metric `{metric_id}`"
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

fn require_safe_relative_path(value: &str, field: &str) -> Result<()> {
    if !is_safe_relative_path(value) {
        return Err(AppError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }

    Ok(())
}

fn require_metric_id(value: &str, field: &str) -> Result<()> {
    require_non_empty(value, field)?;
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::Validation(format!(
            "{field} must contain only ASCII letters, digits, underscores, hyphens, or dots"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::models::evaluation::ScoreVisibility;
    use crate::models::problem::{
        DatasetsSpec, LimitsSpec, MetricDirection, MetricSchemaSpec, MetricVisibility,
        ProblemBundleSpec, ScorerSpec, SubmissionSpec,
    };

    use super::{validate_problem_bundle, validate_problem_bundle_spec};

    fn base_spec() -> ProblemBundleSpec {
        ProblemBundleSpec {
            schema_version: 1,
            problem_id: "sample-sum".to_string(),
            problem_title: "Sample Sum".to_string(),
            problem_version: "v1".to_string(),
            submission: SubmissionSpec {
                format: "python_zip_project".to_string(),
                language: "python".to_string(),
                entrypoint: "main.py".to_string(),
            },
            scorer: ScorerSpec {
                entrypoint: "scorer/run.py".to_string(),
                result_file: "result.json".to_string(),
            },
            limits: LimitsSpec {
                time_limit_sec: 2.0,
                memory_limit_mb: 128,
            },
            datasets: DatasetsSpec {
                shown_dir: "shown".to_string(),
                hidden_dir: "hidden".to_string(),
                heldout_dir: Some("heldout".to_string()),
                shown_policy: ScoreVisibility::Full,
                hidden_policy: "score_only".to_string(),
                validation_enabled: true,
                heldout_enabled: true,
            },
            metric_schema: MetricSchemaSpec::default(),
        }
    }

    #[test]
    fn missing_validation_enabled_defaults_to_false() {
        let spec: ProblemBundleSpec = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "problem_id": "sample-sum",
            "problem_title": "Sample Sum",
            "problem_version": "v1",
            "submission": {
                "format": "python_zip_project",
                "language": "python",
                "entrypoint": "main.py"
            },
            "scorer": {
                "entrypoint": "scorer/run.py",
                "result_file": "result.json"
            },
            "limits": {
                "time_limit_sec": 2.0,
                "memory_limit_mb": 128
            },
            "datasets": {
                "shown_dir": "shown",
                "hidden_dir": "hidden",
                "shown_policy": "full",
                "hidden_policy": "score_only",
                "heldout_enabled": false
            }
        }))
        .expect("legacy spec should deserialize");

        assert!(!spec.datasets.validation_enabled);
        assert_eq!(spec.metric_schema.ranking.primary_metric_id, "score");
    }

    #[test]
    fn disabled_heldout_may_still_declare_heldout_dir() {
        let mut spec = base_spec();
        spec.datasets.heldout_enabled = false;
        spec.datasets.heldout_dir = Some("heldout".to_string());

        assert!(validate_problem_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn enabled_heldout_requires_heldout_dir() {
        let mut spec = base_spec();
        spec.datasets.heldout_enabled = true;
        spec.datasets.heldout_dir = None;

        assert!(validate_problem_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_rejects_unknown_primary_metric() {
        let mut spec = base_spec();
        spec.metric_schema.ranking.primary_metric_id = "missing".to_string();

        assert!(validate_problem_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_rejects_duplicate_metric_ids() {
        let mut spec = base_spec();
        let mut duplicate = spec.metric_schema.metrics[0].clone();
        duplicate.label = "Duplicate Score".to_string();
        spec.metric_schema.metrics.push(duplicate);

        assert!(validate_problem_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_accepts_tie_breaker_metadata() {
        let mut spec = base_spec();
        spec.metric_schema
            .metrics
            .push(crate::models::problem::MetricDefinitionSpec {
                id: "runtime_ms".to_string(),
                label: "Runtime".to_string(),
                unit: Some("ms".to_string()),
                direction: MetricDirection::Minimize,
                visibility: MetricVisibility::Public,
                description: Some("Wall-clock runtime in milliseconds.".to_string()),
            });
        spec.metric_schema
            .ranking
            .tie_breaker_metric_ids
            .push("runtime_ms".to_string());

        assert!(validate_problem_bundle_spec(&spec).is_ok());
    }

    fn create_bundle(root: &Path, spec: &ProblemBundleSpec) {
        std::fs::create_dir_all(root.join("scorer")).expect("failed to create scorer dir");
        std::fs::create_dir_all(root.join("shown")).expect("failed to create shown dir");
        std::fs::create_dir_all(root.join("hidden")).expect("failed to create hidden dir");
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

    #[tokio::test]
    async fn disabled_heldout_bundle_does_not_require_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-disabled-heldout-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.heldout_enabled = false;
        spec.datasets.heldout_dir = Some("heldout".to_string());
        create_bundle(&root, &spec);

        let result = validate_problem_bundle(&root).await;
        let _ = std::fs::remove_dir_all(root);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn enabled_heldout_bundle_requires_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-enabled-heldout-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.heldout_enabled = true;
        spec.datasets.heldout_dir = Some("heldout".to_string());
        create_bundle(&root, &spec);

        let result = validate_problem_bundle(&root).await;
        let _ = std::fs::remove_dir_all(root);

        assert!(result.is_err());
    }
}
