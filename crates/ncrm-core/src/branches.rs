//! Branches: a studio's physical locations.

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::TenantTx;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Branch {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub address: Option<String>,
    pub timezone: Option<String>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Default)]
pub struct NewBranch {
    pub name: String,
    pub address: Option<String>,
    pub timezone: Option<String>,
}

const COLUMNS: &str = "id, org_id, name, address, timezone, active, created_at";

pub async fn create(tx: &mut TenantTx, new: NewBranch) -> Result<Branch> {
    let org_id = tx.org_id();
    let branch = sqlx::query_as::<_, Branch>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO branches (org_id, name, address, timezone)
         VALUES ($1, $2, $3, $4)
         RETURNING {COLUMNS}"
    )))
    .bind(org_id)
    .bind(new.name.trim())
    .bind(new.address)
    .bind(new.timezone)
    .fetch_one(tx.conn())
    .await?;
    Ok(branch)
}

/// All branches of the current studio, by name.
pub async fn list(tx: &mut TenantTx) -> Result<Vec<Branch>> {
    let branches = sqlx::query_as::<_, Branch>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM branches ORDER BY name"
    )))
    .fetch_all(tx.conn())
    .await?;
    Ok(branches)
}

/// One branch. Returns `NotFound` when it does not exist or belongs to
/// another studio.
pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<Branch> {
    let branch = sqlx::query_as::<_, Branch>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM branches WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(branch)
}
