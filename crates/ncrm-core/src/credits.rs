//! Class credits: what a student has left on packs and drop-ins.
//!
//! The ledger is append-only. A *lot* is credits given at once (a pack
//! sold, a drop-in, free classes from the studio); every later change is a
//! *move* on a lot. A lot's remaining credits are its classes plus the sum
//! of its moves, and a database trigger keeps that between zero and the
//! lot's size whatever the application does (see migration 0004).
//!
//! Credits are spent from the lot that expires soonest, so a student never
//! loses credits to expiry while holding ones that would have lasted.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::plans::PlanKind;
use crate::students::StudentStatus;
use crate::{validate, Error, Result};

/// Most free classes staff can give in one go.
pub const MAX_GIFT: i32 = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct CreditLot {
    pub id: Uuid,
    pub plan_id: Option<Uuid>,
    /// The pack's name, or `None` for free classes.
    pub plan_name: Option<String>,
    pub classes: i32,
    pub remaining: i32,
    pub price_paise: i64,
    /// Last day the credits can be used.
    pub expires_on: Option<Date>,
    #[sqlx(skip)]
    pub expired: bool,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Credits {
    /// Classes they can still use: unexpired lots only.
    pub available: i32,
    /// The soonest expiry among usable credits, and how many expire then.
    pub next_expiry: Option<Expiry>,
    /// Newest first.
    pub lots: Vec<CreditLot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Expiry {
    pub on: Date,
    pub classes: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SellPack {
    pub plan_id: Uuid,
    /// Set when they paid on the spot; otherwise the price is left due.
    #[serde(default)]
    pub paid: Option<crate::fees::PaidNow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GiveCredits {
    pub classes: i32,
    /// Last day they can be used; `None` means they don't expire.
    pub expires_on: Option<Date>,
    /// Why, for the record. Required.
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TakeBack {
    pub classes: i32,
    /// Why, for the record. Required.
    pub note: String,
}

/// A student's credits. `NotFound` when the student isn't in this studio.
pub async fn summary(tx: &mut TenantTx, student_id: Uuid) -> Result<Credits> {
    // Confirms the student exists here (RLS hides other studios' students).
    let _: Uuid = sqlx::query_scalar("SELECT id FROM students WHERE id = $1")
        .bind(student_id)
        .fetch_one(tx.conn())
        .await?;
    let today = tx.studio_today().await?;
    let mut lots = sqlx::query_as::<_, CreditLot>(
        "SELECT l.id, l.plan_id, p.name AS plan_name, l.classes,
                (l.classes + coalesce((SELECT sum(m.change) FROM credit_moves m
                                        WHERE m.org_id = l.org_id AND m.lot_id = l.id), 0))::int4
                  AS remaining,
                l.price_paise, l.expires_on, l.note, l.created_at
           FROM credit_lots l
           LEFT JOIN plans p ON p.org_id = l.org_id AND p.id = l.plan_id
          WHERE l.student_id = $1
          ORDER BY l.created_at DESC
          LIMIT 100",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;

    let mut available = 0;
    let mut next_expiry: Option<Expiry> = None;
    for lot in &mut lots {
        lot.expired = lot.expires_on.is_some_and(|d| d < today);
        if lot.expired || lot.remaining == 0 {
            continue;
        }
        available += lot.remaining;
        if let Some(on) = lot.expires_on {
            next_expiry = match next_expiry {
                Some(e) if e.on < on => Some(e),
                Some(e) if e.on == on => Some(Expiry {
                    on,
                    classes: e.classes + lot.remaining,
                }),
                _ => Some(Expiry {
                    on,
                    classes: lot.remaining,
                }),
            };
        }
    }
    Ok(Credits {
        available,
        next_expiry,
        lots,
    })
}

/// Sell a pack or drop-in: the student gets its classes, valid from today
/// for the plan's number of days. The price is copied onto the lot.
pub async fn sell(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    sale: SellPack,
) -> Result<Credits> {
    actor.require(Capability::ManageFees)?;
    usable_student(tx, student_id).await?;
    let plan = sqlx::query_as::<_, PlanTerms>(
        "SELECT name, kind, price_paise, classes, valid_days, active FROM plans WHERE id = $1",
    )
    .bind(sale.plan_id)
    .fetch_optional(tx.conn())
    .await?;
    let Some(PlanTerms {
        name: plan_name,
        kind,
        price_paise: price,
        classes,
        valid_days,
        active,
    }) = plan
    else {
        return Err(Error::Invalid("Choose one of your packs.".into()));
    };
    if kind == PlanKind::Monthly {
        return Err(Error::Invalid(
            "Monthly plans are set when someone joins a class, not sold as credits.".into(),
        ));
    }
    if !active {
        return Err(Error::Invalid(format!("{plan_name} is no longer offered.")));
    }
    // plans_shape guarantees both are set for packs and drop-ins.
    let classes = classes.unwrap_or(1);
    let valid_days = valid_days.unwrap_or(30);
    let today = tx.studio_today().await?;
    // Valid for N days including today, so the last usable day is N - 1 on.
    let expires_on = today + Duration::days(i64::from(valid_days) - 1);

    let lot_id: Uuid = sqlx::query_scalar(
        "INSERT INTO credit_lots (org_id, student_id, plan_id, classes, price_paise, expires_on, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(sale.plan_id)
    .bind(classes)
    .bind(price)
    .bind(expires_on)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "credits.sold".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({
                "lot_id": lot_id,
                "plan_id": sale.plan_id,
                "classes": classes,
                "price_paise": price,
            }),
        },
    )
    .await?;
    // New credits first pay for classes already attended without one.
    crate::attendance::settle_owed(tx, actor, student_id).await?;
    // The price becomes a charge: paid now, or left on their dues.
    crate::fees::charge_for_sale(
        tx,
        actor,
        student_id,
        crate::fees::Sale {
            lot_id,
            kind: if kind == PlanKind::DropIn {
                crate::fees::ChargeKind::DropIn
            } else {
                crate::fees::ChargeKind::Pack
            },
            description: plan_name,
            amount_paise: price,
            paid: sale.paid,
        },
    )
    .await?;
    summary(tx, student_id).await
}

/// Give free classes (a make-up, a goodwill gesture). Owners and managers
/// only, with a reason, because this is money the studio doesn't collect.
pub async fn give(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    gift: GiveCredits,
) -> Result<Credits> {
    actor.require(Capability::ManageClasses)?;
    usable_student(tx, student_id).await?;
    let classes = validate::range(Some(gift.classes), 1, MAX_GIFT, "Free classes")?.unwrap_or(1);
    let note = validate::required(&gift.note, 200, "The reason")?;
    let today = tx.studio_today().await?;
    if gift.expires_on.is_some_and(|d| d < today) {
        return Err(Error::Invalid("The expiry date has already passed.".into()));
    }

    let lot_id: Uuid = sqlx::query_scalar(
        "INSERT INTO credit_lots (org_id, student_id, classes, expires_on, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(classes)
    .bind(gift.expires_on)
    .bind(&note)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "credits.given".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "lot_id": lot_id, "classes": classes }),
        },
    )
    .await?;
    // New credits first pay for classes already attended without one.
    crate::attendance::settle_owed(tx, actor, student_id).await?;
    summary(tx, student_id).await
}

/// Take back usable credits (a pack sold by mistake, a refund). Owners and
/// managers only, with a reason. Taken from the soonest-expiring credits.
pub async fn take_back(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    request: TakeBack,
) -> Result<Credits> {
    actor.require(Capability::ManageClasses)?;
    let classes =
        validate::range(Some(request.classes), 1, 500, "Classes to take back")?.unwrap_or(1);
    let note = validate::required(&request.note, 200, "The reason")?;
    // One credit change per student at a time, so two screens can't both
    // take the same credits. (The trigger would refuse the second anyway;
    // this turns that into a clear message instead of an error.)
    tx.lock_studio(&format!("credits:{student_id}")).await?;

    let before = summary(tx, student_id).await?;
    if before.available < classes {
        return Err(Error::Invalid(format!(
            "They have {} usable {}.",
            before.available,
            if before.available == 1 {
                "class"
            } else {
                "classes"
            }
        )));
    }

    // Soonest expiry first; credits that never expire last.
    let mut usable: Vec<&CreditLot> = before
        .lots
        .iter()
        .filter(|l| !l.expired && l.remaining > 0)
        .collect();
    usable.sort_by_key(|l| (l.expires_on.is_none(), l.expires_on, l.created_at));

    let mut left = classes;
    for lot in usable {
        if left == 0 {
            break;
        }
        let take = left.min(lot.remaining);
        sqlx::query(
            "INSERT INTO credit_moves (org_id, lot_id, change, reason, note, created_by)
             VALUES ($1, $2, $3, 'removed', $4, $5)",
        )
        .bind(tx.org_id())
        .bind(lot.id)
        .bind(-take)
        .bind(&note)
        .bind(actor.staff_id)
        .execute(tx.conn())
        .await?;
        left -= take;
    }

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "credits.taken_back".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "classes": classes }),
        },
    )
    .await?;
    summary(tx, student_id).await
}

