//! HTTP API for ncrm.
//!
//! Layout:
//! - `extract`: who is calling and in which studio (cookies → session →
//!   staff context), plus JSON bodies with our error shape.
//! - `error`: the one JSON error format.
//! - `security`: origin check, per-IP rate limits, client address.
//! - `sign_in`: `/auth/*`.
//! - `branches`: the first studio-scoped resource, and the pattern the rest
//!   follow.
//! - `team`, `students`, `summary`: the team, students with contacts, and
//!   the home-screen counts.

pub mod branches;
pub mod classes;
pub mod enrollments;
pub mod error;
pub mod extract;
pub mod import;
pub mod plans;
pub mod security;
pub mod sign_in;
pub mod students;
pub mod summary;
pub mod team;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use ncrm_core::auth::{AuthConfig, CodeSender};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use security::{RateLimiter, SecurityConfig};

/// Largest request body accepted. Every request is small JSON.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// Longest a request may take before it is cut off.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: Arc<AuthConfig>,
    pub sender: CodeSender,
    /// Mark the session cookie Secure (HTTPS only) and give it the
    /// `__Host-` prefix. Off only for local development over plain http.
    pub cookie_secure: bool,
    pub security: Arc<SecurityConfig>,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(pool: PgPool, auth: AuthConfig, sender: CodeSender) -> Self {
        Self::with_security(pool, auth, sender, SecurityConfig::default())
    }

    pub fn with_security(
        pool: PgPool,
        auth: AuthConfig,
        sender: CodeSender,
        security: SecurityConfig,
    ) -> Self {
        Self {
            pool,
            auth: Arc::new(auth),
            sender,
            cookie_secure: true,
            limiter: Arc::new(RateLimiter::new(security.window)),
            security: Arc::new(security),
        }
    }

    /// The session cookie's name. Over HTTPS it carries the `__Host-`
    /// prefix, which browsers only accept from the exact host, over HTTPS,
    /// with `Path=/` and no `Domain`, so no subdomain can plant or
    /// overwrite it.
    pub fn session_cookie_name(&self) -> &'static str {
        if self.cookie_secure {
            "__Host-ncrm_session"
        } else {
            "ncrm_session"
        }
    }
}

/// Build the application's routes and protective layers.
pub fn router(state: AppState) -> Router {
    let code = post(sign_in::request_code).route_layer(middleware::from_fn_with_state(
        state.clone(),
        security::limit_code_requests,
    ));
    let verify = post(sign_in::verify).route_layer(middleware::from_fn_with_state(
        state.clone(),
        security::limit_verify_requests,
    ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/auth/code", code)
        .route("/auth/verify", verify)
        .route("/auth/me", get(sign_in::me))
        .route("/auth/studio", post(sign_in::select_studio))
        .route("/auth/sign-out", post(sign_in::sign_out))
        .route("/branches", get(branches::list).post(branches::create))
        .route("/summary", get(summary::get))
        .route("/staff", get(team::list).post(team::hire))
        .route("/staff/{id}", put(team::update))
        .route("/students", get(students::list).post(students::create))
        // Spreadsheets are bigger than any JSON body: these two routes
        // override the 64 KB limit set for everything below.
        .route(
            "/students/import/preview",
            post(import::preview).layer(DefaultBodyLimit::max(import::IMPORT_BODY_LIMIT)),
        )
        .route(
            "/students/import",
            post(import::commit).layer(DefaultBodyLimit::max(import::IMPORT_BODY_LIMIT)),
        )
        .route("/students/{id}", get(students::get).put(students::update))
        .route("/students/{id}/contacts", post(students::add_contact))
        .route(
            "/students/{id}/contacts/{contact_id}",
            delete(students::remove_contact),
        )
        .route(
            "/students/{id}/enrollments",
            get(enrollments::list).post(enrollments::enroll),
        )
        .route("/enrollments/{id}/end", post(enrollments::end))
        .route("/students/{id}/credits", get(enrollments::credits))
        .route("/students/{id}/credits/sell", post(enrollments::sell))
        .route("/students/{id}/credits/give", post(enrollments::give))
        .route(
            "/students/{id}/credits/take-back",
            post(enrollments::take_back),
        )
        .route("/rooms", get(classes::list_rooms))
        .route("/rooms/{id}", put(classes::update_room))
        .route("/branches/{id}/rooms", post(classes::create_room))
        .route("/batches", get(classes::list).post(classes::create))
        .route("/batches/{id}", get(classes::get).put(classes::update))
        .route("/plans", get(plans::list).post(plans::create))
        .route("/plans/{id}", put(plans::update))
        // Layers run outside-in for requests: the last one added runs first.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::origin_guard,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        // Every response is personal or operational: never cache it, never
        // sniff it, never leak the URL onward, never render it as a page.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-resource-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(TraceLayer::new_for_http())
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
