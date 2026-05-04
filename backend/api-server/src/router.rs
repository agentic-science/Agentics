//! Axum route table for the API server.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{
        HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::{get, post},
};
use tower_http::cors::CorsLayer;

use crate::state::AppState;
use shared::config::Config;

const ZIP_SUBMISSION_JSON_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

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
            "/api/challenges/{id}",
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
            "/api/validation-runs",
            post(crate::handlers::create_validation_run)
                .layer(DefaultBodyLimit::max(ZIP_SUBMISSION_JSON_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/validation-runs/{id}",
            get(crate::handlers::get_validation_run),
        )
        .route(
            "/api/challenges/{id}/discussions",
            post(crate::handlers::create_thread),
        )
        .route(
            "/api/discussions/{id}/replies",
            post(crate::handlers::create_reply),
        )
        // Public routes
        .route(
            "/api/public/challenges",
            get(crate::handlers::list_challenges),
        )
        .route(
            "/api/public/challenges/{id}",
            get(crate::handlers::get_challenge),
        )
        .route(
            "/api/public/challenges/{id}/solution-submissions",
            get(crate::handlers::list_public_solution_submissions),
        )
        .route(
            "/api/public/challenges/{id}/leaderboard",
            get(crate::handlers::get_leaderboard),
        )
        .route(
            "/api/public/challenges/{id}/discussions",
            get(crate::handlers::list_discussions),
        )
        .route(
            "/api/public/solution-submissions/{id}",
            get(crate::handlers::get_public_solution_submission),
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
            "/admin/challenges/{id}/versions",
            post(crate::handlers::publish_version),
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

fn cors_layer(config: &Config) -> CorsLayer {
    let origins = config
        .cors_allowed_origin_values()
        .into_iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    if origins.is_empty() {
        layer
    } else {
        layer.allow_origin(origins)
    }
}
