//! Audit log: who changed what, kept per studio and append-only.

use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::TenantTx;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub org_id: Uuid,
    pub actor_staff_id: Option<Uuid>,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<Uuid>,
    pub detail: Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct NewAuditEntry {
    /// `None` when the system made the change (a scheduled job).
    pub actor_staff_id: Option<Uuid>,
    /// What happened, for example `fee.marked_paid`.
    pub action: String,
    /// The kind of thing changed, for example `fee`.
    pub entity: String,
    pub entity_id: Option<Uuid>,
    /// Before and after values, or any other context.
    pub detail: Value,
}

const COLUMNS: &str = "id, org_id, actor_staff_id, action, entity, entity_id, detail, created_at";

/// Record a change. Call it in the same transaction as the change itself, so
/// the two succeed or fail together.
pub async fn record(tx: &mut TenantTx, entry: NewAuditEntry) -> Result<AuditEntry> {
    let org_id = tx.org_id();
    let saved = sqlx::query_as::<_, AuditEntry>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO audit_log (org_id, actor_staff_id, action, entity, entity_id, detail)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    )))
    .bind(org_id)
    .bind(entry.actor_staff_id)
    .bind(entry.action)
    .bind(entry.entity)
    .bind(entry.entity_id)
    .bind(entry.detail)
    .fetch_one(tx.conn())
    .await?;
    Ok(saved)
}

/// The most recent entries for the current studio, newest first.
pub async fn recent(tx: &mut TenantTx, limit: i64) -> Result<Vec<AuditEntry>> {
    let entries = sqlx::query_as::<_, AuditEntry>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM audit_log ORDER BY created_at DESC, id DESC LIMIT $1"
    )))
    .bind(limit.clamp(1, 500))
    .fetch_all(tx.conn())
    .await?;
    Ok(entries)
}
