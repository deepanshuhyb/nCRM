//! One JSON error shape for every endpoint:
//!
//! ```json
//! { "error": { "code": "not_signed_in", "message": "Sign in to continue." } }
//! ```
//!
//! `code` is stable and meant for the dashboard to switch on; `message` is
//! plain English for showing to the person.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    /// No valid session cookie.
    NotSignedIn,
    /// Signed in, but the phone works at several studios and none is picked.
    ChooseStudio,
    /// Signed in, but the role does not allow this.
    NotAllowed,
    NotFound,
    Conflict(String),
    Invalid(String),
    InvalidCode,
    CodeLocked,
    RateLimited,
    /// A state-changing request from a page that isn't our dashboard.
    BadOrigin,
    /// The request body is over the size limit.
    TooLarge,
    /// Something broke on our side. The detail is logged, never sent.
    Internal(String),
}

impl ApiError {
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            ApiError::NotSignedIn => (
                StatusCode::UNAUTHORIZED,
                "not_signed_in",
                "Sign in to continue.".into(),
            ),
            ApiError::ChooseStudio => (
                StatusCode::CONFLICT,
                "choose_studio",
                "Choose which studio to open.".into(),
            ),
            ApiError::NotAllowed => (
                StatusCode::FORBIDDEN,
                "not_allowed",
                "Your role can't do this. Ask the studio owner.".into(),
            ),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "That doesn't exist, or it isn't in your studio.".into(),
            ),
            ApiError::Conflict(what) => (StatusCode::CONFLICT, "conflict", what.clone()),
            ApiError::Invalid(what) => (StatusCode::BAD_REQUEST, "invalid_input", what.clone()),
            ApiError::InvalidCode => (
                StatusCode::UNAUTHORIZED,
                "invalid_code",
                "That code is wrong or has expired.".into(),
            ),
            ApiError::CodeLocked => (
                StatusCode::TOO_MANY_REQUESTS,
                "code_locked",
                "Too many wrong attempts. Ask for a new code.".into(),
            ),
            ApiError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                "Too many requests. Try again in a little while.".into(),
            ),
            ApiError::BadOrigin => (
                StatusCode::FORBIDDEN,
                "bad_origin",
                "This request came from a page that isn't the ncrm dashboard.".into(),
            ),
            ApiError::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "That request is too large.".into(),
            ),
            ApiError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Something went wrong on our side. Try again.".into(),
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Internal(detail) = &self {
            tracing::error!(error = %detail, "request failed");
        }
        let (status, code, message) = self.parts();
        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

impl From<ncrm_core::Error> for ApiError {
    fn from(err: ncrm_core::Error) -> Self {
        use ncrm_core::Error as E;
        match err {
            E::NotFound => ApiError::NotFound,
            E::Forbidden => ApiError::NotAllowed,
            E::Conflict(what) => ApiError::Conflict(what),
            E::Invalid(what) => ApiError::Invalid(what),
            E::InvalidCode => ApiError::InvalidCode,
            E::CodeLocked => ApiError::CodeLocked,
            E::RateLimited => ApiError::RateLimited,
            E::Unavailable(what) => ApiError::Internal(what),
            E::Database(e) => ApiError::Internal(e.to_string()),
        }
    }
}
