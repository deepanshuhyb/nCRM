//! `/broadcasts`: one WhatsApp message to many people.
//!
//! Sending costs money and can't be undone, so there are two steps on
//! purpose: `preview` says exactly who would get it, and `send` does it.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::broadcasts::{self, Audience, Broadcast, NewBroadcast, Preview};
use ncrm_core::db::TenantTx;

use crate::error::ApiError;
use crate::extract::{Body, Staff};
use crate::inbox::send_soon;
use crate::AppState;

/// `GET /broadcasts`: the latest ones, with how their copies have fared.
pub async fn list(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<Vec<Broadcast>>, ApiError> {
    who.require(Capability::ManageMessaging)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let list = broadcasts::list(&mut tx, who.actor()).await?;
    tx.commit().await?;
    Ok(Json(list))
}

/// `POST /broadcasts/preview`: who an audience is. Sends nothing.
pub async fn preview(
    State(state): State<AppState>,
    who: Staff,
    Body(audience): Body<Audience>,
) -> Result<Json<Preview>, ApiError> {
    who.require(Capability::ManageMessaging)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let preview = broadcasts::preview(&mut tx, who.actor(), &audience).await?;
    tx.commit().await?;
    Ok(Json(preview))
}

/// `POST /broadcasts`: `{audience, text}`. Queues one message per person
/// and starts sending.
pub async fn send(
    State(state): State<AppState>,
    who: Staff,
    Body(new): Body<NewBroadcast>,
) -> Result<(StatusCode, Json<Broadcast>), ApiError> {
    who.require(Capability::ManageMessaging)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let sent = broadcasts::send(&mut tx, who.actor(), new, &state.templates).await?;
    tx.commit().await?;
    send_soon(&state, who.0.org_id);
    Ok((StatusCode::CREATED, Json(sent)))
}
