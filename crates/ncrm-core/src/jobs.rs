//! Work the background worker does on a timer, for every studio.
//!
//! This is the second place (after `auth`) allowed to look across
//! studios, and it does so for exactly one thing: the list of studio ids.
//! Everything a job then does for a studio happens inside that studio's
//! own `TenantTx`, under row-level security, exactly like a request.
//!
//! Jobs only do what a page load would also do (create today's sessions,
//! create this month's fees), so running them twice, or alongside someone
//! using the dashboard, changes nothing. They exist so the numbers are
//! right even on a day nobody opens the dashboard: an unmarked class is
//! still listed, the first of the month still creates the fees.

use sqlx::PgPool;
use time::Duration;
use uuid::Uuid;

use crate::db::TenantTx;
use crate::whatsapp::MessageSender;
use crate::{attendance, auth, fees, messaging, Result};

/// Every studio's id. The one cross-studio query jobs make; it returns
/// ids only.
pub async fn studio_ids(pool: &PgPool) -> Result<Vec<Uuid>> {
    let ids = sqlx::query_scalar("SELECT id FROM organizations ORDER BY created_at, id")
        .fetch_all(pool)
        .await?;
    Ok(ids)
}

/// Studios with WhatsApp messages waiting to go out (queued, or claimed
/// by a sender that seems to have died). A cross-studio query; it returns
/// ids only, and exists so the worker doesn't open every studio every few
/// seconds to find most of them have nothing to send.
pub async fn studios_with_messages_to_send(pool: &PgPool) -> Result<Vec<Uuid>> {
    let ids = sqlx::query_scalar(
        "SELECT DISTINCT org_id FROM messages WHERE status IN ('queued', 'sending')",
    )
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

/// What one pass over the outbox did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outbox {
    pub studios: usize,
    pub sent: u32,
    pub failed: u32,
    pub retrying: u32,
    /// Studios whose pass failed, with why.
    pub errors: Vec<(Uuid, String)>,
}

/// Send every studio's queued WhatsApp messages. The API sends a message
/// the moment it is written; this is the safety net for when that didn't
/// work (WhatsApp was unreachable, the API restarted) and for retries.
pub async fn send_messages(pool: &PgPool, sender: &MessageSender) -> Result<Outbox> {
    let ids = studios_with_messages_to_send(pool).await?;
    let mut done = Outbox {
        studios: ids.len(),
        ..Outbox::default()
    };
    for org_id in ids {
        match messaging::send_queued(pool, org_id, sender).await {
            Ok(sent) => {
                done.sent += sent.sent;
                done.failed += sent.failed;
                done.retrying += sent.retrying;
            }
            Err(err) => done.errors.push((org_id, err.to_string())),
        }
    }
    Ok(done)
}

/// What one housekeeping pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Housekeeping {
    pub cleanup: auth::Cleanup,
    pub studios: usize,
    /// Monthly fees created across all studios.
    pub fees_created: u64,
    /// Studios whose pass failed, with why. One studio's failure doesn't
    /// stop the others.
    pub failed: Vec<(Uuid, String)>,
}

/// For one studio: make sure the last week's class meetings exist (so
/// unmarked ones show up) and this month's fees are created.
pub async fn studio_upkeep(pool: &PgPool, org_id: Uuid) -> Result<u64> {
    let mut tx = TenantTx::begin(pool, org_id).await?;
    let today = tx.studio_today().await?;
    attendance::ensure(&mut tx, today - Duration::days(7), today).await?;
    let fees_created = fees::ensure_current_month(&mut tx).await?;
    tx.commit().await?;
    Ok(fees_created)
}

/// One full pass: tidy sign-in records, then upkeep for every studio.
pub async fn housekeeping(pool: &PgPool) -> Result<Housekeeping> {
    let cleanup = auth::cleanup(pool).await?;
    let ids = studio_ids(pool).await?;
    let mut done = Housekeeping {
        cleanup,
        studios: ids.len(),
        ..Housekeeping::default()
    };
    for org_id in ids {
        match studio_upkeep(pool, org_id).await {
            Ok(created) => done.fees_created += created,
            Err(err) => done.failed.push((org_id, err.to_string())),
        }
    }
    Ok(done)
}