/// Spend one credit for a class on `on`, from the usable lot that expires
/// soonest. Returns the lot, or `None` when the student has no credit that
/// was valid that day.
pub(crate) async fn use_one(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    on: Date,
) -> Result<Option<Uuid>> {
    tx.lock_studio(&format!("credits:{student_id}")).await?;
    let lot: Option<Uuid> = sqlx::query_scalar(
        "SELECT l.id FROM credit_lots l
          WHERE l.student_id = $1
            AND (l.expires_on IS NULL OR l.expires_on >= $2)
            AND l.classes + coalesce((SELECT sum(m.change) FROM credit_moves m
                                       WHERE m.org_id = l.org_id AND m.lot_id = l.id), 0) > 0
          ORDER BY l.expires_on NULLS LAST, l.created_at
          LIMIT 1",
    )
    .bind(student_id)
    .bind(on)
    .fetch_optional(tx.conn())
    .await?;
    if let Some(lot_id) = lot {
        sqlx::query(
            "INSERT INTO credit_moves (org_id, lot_id, change, reason, created_by)
             VALUES ($1, $2, -1, 'used', $3)",
        )
        .bind(tx.org_id())
        .bind(lot_id)
        .bind(actor.staff_id)
        .execute(tx.conn())
        .await?;
    }
    Ok(lot)
}

/// Give back a credit that `use_one` took (an attendance mark corrected).
pub(crate) async fn return_one(tx: &mut TenantTx, actor: Actor, lot_id: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO credit_moves (org_id, lot_id, change, reason, created_by)
         VALUES ($1, $2, 1, 'returned', $3)",
    )
    .bind(tx.org_id())
    .bind(lot_id)
    .bind(actor.staff_id)
    .execute(tx.conn())
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct PlanTerms {
    name: String,
    kind: PlanKind,
    price_paise: i64,
    classes: Option<i32>,
    valid_days: Option<i32>,
    active: bool,
}

/// The student's name, refusing students who have left.
async fn usable_student(tx: &mut TenantTx, student_id: Uuid) -> Result<String> {
    let (name, status): (String, StudentStatus) =
        sqlx::query_as("SELECT name, status FROM students WHERE id = $1")
            .bind(student_id)
            .fetch_one(tx.conn())
            .await?;
    if status == StudentStatus::Left {
        return Err(Error::Invalid(format!(
            "{name} has left. Set them as active first."
        )));
    }
    Ok(name)
}
