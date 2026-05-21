use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::challenge::{MetricDirection, MetricSchemaSpec, MetricVisibility};
use super::ids::{EvaluationId, EvaluationJobId};
use super::names::{ChallengeName, MetricName, RunName, TargetName};
use super::paths::ManagedBundlePath;
use crate::storage::StorageKey;

/// Evaluation surface requested for a solution submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ScoringMode {
    /// Private validation scoring, backed by public challenge data.
    #[serde(rename = "validation")]
    Validation,
    /// Ranking-visible official scoring, backed by private benchmark data.
    #[serde(rename = "official")]
    Official,
}

impl ScoringMode {
    /// Canonical persisted and API value for this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Official => "official",
        }
    }

    /// Parse canonical persisted values.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "official" => Some(Self::Official),
            _ => None,
        }
    }

    /// Argument passed to the evaluator protocol.
    pub fn evaluator_mode_arg(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Official => "official",
        }
    }
}

impl fmt::Display for ScoringMode {
    /// Format the mode as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Controls how much per-case detail a dataset may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScoreVisibility {
    Full,
    ScoreOnly,
}

/// Per-case evaluator outcome for public validation tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorCaseStatus {
    Passed,
    Failed,
    Error,
}

/// Overall evaluator outcome emitted by `result.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorRunStatus {
    Passed,
    Failed,
    Error,
}

/// Persistent lifecycle state for an evaluation job/result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

impl EvaluationStatus {
    /// Stable database string for an evaluation lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for an evaluation lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for EvaluationStatus {
    /// Format the evaluation status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persistent lifecycle state for an evaluation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationJobStatus {
    Staged,
    Queued,
    Running,
    Completed,
    Failed,
}

impl EvaluationJobStatus {
    /// Stable database string for an evaluation job lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for an evaluation job lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "staged" => Some(Self::Staged),
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for EvaluationJobStatus {
    /// Format the job status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persistent lifecycle state for a solution submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolutionSubmissionStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
}

impl SolutionSubmissionStatus {
    /// Stable database string for a solution-submission lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for a solution-submission lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for SolutionSubmissionStatus {
    /// Format the submission status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Aggregate score summary for validation or official datasets.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreSummary {
    /// Normalized score in the inclusive range `[0, 1]`.
    pub score: f64,
    /// Number of passed cases in the aggregate.
    pub passed: i64,
    /// Total number of cases in the aggregate.
    pub total: i64,
}

/// Public per-case result exposed for validation feedback.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicCaseResult {
    pub case_name: String,
    pub status: EvaluatorCaseStatus,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Numeric value for one declared metric.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricValue {
    pub metric_name: MetricName,
    pub value: f64,
}

/// Metric values for one evaluator-defined run, case, seed, shard, or scenario.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMetricResult {
    pub run_name: RunName,
    #[serde(default)]
    #[schemars(required)]
    pub metrics: Vec<MetricValue>,
}

/// API DTO for a persisted evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationDto {
    pub id: EvaluationId,
    pub target: TargetName,
    pub status: EvaluationStatus,
    pub eval_type: ScoringMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_score: Option<f64>,
    pub aggregate_metrics: Vec<MetricValue>,
    pub run_metrics: Vec<RunMetricResult>,
    pub public_results: Vec<PublicCaseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_key: Option<StorageKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// Raw evaluator output read from a runner container's `result.json`.
///
/// Optional fields match the relaxed JSON contract used by the rewrite:
/// absent nullable fields are accepted, but numeric scores and mode-specific
/// summaries are validated before the result is persisted.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluatorRunResult {
    pub status: EvaluatorRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScoringMode>,
    pub primary_score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank_score: Option<f64>,
    #[serde(default)]
    pub aggregate_metrics: Vec<MetricValue>,
    #[serde(default)]
    pub run_metrics: Vec<RunMetricResult>,
    #[serde(default)]
    pub public_results: Vec<PublicCaseResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_summary: Option<ScoreSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_summary: Option<ScoreSummary>,
    #[serde(default)]
    pub logs: Vec<String>,
}

