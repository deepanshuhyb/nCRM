//! Staff: the people who run a studio and sign in to the dashboard.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::TenantTx;
use crate::{phone, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "staff_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum StaffRole {
    /// Full access, including billing and staff management.
    Owner,
    /// Runs one or more branches.
    Manager,
    /// Enrolls students, takes fees, marks attendance.
    FrontDesk,
    /// Teaches classes and marks attendance for them.
    Instructor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Staff {
    pub id: Uuid,
    pub org_id: Uuid,
    /// `None` means the person works across all branches.
    pub branch_id: Option<Uuid>,
    pub phone: String,
    pub name: String,
    pub role: StaffRole,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct NewStaff {
    pub name: String,
    /// As typed; it is normalized to E.164 before saving.
    pub phone: String,
    pub role: StaffRole,
    pub branch_id: Option<Uuid>,
}

const COLUMNS: &str = "id, org_id, branch_id, phone, name, role, active, created_at";

pub async fn create(tx: &mut TenantTx, new: NewStaff) -> Result<Staff> {
    let org_id = tx.org_id();
    let phone = phone::normalize(&new.phone)?;
    let staff = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO staff (org_id, branch_id, phone, name, role)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {COLUMNS}"
    )))
    .bind(org_id)
    .bind(new.branch_id)
    .bind(phone)
    .bind(new.name.trim())
    .bind(new.role)
    .fetch_one(tx.conn())
    .await?;
    Ok(staff)
}

/// All staff of the current studio, by name.
pub async fn list(tx: &mut TenantTx) -> Result<Vec<Staff>> {
    let staff = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM staff ORDER BY name, id"
    )))
    .fetch_all(tx.conn())
    .await?;
    Ok(staff)
}

/// The staff member with this phone number in the current studio, if any.
pub async fn find_by_phone(tx: &mut TenantTx, phone_as_typed: &str) -> Result<Option<Staff>> {
    let phone = phone::normalize(phone_as_typed)?;
    let staff = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM staff WHERE phone = $1"
    )))
    .bind(phone)
    .fetch_optional(tx.conn())
    .await?;
    Ok(staff)
}
