use anyhow::Result;

use crate::cli::OutputFormat;
use crate::workspace::InitSolutionSummary;

use super::format::pretty_json;

/// Renders init solution for user-facing output.
pub(crate) fn render_init_solution(
    summary: &InitSolutionSummary,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(summary),
        OutputFormat::Table => Ok(format!(
            "Initialized solution workspace: {}\nchallenge_name: {}\nchallenge: {} ({})\nruntime_profile: {}\ninterface: {}",
            summary.workspace_dir.display(),
            summary.challenge_name,
            summary.challenge_title,
            summary.challenge_name,
            summary.runtime_profile,
            summary.interface
        )),
    }
}