impl ScoreSummary {
    /// Validate score bounds and aggregate case counts for a named summary field.
    pub fn validate(&self, label: &str) -> Result<(), String> {
        validate_score(self.score, &format!("{label}.score"))?;
        if self.passed < 0 {
            return Err(format!("{label}.passed must be >= 0"));
        }
        if self.total < 0 {
            return Err(format!("{label}.total must be >= 0"));
        }
        if self.passed > self.total {
            return Err(format!("{label}.passed cannot be greater than total"));
        }

        Ok(())
    }
}

impl PublicCaseResult {
    /// Validate the public case name and normalized score.
    pub fn validate(&self) -> Result<(), String> {
        if self.case_name.trim().is_empty() {
            return Err("public_results.case_name must not be empty".to_string());
        }
        validate_score(self.score, "public_results.score")
    }
}

impl MetricValue {
    /// Validate metric name shape and finite numeric value.
    pub fn validate(&self, field: &str) -> Result<(), String> {
        validate_finite_number(self.value, &format!("{field}.value"))
    }
}

impl RunMetricResult {
    /// Validate one per-run metric record without checking challenge-specific names.
    pub fn validate(&self) -> Result<(), String> {
        let mut metric_names = HashSet::with_capacity(self.metrics.len());
        for metric in &self.metrics {
            metric.validate("run_metrics.metrics")?;
            if !metric_names.insert(metric.metric_name.as_str()) {
                return Err(format!(
                    "run_metrics.metrics contains duplicate metric_name `{}` for run `{}`",
                    metric.metric_name, self.run_name
                ));
            }
        }

        Ok(())
    }
}

impl EvaluatorRunResult {
    /// Validate platform-owned size limits before result persistence.
    pub fn validate_size_limits(
        &self,
        max_public_results: u64,
        max_result_log_bytes: u64,
    ) -> Result<(), String> {
        let public_result_count = u64::try_from(self.public_results.len())
            .map_err(|_| "public_results count exceeds supported range".to_string())?;
        if public_result_count > max_public_results {
            return Err(format!(
                "public_results contains too many entries: {public_result_count} > {max_public_results}"
            ));
        }

        let mut log_bytes = 0u64;
        for log in &self.logs {
            let len = u64::try_from(log.len())
                .map_err(|_| "result.logs byte length exceeds supported range".to_string())?;
            log_bytes = log_bytes
                .checked_add(len)
                .ok_or_else(|| "result.logs byte length overflow".to_string())?;
            if log_bytes > max_result_log_bytes {
                return Err(format!(
                    "result.logs exceeds byte limit: {log_bytes} > {max_result_log_bytes} bytes"
                ));
            }
        }

        Ok(())
    }

    /// Validate evaluator output against the evaluation mode that was actually run.
    ///
    /// If the evaluator included a `mode`, it must match `mode`; older evaluator
    /// outputs may omit it and will be normalized by the runner after this
    /// validation succeeds.
    pub fn validate_for_mode(&self, mode: ScoringMode) -> Result<(), String> {
        if let Some(result_mode) = self.mode
            && result_mode != mode
        {
            return Err("result mode does not match evaluation job type".to_string());
        }

        validate_score(self.primary_score, "primary_score")?;
        if let Some(rank_score) = self.rank_score {
            validate_finite_number(rank_score, "rank_score")?;
        }

        validate_metric_values(&self.aggregate_metrics, "aggregate_metrics")?;

        let mut run_names = HashSet::with_capacity(self.run_metrics.len());
        for run in &self.run_metrics {
            run.validate()?;
            if !run_names.insert(run.run_name.as_str()) {
                return Err(format!(
                    "run_metrics contains duplicate run_name `{}`",
                    run.run_name
                ));
            }
        }

        for public_result in &self.public_results {
            public_result.validate()?;
        }

        if let Some(validation) = &self.validation_summary {
            validation.validate("validation_summary")?;
        }
        if let Some(official) = &self.official_summary {
            official.validate("official_summary")?;
        }

        if self.validation_summary.is_none() && self.official_summary.is_none() {
            return Err(
                "validation_summary and official_summary cannot both be absent".to_string(),
            );
        }
        if mode == ScoringMode::Validation && self.validation_summary.is_none() {
            return Err("validation evaluation requires validation_summary".to_string());
        }
        if mode == ScoringMode::Official && self.official_summary.is_none() {
            return Err("official evaluation requires official_summary".to_string());
        }

        Ok(())
    }

