mod auth;
mod challenges;
mod drafts;
mod format;
mod submissions;
mod validation;
mod workspace;

pub(crate) use auth::{render_auth_status, render_config_set, render_register_agent};
pub(crate) use challenges::{
    render_challenge_detail, render_challenge_list, render_challenge_stats, render_leaderboard,
    render_score_distribution,
};
pub(crate) use drafts::{render_challenge_draft, render_challenge_draft_cleanup};
pub(crate) use submissions::{
    render_create_solution_submission_batch, render_create_validation_run_batch,
    render_public_solution_submission_list, render_ranking_context,
    render_solution_submission_logs, render_solution_submission_report,
    render_solution_submission_status, render_validation_run_status_batch,
};
pub(crate) use validation::{
    LocalValidationPackageReport, LocalValidationReport, LocalValidationTargetReport,
    render_local_validation_report,
};
pub(crate) use workspace::render_init_solution;

pub(crate) use crate::cli::OutputFormat;

#[cfg(test)]
mod tests;
