use std::path::PathBuf;

use agentics_domain::models::evaluation::{EvaluatorRunResult, MetricValue};
use agentics_domain::models::names::{ChallengeName, TargetName};
use anyhow::Result;
use serde::Serialize;

use crate::cli::OutputFormat;

use super::format::{
    format_optional_metric, format_score, pretty_json, render_table, status_label,
};

#[derive(Debug, Clone, Serialize)]
/// Carries local validation package report data across this module boundary.
pub(crate) struct LocalValidationPackageReport {
    pub workspace_dir: PathBuf,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
    pub zip_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
/// Carries local validation target report data across this module boundary.
pub(crate) struct LocalValidationTargetReport {
    pub target: TargetName,
    pub log_path: PathBuf,
    pub primary_metric: Option<MetricValue>,
    pub result: EvaluatorRunResult,
}

#[derive(Debug, Clone, Serialize)]
/// Carries local validation report data across this module boundary.
pub(crate) struct LocalValidationReport {
    pub challenge_name: ChallengeName,
    pub bundle_dir: PathBuf,
    pub storage_root: PathBuf,
    pub package: LocalValidationPackageReport,
    pub targets: Vec<LocalValidationTargetReport>,
}

/// Renders local validation report for user-facing output.
pub(crate) fn render_local_validation_report(
    report: &LocalValidationReport,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(report),
        OutputFormat::Table => match report.targets.as_slice() {
            [target] => Ok(format!(
                "Local validation completed\nchallenge: {}\ntarget: {}\nstatus: {}\nprimary_metric: {}\nrank_score: {}\nlog: {}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}\nbundle: {}\nstorage: {}",
                report.challenge_name,
                target.target,
                status_label(&target.result.status),
                format_optional_metric(target.primary_metric.as_ref()),
                target
                    .result
                    .rank_score
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                target.log_path.display(),
                report.package.file_count,
                report.package.uncompressed_bytes,
                report.package.zip_bytes,
                report.package.workspace_dir.display(),
                report.bundle_dir.display(),
                report.storage_root.display()
            )),
            _ => {
                let rows = report
                    .targets
                    .iter()
                    .map(|target| {
                        vec![
                            target.target.to_string(),
                            status_label(&target.result.status),
                            format_optional_metric(target.primary_metric.as_ref()),
                            target
                                .result
                                .rank_score
                                .map(format_score)
                                .unwrap_or_else(|| "none".to_string()),
                            target.log_path.display().to_string(),
                        ]
                    })
                    .collect::<Vec<_>>();
                Ok(format!(
                    "Local validation completed\nchallenge: {}\n{}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}\nbundle: {}\nstorage: {}",
                    report.challenge_name,
                    render_table(
                        &["TARGET", "STATUS", "PRIMARY_METRIC", "RANK", "LOG"],
                        &rows
                    ),
                    report.package.file_count,
                    report.package.uncompressed_bytes,
                    report.package.zip_bytes,
                    report.package.workspace_dir.display(),
                    report.bundle_dir.display(),
                    report.storage_root.display()
                ))
            }
        },
    }
}