    /// Fill legacy evaluator output into the structured metric fields.
    ///
    /// Older evaluators only emit `primary_score`; that value becomes the default
    /// `score` aggregate metric and rank score so clients can rely on one
    /// metric shape for both old and new bundles.
    pub fn normalize_metrics(
        &mut self,
        schema: &MetricSchemaSpec,
        mode: ScoringMode,
    ) -> Result<(), String> {
        self.validate_for_metric_schema(schema, mode)?;

        if self.aggregate_metrics.is_empty() {
            if schema.ranking.primary_metric_name.as_str() != "score" {
                if mode == ScoringMode::Validation {
                    return Ok(());
                }
                return Err(format!(
                    "aggregate_metrics is required when primary metric is `{}`",
                    schema.ranking.primary_metric_name
                ));
            }
            self.aggregate_metrics.push(MetricValue {
                metric_name: schema.ranking.primary_metric_name.clone(),
                value: self.primary_score,
            });
        }

        if self.rank_score.is_none() {
            let primary_metric = schema
                .primary_metric()
                .ok_or_else(|| "metric schema primary metric is missing".to_string())?;
            let Some(primary_value) =
                self.aggregate_metric_value(&schema.ranking.primary_metric_name)
            else {
                if mode == ScoringMode::Validation {
                    return Ok(());
                }
                return Err(format!(
                    "aggregate_metrics missing primary metric `{}`",
                    schema.ranking.primary_metric_name
                ));
            };
            self.rank_score = Some(match primary_metric.direction {
                MetricDirection::Maximize => primary_value,
                MetricDirection::Minimize => -primary_value,
            });
        }

        Ok(())
    }

    /// Validate metric names against the challenge's declared metric schema.
    pub fn validate_for_metric_schema(
        &self,
        schema: &MetricSchemaSpec,
        mode: ScoringMode,
    ) -> Result<(), String> {
        let declared = schema
            .metrics
            .iter()
            .map(|metric| (metric.name.as_str(), metric))
            .collect::<std::collections::HashMap<_, _>>();
        if declared.is_empty() {
            return Err("metric schema must declare at least one metric".to_string());
        }

        for metric in &self.aggregate_metrics {
            let Some(definition) = declared.get(metric.metric_name.as_str()) else {
                return Err(format!(
                    "aggregate_metrics references unknown metric `{}`",
                    metric.metric_name
                ));
            };
            validate_metric_visibility(mode, definition.visibility, &metric.metric_name)?;
        }

        for run in &self.run_metrics {
            for metric in &run.metrics {
                let Some(definition) = declared.get(metric.metric_name.as_str()) else {
                    return Err(format!(
                        "run_metrics references unknown metric `{}`",
                        metric.metric_name
                    ));
                };
                validate_metric_visibility(mode, definition.visibility, &metric.metric_name)?;
            }
        }

        if mode == ScoringMode::Official
            && !self.aggregate_metrics.is_empty()
            && !self
                .aggregate_metrics
                .iter()
                .any(|metric| metric.metric_name == schema.ranking.primary_metric_name)
        {
            return Err(format!(
                "aggregate_metrics missing primary metric `{}`",
                schema.ranking.primary_metric_name
            ));
        }

        Ok(())
    }

    /// Handles aggregate metric value for this module.
    fn aggregate_metric_value(&self, metric_name: &MetricName) -> Option<f64> {
        self.aggregate_metrics
            .iter()
            .find(|metric| &metric.metric_name == metric_name)
            .map(|metric| metric.value)
    }
}

/// Validates score invariants for this contract.
fn validate_score(value: f64, field: &str) -> Result<(), String> {
    validate_finite_number(value, field)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{field} must be a finite number in [0, 1]"));
    }

    Ok(())
}

