//! `/inbox/...` and `/messaging/settings`: the WhatsApp conversations
//! staff read and answer, and how the bot is set.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ncrm_core::access::Capability;
use ncrm_core::db::TenantTx;
use ncrm_core::messaging::{self, Conversation, Reply, Settings, SettingsEdit, Thread};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{parse_id, Body, Staff};
use crate::AppState;

/// Send a studio's queued messages now, without making the request wait.
/// The worker does the same on its own schedule, so a failure here only
/// delays a message; it is never lost.
pub fn send_soon(state: &AppState, org_id: Uuid) {
    let (pool, sender) = (state.pool.clone(), state.messages.clone());
    tokio::spawn(async move {
        match messaging::send_queued(&pool, org_id, &sender).await {
            Ok(sent) if sent.failed > 0 || sent.retrying > 0 => {
                tracing::warn!(%org_id, sent.failed, sent.retrying, "some WhatsApp messages did not go out")
            }
            Ok(_) => {}
            // Counts and the error only: never a number or a message.
            Err(error) => {
                tracing::warn!(%org_id, %error, "could not send queued WhatsApp messages")
            }
        }
    });
}

/// `GET /inbox`: conversations, the ones waiting for a person first.
pub async fn list(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<Vec<Conversation>>, ApiError> {
    who.require(Capability::UseInbox)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let inbox = messaging::inbox(&mut tx, who.actor()).await?;
    tx.commit().await?;
    Ok(Json(inbox))
}

/// `GET /inbox/{id}`: one conversation. Opening it marks it read.
pub async fn thread(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Thread>, ApiError> {
    who.require(Capability::UseInbox)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let thread = messaging::thread(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(thread))
}

/// `POST /inbox/{id}/reply`: answer as the studio. The bot then stays
/// out of this conversation for a while.
pub async fn reply(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
    Body(body): Body<Reply>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    who.require(Capability::UseInbox)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let thread = messaging::reply(&mut tx, who.actor(), id, body).await?;
    tx.commit().await?;
    send_soon(&state, who.0.org_id);
    Ok((StatusCode::CREATED, Json(thread)))
}

/// `POST /inbox/{id}/hand-back`: let the bot answer here again.
pub async fn hand_back(
    State(state): State<AppState>,
    who: Staff,
    Path(id): Path<String>,
) -> Result<Json<Thread>, ApiError> {
    who.require(Capability::UseInbox)?;
    let id = parse_id(&id)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let thread = messaging::hand_back(&mut tx, who.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(thread))
}

async fn read_settings(state: &AppState, who: &Staff) -> Result<Settings, ApiError> {
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    let settings = messaging::settings(
        &mut tx,
        who.actor(),
        state.messages.is_live(),
        state.ai.is_some(),
    )
    .await?;
    tx.commit().await?;
    Ok(settings)
}

/// `GET /messaging/settings`
pub async fn settings(
    State(state): State<AppState>,
    who: Staff,
) -> Result<Json<Settings>, ApiError> {
    who.require(Capability::UseInbox)?;
    Ok(Json(read_settings(&state, &who).await?))
}

/// `PUT /messaging/settings`: choose how the bot answers.
pub async fn set_settings(
    State(state): State<AppState>,
    who: Staff,
    Body(body): Body<SettingsEdit>,
) -> Result<Json<Settings>, ApiError> {
    who.require(Capability::ManageMessaging)?;
    let mut tx = TenantTx::begin(&state.pool, who.0.org_id).await?;
    messaging::set_bot_mode(&mut tx, who.actor(), body).await?;
    tx.commit().await?;
    Ok(Json(read_settings(&state, &who).await?))
}
