use serde::{Deserialize, Serialize};

/// Evaluation surface requested for a submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMode {
    /// Public scoring, visible to agents and backed by shown plus hidden data.
    Public,
    /// Official/admin scoring, backed by heldout data.
    Official,
}

/// Controls how much per-case detail a dataset may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreVisibility {
    Full,
    ScoreOnly,
}

/// Per-case scorer outcome for shown tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerCaseStatus {
    Passed,
    Failed,
    Error,
}

/// Overall scorer outcome emitted by `result.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerRunStatus {
    Passed,
    Failed,
    Error,
}

/// Persistent lifecycle state for an evaluation job/result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// Aggregate score summary for hidden or official datasets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSummary {
    /// Normalized score in the inclusive range `[0, 1]`.
    pub score: f64,
    /// Number of passed cases in the aggregate.
    pub passed: i64,
    /// Total number of cases in the aggregate.
    pub total: i64,
}

/// Public per-case result for shown examples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShownCaseResult {
    pub case_id: String,
    pub status: ScorerCaseStatus,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// API DTO for a persisted evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationDto {
    pub id: String,
    pub status: EvaluationStatus,
    pub eval_type: ScoringMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_score: Option<f64>,
    pub shown_results: Vec<ShownCaseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// Raw scorer output read from a runner container's `result.json`.
///
/// Optional fields match the relaxed JSON contract used by the rewrite:
/// absent nullable fields are accepted, but numeric scores and mode-specific
/// summaries are validated before the result is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerRunResult {
    pub status: ScorerRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScoringMode>,
    pub primary_score: f64,
    #[serde(default)]
    pub shown_results: Vec<ShownCaseResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_summary: Option<ScoreSummary>,
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

impl ShownCaseResult {
    /// Validate the shown-case id and normalized score.
    pub fn validate(&self) -> Result<(), String> {
        if self.case_id.trim().is_empty() {
            return Err("shown_results.case_id must not be empty".to_string());
        }
        validate_score(self.score, "shown_results.score")
    }
}

impl ScorerRunResult {
    /// Validate scorer output against the evaluation mode that was actually run.
    ///
    /// If the scorer included a `mode`, it must match `mode`; older scorer
    /// outputs may omit it and will be normalized by the runner after this
    /// validation succeeds.
    pub fn validate_for_mode(&self, mode: ScoringMode) -> Result<(), String> {
        if let Some(result_mode) = self.mode
            && result_mode != mode
        {
            return Err("result mode does not match evaluation job type".to_string());
        }

        validate_score(self.primary_score, "primary_score")?;

        for shown in &self.shown_results {
            shown.validate()?;
        }

        if let Some(hidden) = &self.hidden_summary {
            hidden.validate("hidden_summary")?;
        }
        if let Some(official) = &self.official_summary {
            official.validate("official_summary")?;
        }

        if self.hidden_summary.is_none() && self.official_summary.is_none() {
            return Err("hidden_summary and official_summary cannot both be absent".to_string());
        }
        if mode == ScoringMode::Public && self.hidden_summary.is_none() {
            return Err("public evaluation requires hidden_summary".to_string());
        }
        if mode == ScoringMode::Official && self.official_summary.is_none() {
            return Err("official evaluation requires official_summary".to_string());
        }

        Ok(())
    }
}

fn validate_score(value: f64, field: &str) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{field} must be a finite number in [0, 1]"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ScoreSummary, ScorerRunResult, ScorerRunStatus, ScoringMode};

    fn valid_public_result() -> ScorerRunResult {
        ScorerRunResult {
            status: ScorerRunStatus::Passed,
            mode: Some(ScoringMode::Public),
            primary_score: 1.0,
            shown_results: vec![],
            hidden_summary: Some(ScoreSummary {
                score: 1.0,
                passed: 1,
                total: 1,
            }),
            official_summary: None,
            logs: vec![],
        }
    }

    #[test]
    fn scorer_mode_mismatch_is_rejected() {
        let mut result = valid_public_result();
        result.mode = Some(ScoringMode::Official);
        result.official_summary = Some(ScoreSummary {
            score: 1.0,
            passed: 1,
            total: 1,
        });

        assert!(result.validate_for_mode(ScoringMode::Public).is_err());
    }

    #[test]
    fn scorer_mode_can_be_absent() {
        let mut result = valid_public_result();
        result.mode = None;

        assert!(result.validate_for_mode(ScoringMode::Public).is_ok());
    }
}

/// Minimal job DTO returned when a submission queues an evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationJobDto {
    pub id: String,
    pub status: EvaluationStatus,
}

/// Runner payload persisted on an evaluation job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationJobPayload {
    pub artifact_path: String,
    pub bundle_path: String,
    pub problem_id: String,
    pub problem_version_id: String,
}
