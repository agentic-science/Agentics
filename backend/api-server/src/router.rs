//! Axum route table for the API server.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{
        HeaderName, HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::{get, post},
};
use tower_http::cors::CorsLayer;

use crate::state::AppState;
use shared::config::Config;

const ZIP_SUBMISSION_JSON_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const PRIVATE_ASSET_JSON_OVERHEAD_BYTES: u64 = 1024 * 1024;
const X_AGENTICS_CSRF_TOKEN: HeaderName = HeaderName::from_static("x-agentics-csrf-token");

/// Build the application router with public, agent, admin, and health routes.
pub fn router(config: &Config) -> Router<AppState> {
    Router::new()
        // Health
        .route("/healthz", get(crate::handlers::healthz))
        // Registration (no auth)
        .route(
            "/api/agents/register",
            post(crate::handlers::register_agent),
        )
        // Agent routes (Bearer auth via extractor)
        .route(
            "/api/challenges",
            get(crate::handlers::list_agent_challenges),
        )
        .route(
            "/api/challenges/{name}",
            get(crate::handlers::get_agent_challenge),
        )
        .route(
            "/api/solution-submissions",
            post(crate::handlers::create_solution_submission)
                .layer(DefaultBodyLimit::max(ZIP_SUBMISSION_JSON_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/solution-submissions/{id}",
            get(crate::handlers::get_solution_submission),
        )
        .route(
            "/api/solution-submissions/{id}/result-report",
            get(crate::handlers::get_solution_submission_result_report),
        )
        .route(
            "/api/solution-submissions/{id}/ranking-context",
            get(crate::handlers::get_solution_submission_ranking_context),
        )
        .route(
            "/api/solution-submissions/{id}/logs",
            get(crate::handlers::get_solution_submission_logs),
        )
        .route(
            "/api/validation-runs",
            post(crate::handlers::create_validation_run)
                .layer(DefaultBodyLimit::max(ZIP_SUBMISSION_JSON_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/validation-runs/{id}",
            get(crate::handlers::get_validation_run),
        )
        .route(
            "/api/auth/github/login",
            get(crate::auth_handlers::github_oauth_login),
        )
        .route(
            "/api/auth/github/callback",
            get(crate::auth_handlers::github_oauth_callback),
        )
        .route(
            "/api/auth/admin/login",
            post(crate::auth_handlers::admin_login),
        )
        .route(
            "/api/auth/admin/logout",
            post(crate::auth_handlers::admin_logout),
        )
        .route("/api/creator/me", get(crate::auth_handlers::creator_me))
        .route(
            "/api/creator/challenge-drafts",
            post(crate::challenge_creation_handlers::create_challenge_draft),
        )
        .route(
            "/api/creator/challenge-drafts/{id}",
            get(crate::challenge_creation_handlers::get_challenge_draft),
        )
        .route(
            "/api/creator/challenge-drafts/{id}/private-assets",
            post(crate::challenge_creation_handlers::upload_challenge_private_asset)
                .layer(DefaultBodyLimit::max(private_asset_json_body_limit(config))),
        )
        .route(
            "/api/creator/challenges/{name}/stats",
            get(crate::handlers::get_creator_challenge_stats),
        )
        .route(
            "/api/creator/challenges/{name}/participants",
            get(crate::handlers::list_creator_challenge_participants),
        )
        .route(
            "/api/creator/challenges/{name}/shortlist-revisions",
            post(crate::handlers::create_challenge_shortlist_revision),
        )
        .route(
            "/api/creator/challenges/{name}/shortlist",
            get(crate::handlers::get_challenge_shortlist),
        )
        // Public routes
        .route(
            "/api/public/challenges",
            get(crate::handlers::list_challenges),
        )
        .route(
            "/api/public/challenges/{name}",
            get(crate::handlers::get_challenge),
        )
        .route(
            "/api/public/challenges/{name}/solution-submissions",
            get(crate::handlers::list_public_solution_submissions),
        )
        .route(
            "/api/public/challenges/{name}/leaderboard",
            get(crate::handlers::get_leaderboard),
        )
        .route(
            "/api/public/challenges/{name}/score-distributions",
            get(crate::handlers::get_score_distribution),
        )
        .route(
            "/api/public/solution-submissions/{id}",
            get(crate::handlers::get_public_solution_submission),
        )
        .route(
            "/api/public/solution-submissions/{id}/result-report",
            get(crate::handlers::get_public_solution_submission_result_report),
        )
        .route(
            "/api/public/solution-submissions/{id}/ranking-context",
            get(crate::handlers::get_public_solution_submission_ranking_context),
        )
        .route(
            "/api/public/solution-submissions/{id}/artifact",
            get(crate::handlers::get_public_artifact),
        )
        // Admin routes (Basic auth via extractor)
        .route(
            "/admin/challenges",
            get(crate::handlers::list_admin_challenges).post(crate::handlers::create_challenge),
        )
        .route(
            "/admin/challenges/{name}/publish",
            post(crate::handlers::publish_challenge),
        )
        .route(
            "/admin/challenge-drafts",
            get(crate::challenge_creation_handlers::list_admin_challenge_drafts),
        )
        .route(
            "/admin/challenge-drafts/cleanup",
            post(crate::challenge_creation_handlers::cleanup_challenge_drafts),
        )
        .route(
            "/admin/challenge-drafts/{id}/validate",
            post(crate::challenge_creation_handlers::validate_challenge_draft),
        )
        .route(
            "/admin/challenge-drafts/{id}/approve",
            post(crate::challenge_creation_handlers::approve_challenge_draft),
        )
        .route(
            "/admin/challenge-drafts/{id}/reject",
            post(crate::challenge_creation_handlers::reject_challenge_draft),
        )
        .route(
            "/admin/challenge-drafts/{id}/abandon",
            post(crate::challenge_creation_handlers::abandon_challenge_draft),
        )
        .route(
            "/admin/challenge-drafts/{id}/publish",
            post(crate::challenge_creation_handlers::publish_challenge_draft),
        )
        .route(
            "/admin/solution-submissions",
            get(crate::handlers::list_admin_solution_submissions),
        )
        .route(
            "/admin/service-heartbeats",
            get(crate::handlers::list_admin_service_heartbeats),
        )
        .route("/admin/capacity", get(crate::handlers::get_admin_capacity))
        .route(
            "/admin/pioneer-codes",
            get(crate::handlers::list_pioneer_codes).post(crate::handlers::create_pioneer_code),
        )
        .route(
            "/admin/pioneer-codes/{id}",
            get(crate::handlers::get_pioneer_code),
        )
        .route(
            "/admin/pioneer-codes/{id}/revoke",
            post(crate::handlers::revoke_pioneer_code),
        )
        .route(
            "/admin/solution-submissions/{id}/rejudge",
            post(crate::handlers::rejudge),
        )
        .route(
            "/admin/solution-submissions/{id}/official-run",
            post(crate::handlers::official_run),
        )
        .route(
            "/admin/solution-submissions/{id}/hide",
            post(crate::handlers::hide_solution_submission),
        )
        .route(
            "/admin/agents/{id}/disable",
            post(crate::handlers::disable_agent),
        )
        .layer(cors_layer(config))
}

/// Return a JSON body limit that can carry one configured private asset after base64 encoding.
fn private_asset_json_body_limit(config: &Config) -> usize {
    let decoded_limit = config.challenge_private_asset_bytes_per_draft;
    let base64_limit = decoded_limit
        .saturating_add(2)
        .div_ceil(3)
        .saturating_mul(4);
    let limit = base64_limit.saturating_add(PRIVATE_ASSET_JSON_OVERHEAD_BYTES);
    usize::try_from(limit).unwrap_or(usize::MAX)
}

/// Builds the CORS layer from configured browser origins.
fn cors_layer(config: &Config) -> CorsLayer {
    let origins = config
        .cors_allowed_origin_values()
        .into_iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE, X_AGENTICS_CSRF_TOKEN])
        .allow_credentials(true);

    if origins.is_empty() {
        layer
    } else {
        layer.allow_origin(origins)
    }
}
