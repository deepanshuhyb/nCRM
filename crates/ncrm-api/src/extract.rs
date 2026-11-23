//! Request extractors: who is calling, and inside which studio.
//!
//! Handlers declare what they need in their arguments:
//!
//! - `Body<T>`: a JSON body, with errors in our JSON error shape.
//! - `SignedIn`: any valid session, studio picked or not.
//! - `Staff`: a session with a studio picked and the person still working
//!   there. Carries the org id every studio-scoped query needs, and the role
//!   for `require`.

use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use ncrm_core::access::Capability;
use ncrm_core::auth::{self, Session, StaffContext};
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use crate::AppState;

/// A JSON request body. Malformed JSON becomes a 400 in our error shape.
pub struct Body<T>(pub T);

impl<T, S> FromRequest<S> for Body<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Body(value)),
            Err(rejection) if rejection.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE => {
                Err(ApiError::TooLarge)
            }
            Err(rejection) => Err(ApiError::Invalid(rejection.body_text())),
        }
    }
}

/// A valid session.
pub struct SignedIn {
    pub token: String,
    pub session: Session,
}

impl FromRequestParts<AppState> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(state.session_cookie_name())
            .map(|c| c.value().to_owned())
            .ok_or(ApiError::NotSignedIn)?;
        let session = auth::session_from_token(&state.pool, &token)
            .await?
            .ok_or(ApiError::NotSignedIn)?;
        Ok(SignedIn { token, session })
    }
}

/// A signed-in staff member inside a picked studio.
pub struct Staff(pub StaffContext);

impl Staff {
    /// Refuse with 403 unless the person's role allows `cap`.
    pub fn require(&self, cap: Capability) -> Result<(), ApiError> {
        if self.0.role.can(cap) {
            Ok(())
        } else {
            Err(ApiError::NotAllowed)
        }
    }
}

impl FromRequestParts<AppState> for Staff {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let signed_in = SignedIn::from_request_parts(parts, state).await?;
        if signed_in.session.org_id.is_none() {
            return Err(ApiError::ChooseStudio);
        }
        // None here means they were deactivated or their number changed
        // since signing in: treat it as signed out.
        let ctx = auth::staff_context(&state.pool, &signed_in.session)
            .await?
            .ok_or(ApiError::NotSignedIn)?;
        Ok(Staff(ctx))
    }
}
