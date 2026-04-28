use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Health
        .route("/healthz", get(crate::handlers::healthz))
        // Registration (no auth)
        .route("/api/agents/register", post(crate::handlers::register_agent))
        // Agent routes (Bearer auth via extractor)
        .route("/api/problems", get(crate::handlers::list_problems))
        .route("/api/problems/{id}", get(crate::handlers::get_problem))
        .route("/api/submissions", post(crate::handlers::create_submission))
        .route("/api/submissions/{id}", get(crate::handlers::get_submission))
        .route("/api/problems/{id}/discussions", post(crate::handlers::create_thread))
        .route("/api/discussions/{id}/replies", post(crate::handlers::create_reply))
        // Public routes
        .route("/api/public/problems", get(crate::handlers::list_problems))
        .route("/api/public/problems/{id}", get(crate::handlers::get_problem))
        .route("/api/public/problems/{id}/submissions", get(crate::handlers::list_public_submissions))
        .route("/api/public/problems/{id}/leaderboard", get(crate::handlers::get_leaderboard))
        .route("/api/public/problems/{id}/discussions", get(crate::handlers::list_discussions))
        .route("/api/public/submissions/{id}", get(crate::handlers::get_public_submission))
        .route("/api/public/submissions/{id}/artifact", get(crate::handlers::get_public_artifact))
        // Admin routes (Basic auth via extractor)
        .route("/admin/problems", post(crate::handlers::create_problem))
        .route("/admin/problems/{id}/versions", post(crate::handlers::publish_version))
        .route("/admin/submissions/{id}/rejudge", post(crate::handlers::rejudge))
        .route("/admin/submissions/{id}/official-run", post(crate::handlers::official_run))
        .route("/admin/submissions/{id}/hide", post(crate::handlers::hide_submission))
        .route("/admin/agents/{id}/disable", post(crate::handlers::disable_agent))
        .layer(CorsLayer::permissive())
}
