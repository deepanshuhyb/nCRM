//! `/webhooks/whatsapp`: where Meta delivers incoming messages and
//! delivery reports.
//!
//! This is the one address that takes requests from outside without a
//! session, so it trusts nothing but the signature:
//! - Every delivery must carry `X-Hub-Signature-256`, an HMAC of the exact
//!   body keyed with our Meta app's secret. No valid signature, no work.
//! - The body is read as raw bytes first (the signature is over bytes,
//!   not over re-serialized JSON), then parsed.
//! - Which studio a message is for comes from the number it was sent
//!   *to*, looked up in our own table. Nothing in the payload can name a
//!   studio.
//! - Message text and phone numbers are never logged.
//!
//! Meta retries a delivery until it gets a 200, so storing is idempotent
//! (`messaging::receive` ignores a message id it has seen) and anything
//! slow (the bot, sending) happens after we have answered.

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use ncrm_core::bot;
use ncrm_core::db::TenantTx;
use ncrm_core::messaging::{self, Incoming, MessageStatus};
use ncrm_core::whatsapp::{parse_webhook, WebhookEvent};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::inbox::send_soon;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct Verify {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

/// `GET /webhooks/whatsapp`: Meta's one-time check that the address is
/// ours. It sends the token we configured and a challenge to echo.
pub async fn verify(
    State(state): State<AppState>,
    Query(q): Query<Verify>,
) -> Result<String, StatusCode> {
    let hook = state.webhook.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    match (q.mode.as_deref(), q.token, q.challenge) {
        (Some("subscribe"), Some(token), Some(challenge))
            if hook.token_matches(&token) && challenge.len() <= 200 =>
        {
            Ok(challenge)
        }
        _ => Err(StatusCode::FORBIDDEN),
    }
}

/// `POST /webhooks/whatsapp`
pub async fn deliver(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let Some(hook) = state.webhook.as_ref() else {
        return Ok(StatusCode::NOT_FOUND);
    };
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !hook.signature_is_valid(&body, signature) {
        return Ok(StatusCode::UNAUTHORIZED);
    }
    // Signed but not JSON we understand: accept it, so Meta doesn't keep
    // retrying something we will never be able to read.
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return Ok(StatusCode::OK);
    };

    for event in parse_webhook(&json) {
        match event {
            WebhookEvent::Message {
                to_number_id,
                from,
                wa_message_id,
                text,
                profile_name,
            } => {
                let Some(org_id) = messaging::studio_for_number(&state.pool, &to_number_id).await?
                else {
                    tracing::warn!("a WhatsApp message arrived for a number no studio has");
                    continue;
                };
                let mut tx = TenantTx::begin(&state.pool, org_id).await?;
                let got = messaging::receive(
                    &mut tx,
                    &Incoming {
                        from,
                        wa_message_id,
                        text,
                        profile_name,
                    },
                )
                .await?;
                tx.commit().await?;
                if got.new {
                    answer_soon(&state, org_id, got.conversation_id);
                }
            }
            WebhookEvent::Status {
                to_number_id,
                wa_message_id,
                status,
                error_code,
            } => {
                let status = match status.as_str() {
                    "sent" => MessageStatus::Sent,
                    "delivered" => MessageStatus::Delivered,
                    "read" => MessageStatus::Read,
                    "failed" => MessageStatus::Failed,
                    _ => continue,
                };
                let Some(org_id) = messaging::studio_for_number(&state.pool, &to_number_id).await?
                else {
                    continue;
                };
                let mut tx = TenantTx::begin(&state.pool, org_id).await?;
                messaging::delivery_report(
                    &mut tx,
                    &wa_message_id,
                    status,
                    error_code.and_then(|c| i32::try_from(c).ok()),
                )
                .await?;
                tx.commit().await?;
            }
        }
    }
    Ok(StatusCode::OK)
}

/// Let the bot answer, then send what it wrote. Runs after the webhook
/// has replied to Meta.
fn answer_soon(state: &AppState, org_id: Uuid, conversation_id: Uuid) {
    let state = state.clone();
    tokio::spawn(async move {
        match bot::answer(&state.pool, org_id, conversation_id, state.ai.as_ref()).await {
            Ok(true) => send_soon(&state, org_id),
            Ok(false) => {}
            Err(error) => tracing::warn!(%org_id, %error, "the bot could not answer"),
        }
    });
}
