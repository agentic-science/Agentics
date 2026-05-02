//! Axum route table for the API server.

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::cors::CorsLayer;

use crate::state::AppState;

/// Build the application router with public, agent, admin, and health routes.
pub fn router() -> Router<AppState> {
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
            post(crate::handlers::create_solution_submission),
        )
        .route(
            "/api/solution-submissions/{id}",
            get(crate::handlers::get_solution_submission),
        )
        .route(
            "/api/validation-runs",
            post(crate::handlers::create_validation_run),
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
        .layer(CorsLayer::permissive())
}
