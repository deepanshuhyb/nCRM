//! HTTP API for ncrm.
//!
//! Layout:
//! - `extract`: who is calling and in which studio (cookies → session →
//!   staff context), plus JSON bodies with our error shape.
//! - `error`: the one JSON error format.
//! - `sign_in`: `/auth/*`.
//! - `branches`: the first studio-scoped resource, and the pattern the rest
//!   follow.

pub mod branches;
pub mod error;
pub mod extract;
pub mod sign_in;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use ncrm_core::auth::{AuthConfig, CodeSender};
use serde_json::{json, Value};
use sqlx::PgPool;

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: Arc<AuthConfig>,
    pub sender: CodeSender,
    /// Mark the session cookie Secure (HTTPS only). Off only for local
    /// development over plain http.
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(pool: PgPool, auth: AuthConfig, sender: CodeSender) -> Self {
        Self {
            pool,
            auth: Arc::new(auth),
            sender,
            cookie_secure: true,
        }
    }
}

/// Build the application's routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/auth/code", post(sign_in::request_code))
        .route("/auth/verify", post(sign_in::verify))
        .route("/auth/me", get(sign_in::me))
        .route("/auth/studio", post(sign_in::select_studio))
        .route("/auth/sign-out", post(sign_in::sign_out))
        .route("/branches", get(branches::list).post(branches::create))
        .with_state(state)
}

/// Liveness and database check, for uptime monitors and deploys.
async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    if db_ok {
        (StatusCode::OK, Json(json!({ "status": "ok" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "database unavailable" })),
        )
    }
}
