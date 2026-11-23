//! What the studio charges: a monthly fee for a regular class, a pack of
//! classes, or a single drop-in class.
//!
//! Plans are never deleted, only made inactive, because enrollments and
//! credit packs keep pointing at the plan they were sold on. Changing a
//! price doesn't touch packs already sold: the price is copied onto each
//! pack when it's sold.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{validate, Error, Result};

/// Most a single plan can cost: ₹10,00,000, in paise.
pub const MAX_PRICE_PAISE: i64 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "plan_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// A fee every month for being in a class.
    Monthly,
    /// Several classes, used within a number of days.
    Pack,
    /// One class.
    DropIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Plan {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub kind: PlanKind,
    pub price_paise: i64,
    pub classes: Option<i32>,
    pub valid_days: Option<i32>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPlan {
    pub name: String,
    pub kind: PlanKind,
    pub price_paise: i64,
    /// Packs only. Ignored for monthly; always 1 for a drop-in.
    pub classes: Option<i32>,
    /// Packs and drop-ins only.
    pub valid_days: Option<i32>,
}

/// Everything editable about a plan, sent whole. The kind can't change:
/// a monthly plan that becomes a pack would confuse everything sold on it.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanEdit {
    pub name: String,
    pub price_paise: i64,
    pub classes: Option<i32>,
    pub valid_days: Option<i32>,
    pub active: bool,
}

const COLUMNS: &str =
    "id, org_id, name, kind, price_paise, classes, valid_days, active, created_at, updated_at";

/// All plans: offered ones first, then by kind and price.
pub async fn list(tx: &mut TenantTx) -> Result<Vec<Plan>> {
    let plans = sqlx::query_as::<_, Plan>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM plans ORDER BY active DESC, kind, price_paise, lower(name)"
    )))
    .fetch_all(tx.conn())
    .await?;
    Ok(plans)
}

pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<Plan> {
    let plan = sqlx::query_as::<_, Plan>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM plans WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(plan)
}

pub async fn count_active(tx: &mut TenantTx) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM plans WHERE active")
        .fetch_one(tx.conn())
        .await?;
    Ok(n)
}

pub async fn create(tx: &mut TenantTx, actor: Actor, new: NewPlan) -> Result<Plan> {
    actor.require(Capability::ManageClasses)?;
    let name = validate::required(&new.name, 80, "Plan names")?;
    check_price(new.price_paise)?;
    let (classes, valid_days) = shape(new.kind, new.classes, new.valid_days)?;
    if name_taken(tx, &name, None).await? {
        return Err(Error::Conflict(
            "There's already a plan with that name.".into(),
        ));
    }

    let plan = sqlx::query_as::<_, Plan>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO plans (org_id, name, kind, price_paise, classes, valid_days)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    )))
    .bind(tx.org_id())
    .bind(&name)
    .bind(new.kind)
    .bind(new.price_paise)
    .bind(classes)
    .bind(valid_days)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "plan.created".into(),
            entity: "plan".into(),
            entity_id: Some(plan.id),
            detail: json!({ "kind": plan.kind, "price_paise": plan.price_paise }),
        },
    )
    .await?;
    Ok(plan)
}

pub async fn update(tx: &mut TenantTx, actor: Actor, id: Uuid, edit: PlanEdit) -> Result<Plan> {
    actor.require(Capability::ManageClasses)?;
    let name = validate::required(&edit.name, 80, "Plan names")?;
    check_price(edit.price_paise)?;
    let before = get(tx, id).await?;
    let (classes, valid_days) = shape(before.kind, edit.classes, edit.valid_days)?;
    if name_taken(tx, &name, Some(id)).await? {
        return Err(Error::Conflict(
            "There's already a plan with that name.".into(),
        ));
    }

    let plan = sqlx::query_as::<_, Plan>(sqlx::AssertSqlSafe(format!(
        "UPDATE plans
            SET name = $2, price_paise = $3, classes = $4, valid_days = $5, active = $6,
                updated_at = now()
          WHERE id = $1
         RETURNING {COLUMNS}"
    )))
    .bind(id)
    .bind(&name)
    .bind(edit.price_paise)
    .bind(classes)
    .bind(valid_days)
    .bind(edit.active)
    .fetch_one(tx.conn())
    .await?;

    let mut detail = serde_json::Map::new();
    if before.price_paise != plan.price_paise {
        detail.insert(
            "price_paise".into(),
            json!({ "from": before.price_paise, "to": plan.price_paise }),
        );
    }
    if before.active != plan.active {
        detail.insert(
            "active".into(),
            json!({ "from": before.active, "to": plan.active }),
        );
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "plan.updated".into(),
            entity: "plan".into(),
            entity_id: Some(id),
            detail: serde_json::Value::Object(detail),
        },
    )
    .await?;
    Ok(plan)
}

fn check_price(price_paise: i64) -> Result<()> {
    if !(0..=MAX_PRICE_PAISE).contains(&price_paise) {
        return Err(Error::Invalid(format!(
            "Prices must be between ₹0 and {}.",
            validate::rupees(MAX_PRICE_PAISE)
        )));
    }
    Ok(())
}

/// The class count and validity a kind of plan needs, with plain messages
/// before the database's own check (`plans_shape`) gets a say.
fn shape(
    kind: PlanKind,
    classes: Option<i32>,
    valid_days: Option<i32>,
) -> Result<(Option<i32>, Option<i32>)> {
    match kind {
        PlanKind::Monthly => Ok((None, None)),
        PlanKind::Pack => {
            let classes = validate::range(classes, 2, 500, "A pack's classes")?
                .ok_or_else(|| Error::Invalid("Say how many classes the pack has.".into()))?;
            let days = validate::range(valid_days, 1, 730, "Days a pack is valid")?
                .ok_or_else(|| Error::Invalid("Say how many days the pack is valid.".into()))?;
            Ok((Some(classes), Some(days)))
        }
        PlanKind::DropIn => {
            let days =
                validate::range(valid_days, 1, 365, "Days a drop-in is valid")?.unwrap_or(30);
            Ok((Some(1), Some(days)))
        }
    }
}

async fn name_taken(tx: &mut TenantTx, name: &str, except: Option<Uuid>) -> Result<bool> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM plans WHERE lower(name) = lower($1) AND id IS DISTINCT FROM $2)",
    )
    .bind(name)
    .bind(except)
    .fetch_one(tx.conn())
    .await?;
    Ok(taken)
}
