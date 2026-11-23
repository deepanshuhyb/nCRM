//! Rooms: the spaces inside a branch where classes happen.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{validate, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Room {
    pub id: Uuid,
    pub org_id: Uuid,
    pub branch_id: Uuid,
    pub name: String,
    pub capacity: Option<i32>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewRoom {
    pub name: String,
    pub capacity: Option<i32>,
}

/// Everything editable about a room, sent whole.
#[derive(Debug, Clone, Deserialize)]
pub struct RoomEdit {
    pub name: String,
    pub capacity: Option<i32>,
    pub active: bool,
}

const COLUMNS: &str = "id, org_id, branch_id, name, capacity, active, created_at";

/// Every room in the studio, by branch then name.
pub async fn list(tx: &mut TenantTx) -> Result<Vec<Room>> {
    let rooms = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM rooms ORDER BY branch_id, lower(name)"
    )))
    .fetch_all(tx.conn())
    .await?;
    Ok(rooms)
}

pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<Room> {
    let room = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM rooms WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(room)
}

/// Add a room to a branch. `NotFound` when the branch isn't in this studio.
pub async fn create(
    tx: &mut TenantTx,
    actor: Actor,
    branch_id: Uuid,
    new: NewRoom,
) -> Result<Room> {
    actor.require(Capability::ManageBranches)?;
    let name = validate::required(&new.name, 60, "Room names")?;
    let capacity = validate::range(new.capacity, 1, 1000, "Room capacity")?;
    // Checked first so a wrong branch is "not found", not a key error.
    crate::branches::get(tx, branch_id).await?;
    if name_taken(tx, branch_id, &name, None).await? {
        return Err(Error::Conflict(
            "That branch already has a room with that name.".into(),
        ));
    }

    let room = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO rooms (org_id, branch_id, name, capacity)
         VALUES ($1, $2, $3, $4)
         RETURNING {COLUMNS}"
    )))
    .bind(tx.org_id())
    .bind(branch_id)
    .bind(&name)
    .bind(capacity)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "room.created".into(),
            entity: "room".into(),
            entity_id: Some(room.id),
            detail: json!({ "name": room.name, "branch_id": branch_id }),
        },
    )
    .await?;
    Ok(room)
}

/// Rename, resize, or close a room. A room can't shrink below the
/// capacity of an active class that uses it, and can't close while one
/// does.
pub async fn update(tx: &mut TenantTx, actor: Actor, id: Uuid, edit: RoomEdit) -> Result<Room> {
    actor.require(Capability::ManageBranches)?;
    let name = validate::required(&edit.name, 60, "Room names")?;
    let capacity = validate::range(edit.capacity, 1, 1000, "Room capacity")?;
    let before = get(tx, id).await?;
    if name_taken(tx, before.branch_id, &name, Some(id)).await? {
        return Err(Error::Conflict(
            "That branch already has a room with that name.".into(),
        ));
    }

    // The largest active class held here, if any.
    let biggest: Option<(String, Option<i32>)> = sqlx::query_as(
        "SELECT name, capacity FROM batches
          WHERE room_id = $1 AND active
          ORDER BY capacity DESC NULLS LAST LIMIT 1",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((class_name, class_capacity)) = biggest {
        if !edit.active {
            return Err(Error::Invalid(format!(
                "{class_name} still meets in this room. Move or archive it first."
            )));
        }
        if let (Some(room_cap), Some(class_cap)) = (capacity, class_capacity) {
            if class_cap > room_cap {
                return Err(Error::Invalid(format!(
                    "{class_name} takes {class_cap} people here, so the room can't hold fewer."
                )));
            }
        }
    }

    let room = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "UPDATE rooms SET name = $2, capacity = $3, active = $4 WHERE id = $1
         RETURNING {COLUMNS}"
    )))
    .bind(id)
    .bind(&name)
    .bind(capacity)
    .bind(edit.active)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "room.updated".into(),
            entity: "room".into(),
            entity_id: Some(id),
            detail: json!({ "name": room.name, "capacity": room.capacity, "active": room.active }),
        },
    )
    .await?;
    Ok(room)
}

async fn name_taken(
    tx: &mut TenantTx,
    branch_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<bool> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM rooms
                         WHERE branch_id = $1 AND lower(name) = lower($2)
                           AND id IS DISTINCT FROM $3)",
    )
    .bind(branch_id)
    .bind(name)
    .bind(except)
    .fetch_one(tx.conn())
    .await?;
    Ok(taken)
}