/// Validates finite number invariants for this contract.
fn validate_finite_number(value: f64, field: &str) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{field} must be finite"));
    }

    Ok(())
}

/// Validates metric values invariants for this contract.
fn validate_metric_values(metrics: &[MetricValue], field: &str) -> Result<(), String> {
    let mut metric_names = HashSet::with_capacity(metrics.len());
    for metric in metrics {
        metric.validate(field)?;
        if !metric_names.insert(metric.metric_name.as_str()) {
            return Err(format!(
                "{field} contains duplicate metric_name `{}`",
                metric.metric_name
            ));
        }
    }

    Ok(())
}

/// Validates metric visibility invariants for this contract.
fn validate_metric_visibility(
    mode: ScoringMode,
    visibility: MetricVisibility,
    metric_name: &MetricName,
) -> Result<(), String> {
    if mode == ScoringMode::Validation && visibility == MetricVisibility::Official {
        return Err(format!(
            "validation results cannot include official-only metric `{metric_name}`"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::models::challenge::{
        MetricDefinitionSpec, MetricDirection, MetricSchemaSpec, MetricVisibility, RankingSpec,
    };
    use crate::models::names::{MetricName, RunName};

    use super::{
        EvaluatorCaseStatus, EvaluatorRunResult, EvaluatorRunStatus, MetricValue, RunMetricResult,
        ScoreSummary, ScoringMode,
    };

    /// Handles metric name for this module.
    fn metric_name(value: &str) -> MetricName {
        MetricName::try_new(value.to_string()).expect("test metric name is valid")
    }

    /// Handles run name for this module.
    fn run_name(value: &str) -> RunName {
        RunName::try_new(value.to_string()).expect("test run name is valid")
    }

    /// Handles valid validation result for this module.
    fn valid_validation_result() -> EvaluatorRunResult {
        EvaluatorRunResult {
            status: EvaluatorRunStatus::Passed,
            mode: Some(ScoringMode::Validation),
            primary_score: 1.0,
            rank_score: None,
            aggregate_metrics: vec![],
            run_metrics: vec![],
            public_results: vec![],
            validation_summary: Some(ScoreSummary {
                score: 1.0,
                passed: 1,
                total: 1,
            }),
            official_summary: None,
            logs: vec![],
        }
    }

    /// Verifies that evaluator mode mismatch is rejected.
    #[test]
    fn evaluator_mode_mismatch_is_rejected() {
        let mut result = valid_validation_result();
        result.mode = Some(ScoringMode::Official);
        result.official_summary = Some(ScoreSummary {
            score: 1.0,
            passed: 1,
            total: 1,
        });

        assert!(result.validate_for_mode(ScoringMode::Validation).is_err());
    }

    /// Verifies that evaluator mode can be absent.
    #[test]
    fn evaluator_mode_can_be_absent() {
        let mut result = valid_validation_result();
        result.mode = None;

        assert!(result.validate_for_mode(ScoringMode::Validation).is_ok());
    }

    /// Verifies that minimal metric output normalizes to primary score.
    #[test]
    fn minimal_metric_output_normalizes_to_primary_score() {
        let mut result = valid_validation_result();
        result
            .normalize_metrics(&MetricSchemaSpec::default(), ScoringMode::Validation)
            .unwrap();

        assert_eq!(result.rank_score, Some(1.0));
        assert_eq!(result.aggregate_metrics.len(), 1);
        assert_eq!(result.aggregate_metrics[0].metric_name.as_str(), "score");
        assert_eq!(result.aggregate_metrics[0].value, 1.0);
    }

    /// Verifies that missing rank score derives from minimized primary metric.
    #[test]
    fn missing_rank_score_derives_from_minimized_primary_metric() {
        let schema = MetricSchemaSpec {
            metrics: vec![MetricDefinitionSpec {
                name: metric_name("latency_ms"),
                label: "Latency".to_string(),
                unit: Some("ms".to_string()),
                direction: MetricDirection::Minimize,
                visibility: MetricVisibility::Public,
                metric_description: None,
            }],
            ranking: RankingSpec {
                primary_metric_name: metric_name("latency_ms"),
                tie_breaker_metric_names: vec![],
            },
        };
        let mut result = valid_validation_result();
        result.aggregate_metrics = vec![MetricValue {
            metric_name: metric_name("latency_ms"),
            value: 42.0,
        }];

        result
            .normalize_metrics(&schema, ScoringMode::Validation)
            .unwrap();

        assert_eq!(result.rank_score, Some(-42.0));
    }

    /// Verifies that unknown aggregate metric is rejected.
    #[test]
    fn unknown_aggregate_metric_is_rejected() {
        let mut result = valid_validation_result();
        result.aggregate_metrics = vec![MetricValue {
            metric_name: metric_name("unknown"),
            value: 1.0,
        }];

        assert!(
            result
                .normalize_metrics(&MetricSchemaSpec::default(), ScoringMode::Validation)
                .is_err()
        );
    }

    /// Verifies that non finite metric value is rejected.
    #[test]
    fn non_finite_metric_value_is_rejected() {
        let mut result = valid_validation_result();
        result.aggregate_metrics = vec![MetricValue {
            metric_name: metric_name("score"),
            value: f64::NAN,
        }];

        assert!(result.validate_for_mode(ScoringMode::Validation).is_err());
    }

    /// Verifies that per run metrics are validated.
    #[test]
    fn per_run_metrics_are_validated() {
        let mut result = valid_validation_result();
        result.aggregate_metrics = vec![MetricValue {
            metric_name: metric_name("score"),
            value: 1.0,
        }];
        result.run_metrics = vec![RunMetricResult {
            run_name: run_name("case-1"),
            metrics: vec![MetricValue {
                metric_name: metric_name("score"),
                value: 1.0,
            }],
        }];

        assert!(
            result
                .normalize_metrics(&MetricSchemaSpec::default(), ScoringMode::Validation)
                .is_ok()
        );
    }

    /// Verifies that validation result rejects official only metrics.
    #[test]
    fn validation_result_rejects_official_only_metrics() {
        let schema = MetricSchemaSpec {
            metrics: vec![MetricDefinitionSpec {
                name: metric_name("private_quality"),
                label: "Private Quality".to_string(),
                unit: None,
                direction: MetricDirection::Maximize,
                visibility: MetricVisibility::Official,
                metric_description: None,
            }],
            ranking: RankingSpec {
                primary_metric_name: metric_name("private_quality"),
                tie_breaker_metric_names: vec![],
            },
        };
        let mut result = valid_validation_result();
        result.aggregate_metrics = vec![MetricValue {
            metric_name: metric_name("private_quality"),
            value: 0.9,
        }];

        assert!(
            result
                .normalize_metrics(&schema, ScoringMode::Validation)
                .is_err()
        );
    }

    /// Verifies platform size limits reject result payload expansion.
    #[test]
    fn evaluator_result_size_limits_are_enforced() {
        let mut result = valid_validation_result();
        result.public_results = vec![
            super::PublicCaseResult {
                case_name: "case-1".to_string(),
                status: EvaluatorCaseStatus::Passed,
                score: 1.0,
                message: None,
            },
            super::PublicCaseResult {
                case_name: "case-2".to_string(),
                status: EvaluatorCaseStatus::Passed,
                score: 1.0,
                message: None,
            },
        ];

        let public_result_error = result
            .validate_size_limits(1, 1024)
            .expect_err("public result count should be capped");
        assert!(public_result_error.contains("public_results"));

        let mut result = valid_validation_result();
        result.logs = vec!["abcd".to_string(), "efgh".to_string()];

        let log_error = result
            .validate_size_limits(1024, 7)
            .expect_err("embedded result logs should be capped");
        assert!(log_error.contains("result.logs"));
    }
}

/// Minimal job DTO returned when a solution submission queues an evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationJobDto {
    pub id: EvaluationJobId,
    pub target: TargetName,
    pub status: EvaluationJobStatus,
}

/// Runner payload persisted on an evaluation job.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationJobPayload {
    pub artifact_key: StorageKey,
    pub bundle_path: ManagedBundlePath,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
}
