//! Organizations: one per studio business.

use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::TenantTx;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub timezone: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct NewOrganization {
    pub name: String,
    /// Short lowercase handle, letters, digits and hyphens.
    pub slug: String,
    /// IANA timezone name. Defaults to Asia/Kolkata when `None`.
    pub timezone: Option<String>,
}

/// Create a studio.
///
/// This is the one write that cannot happen inside a studio's scope, because
/// the studio does not exist yet. It takes the pool directly and must only be
/// called from signup or support tooling.
pub async fn create(pool: &PgPool, new: NewOrganization) -> Result<Organization> {
    let org = sqlx::query_as::<_, Organization>(
        "INSERT INTO organizations (name, slug, timezone)
         VALUES ($1, $2, coalesce($3, 'Asia/Kolkata'))
         RETURNING id, name, slug, timezone, created_at",
    )
    .bind(new.name.trim())
    .bind(new.slug.trim().to_lowercase())
    .bind(new.timezone)
    .fetch_one(pool)
    .await?;
    Ok(org)
}

/// Find a studio by its slug. Like `create`, this runs outside any studio,
/// so it is only for signup and support or development tooling.
pub async fn find_by_slug(pool: &PgPool, slug: &str) -> Result<Option<Organization>> {
    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, slug, timezone, created_at FROM organizations WHERE slug = $1",
    )
    .bind(slug.trim().to_lowercase())
    .fetch_optional(pool)
    .await?;
    Ok(org)
}

/// The studio this transaction is scoped to.
pub async fn current(tx: &mut TenantTx) -> Result<Organization> {
    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, slug, timezone, created_at FROM organizations",
    )
    .fetch_one(tx.conn())
    .await?;
    Ok(org)
}

/// Rename the current studio.
pub async fn rename(tx: &mut TenantTx, name: &str) -> Result<Organization> {
    let org = sqlx::query_as::<_, Organization>(
        "UPDATE organizations SET name = $1
         RETURNING id, name, slug, timezone, created_at",
    )
    .bind(name.trim())
    .fetch_one(tx.conn())
    .await?;
    Ok(org)
}
