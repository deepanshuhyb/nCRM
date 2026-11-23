//! `/auth/*`: asking for a code, signing in, picking a studio, signing out.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use ncrm_core::access::Capability;
use ncrm_core::auth::{self, Membership, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{Body, SignedIn};
use crate::AppState;

#[derive(Deserialize)]
pub struct CodeRequest {
    pub phone: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub phone: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct StudioRequest {
    pub org_id: Uuid,
}

/// What the dashboard needs to know about the signed-in person.
#[derive(Serialize)]
pub struct Me {
    pub phone: String,
    /// The studio this session is in, or null if one must be picked.
    pub studio: Option<CurrentStudio>,
    /// Every studio this phone works at, for the studio switcher.
    pub memberships: Vec<Membership>,
}

#[derive(Serialize)]
pub struct CurrentStudio {
    pub org_id: Uuid,
    pub org_name: String,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub role: ncrm_core::staff::StaffRole,
    /// What the role may do, so the UI can hide what it can't.
    pub capabilities: Vec<Capability>,
}

/// `POST /auth/code` — always answers 202 the same way, whether or not the
/// number is staff, and delivers the code in the background so response
/// time does not give it away either.
pub async fn request_code(
    State(state): State<AppState>,
    Body(body): Body<CodeRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if let Some(pending) = auth::request_code(&state.pool, &state.auth, &body.phone).await? {
        let sender = state.sender.clone();
        tokio::spawn(async move {
            if let Err(e) = sender.send(pending).await {
                tracing::error!(error = %e, "could not deliver sign-in code");
            }
        });
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "If this number belongs to a studio, a code is on its way." })),
    ))
}

/// `POST /auth/verify` — checks the code, sets the session cookie.
pub async fn verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Body(body): Body<VerifyRequest>,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let signed_in = auth::verify_code(&state.pool, &state.auth, &body.phone, &body.code).await?;
    let cookie = session_cookie(&state, signed_in.token.clone());
    let me = describe(&state, &signed_in.session, signed_in.memberships).await?;
    Ok((jar.add(cookie), Json(me)))
}

/// `GET /auth/me`
pub async fn me(State(state): State<AppState>, who: SignedIn) -> Result<Json<Me>, ApiError> {
    let memberships = auth::memberships(&state.pool, &who.session.phone).await?;
    Ok(Json(describe(&state, &who.session, memberships).await?))
}

/// `POST /auth/studio` — pick or switch studio.
pub async fn select_studio(
    State(state): State<AppState>,
    who: SignedIn,
    Body(body): Body<StudioRequest>,
) -> Result<Json<Me>, ApiError> {
    let session = auth::select_studio(&state.pool, &who.session, body.org_id).await?;
    let memberships = auth::memberships(&state.pool, &session.phone).await?;
    Ok(Json(describe(&state, &session, memberships).await?))
}

/// `POST /auth/sign-out` — ends the session if there is one, and always
/// clears the cookie.
pub async fn sign_out(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, CookieJar), ApiError> {
    let jar = CookieJar::from_headers(&headers);
    let name = state.session_cookie_name();
    if let Some(cookie) = jar.get(name) {
        auth::sign_out(&state.pool, cookie.value()).await?;
    }
    let jar = jar.remove(
        Cookie::build(name)
            .path("/")
            .secure(state.cookie_secure)
            .http_only(true),
    );
    Ok((StatusCode::NO_CONTENT, jar))
}

async fn describe(
    state: &AppState,
    session: &Session,
    memberships: Vec<Membership>,
) -> Result<Me, ApiError> {
    let studio = auth::staff_context(&state.pool, session)
        .await?
        .map(|ctx| CurrentStudio {
            org_id: ctx.org_id,
            org_name: ctx.org_name,
            staff_id: ctx.staff_id,
            staff_name: ctx.staff_name,
            role: ctx.role,
            capabilities: ctx.role.capabilities(),
        });
    Ok(Me {
        phone: session.phone.clone(),
        studio,
        memberships,
    })
}

/// The session cookie: unreadable from JavaScript, only sent over HTTPS in
/// production (with the `__Host-` prefix), and not sent on cross-site POSTs.
/// Together with the origin check and JSON-only bodies this blocks CSRF.
fn session_cookie(state: &AppState, token: String) -> Cookie<'static> {
    Cookie::build((state.session_cookie_name(), token))
        .http_only(true)
        .secure(state.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(state.auth.session_ttl_secs))
        .build()
}
