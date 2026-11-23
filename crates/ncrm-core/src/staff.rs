//! Staff: the people who run a studio and sign in to the dashboard.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{phone, Error, Result};

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

/// One staff member of the current studio.
pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<Staff> {
    let staff = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM staff WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(staff)
}

/// Add someone to the team, applying the rules on who may hire whom.
///
/// Only an owner can add another owner. Recorded in the audit log.
pub async fn hire(tx: &mut TenantTx, actor: Actor, new: NewStaff) -> Result<Staff> {
    actor.require(Capability::ManageStaff)?;
    if new.role == StaffRole::Owner && actor.role != StaffRole::Owner {
        return Err(Error::Forbidden);
    }
    validate_name(&new.name)?;
    let staff = create(tx, new).await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "staff.hired".into(),
            entity: "staff".into(),
            entity_id: Some(staff.id),
            detail: json!({ "name": staff.name, "role": staff.role }),
        },
    )
    .await?;
    Ok(staff)
}

/// The full, editable profile of a staff member. Sent whole (PUT), so
/// "no branch" and "field not sent" can't be confused.
#[derive(Debug, Clone, Deserialize)]
pub struct StaffEdit {
    pub name: String,
    pub phone: String,
    pub role: StaffRole,
    pub branch_id: Option<Uuid>,
    pub active: bool,
}

/// Change a staff member, applying the rules that stop privilege games:
///
/// - Managers can't change owners, and can't make anyone an owner.
/// - Nobody can change their own role or deactivate themselves.
/// - The studio always keeps at least one active owner.
///
/// Changing someone's phone number signs them out everywhere, because a
/// session is only valid while the staff row still has the session's phone.
pub async fn update(tx: &mut TenantTx, actor: Actor, id: Uuid, edit: StaffEdit) -> Result<Staff> {
    actor.require(Capability::ManageStaff)?;
    validate_name(&edit.name)?;
    let phone = phone::normalize(&edit.phone)?;

    let current = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM staff WHERE id = $1 FOR UPDATE"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;

    let touches_owner = current.role == StaffRole::Owner || edit.role == StaffRole::Owner;
    if touches_owner && actor.role != StaffRole::Owner {
        return Err(Error::Forbidden);
    }
    if current.id == actor.staff_id && (edit.role != current.role || !edit.active) {
        return Err(Error::Invalid(
            "You can't change your own role or deactivate yourself. Ask another owner.".into(),
        ));
    }

    let stops_being_owner = current.role == StaffRole::Owner
        && current.active
        && (edit.role != StaffRole::Owner || !edit.active);
    if stops_being_owner {
        // Lock every active owner row so two owners demoting each other at
        // the same moment are serialized and the second sees the first.
        let others: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM staff WHERE role = 'owner' AND active AND id <> $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_all(tx.conn())
        .await?;
        if others.is_empty() {
            return Err(Error::Invalid(
                "A studio needs at least one active owner. Make someone else an owner first."
                    .into(),
            ));
        }
    }

    let updated = sqlx::query_as::<_, Staff>(sqlx::AssertSqlSafe(format!(
        "UPDATE staff SET name = $2, phone = $3, role = $4, branch_id = $5, active = $6
          WHERE id = $1
          RETURNING {COLUMNS}"
    )))
    .bind(id)
    .bind(edit.name.trim())
    .bind(&phone)
    .bind(edit.role)
    .bind(edit.branch_id)
    .bind(edit.active)
    .fetch_one(tx.conn())
    .await?;

    // Field names, plus the before/after of role and active when those
    // changed (they're what an owner looks for in a dispute). Never phone
    // numbers: the audit log is kept for years and shouldn't be a contact list.
    let mut changed = Vec::new();
    let mut detail = serde_json::Map::new();
    if current.name != updated.name {
        changed.push("name");
    }
    if current.phone != updated.phone {
        changed.push("phone");
    }
    if current.role != updated.role {
        changed.push("role");
        detail.insert(
            "role".into(),
            json!({ "from": current.role, "to": updated.role }),
        );
    }
    if current.branch_id != updated.branch_id {
        changed.push("branch");
    }
    if current.active != updated.active {
        changed.push("active");
        detail.insert(
            "active".into(),
            json!({ "from": current.active, "to": updated.active }),
        );
    }
    detail.insert("changed".into(), json!(changed));
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "staff.updated".into(),
            entity: "staff".into(),
            entity_id: Some(id),
            detail: serde_json::Value::Object(detail),
        },
    )
    .await?;
    Ok(updated)
}

fn validate_name(name: &str) -> Result<()> {
    let len = name.trim().chars().count();
    if len == 0 || len > 120 {
        return Err(Error::Invalid("Names must be 1 to 120 characters.".into()));
    }
    Ok(())
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
