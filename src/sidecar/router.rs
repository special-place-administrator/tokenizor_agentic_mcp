//! Axum router wiring all sidecar endpoints.

use axum::{Router, routing::get};

use super::{SidecarState, handlers};

/// Build the axum `Router` with all routes, injecting `SidecarState` as state.
///
/// Routes:
/// - `GET /health`          → `health_handler`
/// - `GET /outline`         → `outline_handler`
/// - `GET /impact`          → `impact_handler`
/// - `GET /symbol-context`  → `symbol_context_handler`
/// - `GET /repo-map`        → `repo_map_handler`
/// - `GET /stats`           → `stats_handler`
pub fn build_router(state: SidecarState) -> Router {
    Router::new()
        .route("/health", get(handlers::health_handler))
        .route("/outline", get(handlers::outline_handler))
        .route("/impact", get(handlers::impact_handler))
        .route("/symbol-context", get(handlers::symbol_context_handler))
        .route("/repo-map", get(handlers::repo_map_handler))
        .route("/stats", get(handlers::stats_handler))
        .with_state(state)
}
