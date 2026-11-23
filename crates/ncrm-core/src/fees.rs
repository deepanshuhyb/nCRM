//! Fees: what students owe, what they paid, receipts and the dues list.
//!
//! Three tables (see migration 0007):
//! - a *charge* is money owed: a month's fee for a class, a pack that was
//!   sold, or anything else the front desk adds (costume, registration);
//! - a *payment* is money received, with a receipt number;
//! - an *allocation* says how much of a payment went to which charge.
//!
//! Money is always whole paise in an `i64`. Amounts are never edited: a
//! mistake is *voided* with a reason (owners and managers only) and entered
//! again. The front desk takes money; it can't make money disappear.
//!
//! Payments settle the oldest dues first, automatically (`settle`). Money
//! left over stays on the payment as an advance and is used by the next
//! charge.
//!
//! Monthly fees are created on demand, like class sessions: the first time
//! anyone opens fees in a month, `ensure_current_month` creates one charge
//! per student per class on a monthly plan. Nothing is created until the
//! studio switches monthly fees on (`organizations.fees_from`).

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, Duration, Month, OffsetDateTime};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{validate, Error, Result};

/// The most one charge or payment can be: ₹10,00,000.
pub const MAX_AMOUNT_PAISE: i64 = 100_000_000;
/// A payment can be dated this many days back (money found in the drawer).
pub const PAYMENT_DAYS_BACK: i64 = 60;
/// Without "view financials", the payments list goes back this far: enough
/// to tally the cash at the end of the day and reprint a receipt.
pub const FRONT_DESK_DAYS_BACK: i64 = 7;
/// The longest stretch the payments list covers in one request.
const MAX_RANGE_DAYS: i64 = 366;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "charge_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ChargeKind {
    Monthly,
    Pack,
    DropIn,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "payment_method", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Upi,
    Card,
    BankTransfer,
    Cheque,
    Other,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Charge {
    pub id: Uuid,
    pub kind: ChargeKind,
    pub description: String,
    pub amount_paise: i64,
    /// How much of it payments have covered so far.
    pub paid_paise: i64,
    pub due_on: Date,
    /// Monthly fees: the first day of the month it's for.
    pub period_start: Option<Date>,
    /// Past its due date with something still to pay.
    pub overdue: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub voided_at: Option<OffsetDateTime>,
    pub void_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub student_id: Uuid,
    pub student_name: String,
    pub receipt_no: i64,
    pub amount_paise: i64,
    /// Not yet used by any charge: an advance.
    pub unallocated_paise: i64,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub received_on: Date,
    pub received_by_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub voided_at: Option<OffsetDateTime>,
    pub void_reason: Option<String>,
}

/// Everything about one student's money.
#[derive(Debug, Clone, Serialize)]
pub struct StudentFees {
    /// Still to pay, across all charges.
    pub due_paise: i64,
    /// The part of that which is past its due date.
    pub overdue_paise: i64,
    /// Paid ahead and not used yet.
    pub advance_paise: i64,
    /// Newest first, up to 60.
    pub charges: Vec<Charge>,
    /// Newest first, up to 60.
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewCharge {
    pub description: String,
    pub amount_paise: i64,
    /// Defaults to today.
    #[serde(default)]
    pub due_on: Option<Date>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPayment {
    pub amount_paise: i64,
    pub method: PaymentMethod,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// The day the money came in. Defaults to today.
    #[serde(default)]
    pub received_on: Option<Date>,
}

/// "They paid on the spot", when selling a pack.
#[derive(Debug, Clone, Deserialize)]
pub struct PaidNow {
    pub method: PaymentMethod,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Void {
    /// Why, for the record. Required.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReceiptLine {
    pub description: String,
    pub amount_paise: i64,
}

/// What a printed receipt shows.
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub payment: Payment,
    pub studio_name: String,
    /// What the money went to. Empty when all of it is an advance.
    pub lines: Vec<ReceiptLine>,
    /// What the student still owes after this.
    pub still_due_paise: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DuesQuery {
    #[serde(default)]
    pub branch_id: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DueRow {
    pub student_id: Uuid,
    pub student_name: String,
    pub branch_name: Option<String>,
    pub due_paise: i64,
    pub overdue_paise: i64,
    /// The due date of the oldest unpaid charge.
    pub oldest_due_on: Date,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dues {
    pub items: Vec<DueRow>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeesSummary {
    /// Monthly fees are created from this month on. `None`: switched off.
    pub fees_from: Option<Date>,
    pub fee_due_day: i16,
    pub due_paise: i64,
    pub overdue_paise: i64,
    pub students_owing: i64,
    /// Only for roles that may see financial reports.
    pub collected_this_month_paise: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeeSettings {
    /// First day of the first month to charge, or `None` to stop.
    pub fees_from: Option<Date>,
    pub fee_due_day: i16,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PaymentsQuery {
    /// Defaults to today.
    #[serde(default)]
    pub from: Option<Date>,
    /// Defaults to `from`.
    #[serde(default)]
    pub to: Option<Date>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MethodTotal {
    pub method: PaymentMethod,
    pub amount_paise: i64,
    pub payments: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Payments {
    pub from: Date,
    pub to: Date,
    pub items: Vec<Payment>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    /// Money actually kept (voided payments left out), by how it came in.
    pub by_method: Vec<MethodTotal>,
    pub collected_paise: i64,
}

// ---------------------------------------------------------------- helpers

/// One student's fees change one at a time. The allocation trigger takes
/// the same lock, so the database agrees with us about who goes first.
async fn lock_student(tx: &mut TenantTx, student_id: Uuid) -> Result<()> {
    tx.lock_studio(&format!("fees:{student_id}")).await
}

fn amount(paise: i64, what: &str) -> Result<i64> {
    if !(1..=MAX_AMOUNT_PAISE).contains(&paise) {
        return Err(Error::Invalid(format!(
            "{what} must be between ₹1 and {}.",
            validate::rupees(MAX_AMOUNT_PAISE)
        )));
    }
    Ok(paise)
}

fn first_of_month(date: Date) -> Date {
    date.replace_day(1).expect("every month has a first day")
}

fn month_label(date: Date) -> String {
    let name = match date.month() {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    };
    format!("{name} {}", date.year())
}

async fn student_name(tx: &mut TenantTx, student_id: Uuid) -> Result<String> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM students WHERE id = $1")
        .bind(student_id)
        .fetch_optional(tx.conn())
        .await?;
    name.ok_or(Error::NotFound)
}

const CHARGE_SELECT: &str = "
    SELECT c.id, c.kind, c.description, c.amount_paise,
           coalesce(a.paid, 0)::bigint AS paid_paise,
           c.due_on, c.period_start,
           (c.voided_at IS NULL AND c.due_on < $1 AND c.amount_paise > coalesce(a.paid, 0)) AS overdue,
           c.created_at, c.voided_at, c.void_reason
      FROM charges c
      LEFT JOIN LATERAL (
        SELECT sum(pa.amount_paise) AS paid FROM payment_allocations pa
         WHERE pa.org_id = c.org_id AND pa.charge_id = c.id
      ) a ON true";

const PAYMENT_SELECT: &str = "
    SELECT p.id, p.student_id, su.name AS student_name, p.receipt_no, p.amount_paise,
           (CASE WHEN p.voided_at IS NULL
                 THEN p.amount_paise - coalesce(a.used, 0) ELSE 0 END)::bigint AS unallocated_paise,
           p.method, p.reference, p.note, p.received_on, st.name AS received_by_name,
           p.created_at, p.voided_at, p.void_reason
      FROM payments p
      JOIN students su ON su.org_id = p.org_id AND su.id = p.student_id
      JOIN staff st ON st.org_id = p.org_id AND st.id = p.received_by
      LEFT JOIN LATERAL (
        SELECT sum(pa.amount_paise) AS used FROM payment_allocations pa
         WHERE pa.org_id = p.org_id AND pa.payment_id = p.id
      ) a ON true";

/// Use money that isn't allocated yet on charges that aren't paid yet,
/// oldest first on both sides. Safe to call any number of times. The
/// caller holds the student's lock.
async fn settle(tx: &mut TenantTx, student_id: Uuid) -> Result<()> {
    let mut money: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT p.id, (p.amount_paise - coalesce((
                    SELECT sum(a.amount_paise) FROM payment_allocations a
                     WHERE a.org_id = p.org_id AND a.payment_id = p.id), 0))::bigint
           FROM payments p
          WHERE p.student_id = $1 AND p.voided_at IS NULL
          ORDER BY p.received_on, p.receipt_no",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;
    money.retain(|(_, left)| *left > 0);
    if money.is_empty() {
        return Ok(());
    }
    let open: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT c.id, (c.amount_paise - coalesce((
                    SELECT sum(a.amount_paise) FROM payment_allocations a
                     WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0))::bigint
           FROM charges c
          WHERE c.student_id = $1 AND c.voided_at IS NULL
          ORDER BY c.due_on, c.created_at, c.id",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;

    let mut m = 0;
    for (charge_id, mut owed) in open {
        while owed > 0 && m < money.len() {
            let take = owed.min(money[m].1);
            sqlx::query(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(tx.org_id())
            .bind(student_id)
            .bind(money[m].0)
            .bind(charge_id)
            .bind(take)
            .execute(tx.conn())
            .await?;
            owed -= take;
            money[m].1 -= take;
            if money[m].1 == 0 {
                m += 1;
            }
        }
        if m == money.len() {
            break;
        }
    }
    Ok(())
}

/// (due, overdue, advance) for one student.
async fn balance(tx: &mut TenantTx, student_id: Uuid, today: Date) -> Result<(i64, i64, i64)> {
    let (due, overdue): (i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(o.open), 0)::bigint,
                coalesce(sum(o.open) FILTER (WHERE o.due_on < $2), 0)::bigint
           FROM (
             SELECT c.due_on, c.amount_paise - coalesce((
                      SELECT sum(a.amount_paise) FROM payment_allocations a
                       WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open
               FROM charges c
              WHERE c.student_id = $1 AND c.voided_at IS NULL
           ) o",
    )
    .bind(student_id)
    .bind(today)
    .fetch_one(tx.conn())
    .await?;
    let advance: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(p.amount_paise - coalesce((
                    SELECT sum(a.amount_paise) FROM payment_allocations a
                     WHERE a.org_id = p.org_id AND a.payment_id = p.id), 0)), 0)::bigint
           FROM payments p
          WHERE p.student_id = $1 AND p.voided_at IS NULL",
    )
    .bind(student_id)
    .fetch_one(tx.conn())
    .await?;
    Ok((due, overdue, advance))
}

// ----------------------------------------------------------- monthly fees

/// Create this month's fee for every student in a class on a monthly plan,
/// if the studio has switched monthly fees on. Safe to call any number of
/// times: one fee per class membership per month, ever (a waived fee
/// doesn't come back). Returns how many were created.
///
/// Call this before taking any student's fee lock: it locks the students
/// it charges, in id order, and a caller already holding one could
/// deadlock with it.
pub async fn ensure_current_month(tx: &mut TenantTx) -> Result<u64> {
    let (fees_from, due_day): (Option<Date>, i16) =
        sqlx::query_as("SELECT fees_from, fee_due_day FROM organizations WHERE id = $1")
            .bind(tx.org_id())
            .fetch_one(tx.conn())
            .await?;
    let today = tx.studio_today().await?;
    let month = first_of_month(today);
    let Some(from) = fees_from else {
        return Ok(0);
    };
    if month < from {
        return Ok(0);
    }
    // Two page loads at once: the second waits here, then finds nothing
    // left to create.
    tx.lock_studio("fees-month").await?;
    let due_on = month + Duration::days(i64::from(due_day) - 1);
    let mut charged: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO charges (org_id, student_id, kind, description, amount_paise, due_on,
                              enrollment_id, period_start)
         SELECT e.org_id, e.student_id, 'monthly', left(b.name || ', ' || $2::text, 120),
                p.price_paise, greatest($3::date, e.starts_on), e.id, $1::date
           FROM enrollments e
           JOIN plans p ON p.org_id = e.org_id AND p.id = e.plan_id
           JOIN batches b ON b.org_id = e.org_id AND b.id = e.batch_id
           JOIN students s ON s.org_id = e.org_id AND s.id = e.student_id
          WHERE p.kind = 'monthly' AND p.price_paise > 0
            AND s.status = 'active'
            AND e.starts_on <= $4
            AND (e.ended_on IS NULL OR e.ended_on > $1::date)
         ON CONFLICT (org_id, enrollment_id, period_start) WHERE enrollment_id IS NOT NULL
         DO NOTHING
         RETURNING student_id",
    )
    .bind(month)
    .bind(month_label(month))
    .bind(due_on)
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    let created = charged.len() as u64;
    if created == 0 {
        return Ok(0);
    }
    // Money paid ahead settles the new fee straight away.
    charged.sort_unstable();
    charged.dedup();
    for student_id in charged {
        lock_student(tx, student_id).await?;
        settle(tx, student_id).await?;
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: None,
            action: "fees.month_created".into(),
            entity: "organization".into(),
            entity_id: Some(tx.org_id()),
            detail: json!({ "month": month.to_string(), "charges": created }),
        },
    )
    .await?;
    Ok(created)
}

/// Switch monthly fees on from a month (or off), and set the due day.
pub async fn set_settings(
    tx: &mut TenantTx,
    actor: Actor,
    settings: FeeSettings,
) -> Result<FeesSummary> {
    actor.require(Capability::ManageClasses)?;
    if !(1..=28).contains(&settings.fee_due_day) {
        return Err(Error::Invalid(
            "The due day must be between 1 and 28, so every month has it.".into(),
        ));
    }
    let today = tx.studio_today().await?;
    let this_month = first_of_month(today);
    if let Some(from) = settings.fees_from {
        if from.day() != 1 {
            return Err(Error::Invalid(
                "Fees start from the first day of a month.".into(),
            ));
        }
        // Earlier months aren't created after the fact: add those by hand.
        let current: Option<Date> =
            sqlx::query_scalar("SELECT fees_from FROM organizations WHERE id = $1")
                .bind(tx.org_id())
                .fetch_one(tx.conn())
                .await?;
        if from < this_month && current != Some(from) {
            return Err(Error::Invalid(
                "Fees can start from this month or a later one.".into(),
            ));
        }
        if from > this_month + Duration::days(366) {
            return Err(Error::Invalid(
                "That's more than a year away. Pick a nearer month.".into(),
            ));
        }
    }
    sqlx::query("UPDATE organizations SET fees_from = $2, fee_due_day = $3 WHERE id = $1")
        .bind(tx.org_id())
        .bind(settings.fees_from)
        .bind(settings.fee_due_day)
        .execute(tx.conn())
        .await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "fees.settings_changed".into(),
            entity: "organization".into(),
            entity_id: Some(tx.org_id()),
            detail: json!({
                "fees_from": settings.fees_from.map(|d| d.to_string()),
                "fee_due_day": settings.fee_due_day,
            }),
        },
    )
    .await?;
    summary(tx, actor).await
}

// ---------------------------------------------------------------- charges

/// Add a charge by hand: registration, a costume, an old balance.
pub async fn add_charge(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    new: NewCharge,
) -> Result<StudentFees> {
    actor.require(Capability::ManageFees)?;
    student_name(tx, student_id).await?;
    let description = validate::required(&new.description, 120, "What it's for")?;
    let amount_paise = amount(new.amount_paise, "The amount")?;
    let today = tx.studio_today().await?;
    let due_on = new.due_on.unwrap_or(today);
    if due_on < today - Duration::days(366) || due_on > today + Duration::days(366) {
        return Err(Error::Invalid(
            "The due date must be within a year of today.".into(),
        ));
    }
    lock_student(tx, student_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO charges (org_id, student_id, kind, description, amount_paise, due_on, created_by)
         VALUES ($1, $2, 'other', $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(&description)
    .bind(amount_paise)
    .bind(due_on)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;
    settle(tx, student_id).await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "fees.charge_added".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "charge_id": id, "amount_paise": amount_paise, "kind": "other" }),
        },
    )
    .await?;
    student_fees(tx, student_id).await
}

/// A pack or drop-in that was just sold.
pub(crate) struct Sale {
    pub lot_id: Uuid,
    pub kind: ChargeKind,
    pub description: String,
    pub amount_paise: i64,
    /// Set when they paid on the spot.
    pub paid: Option<PaidNow>,
}

/// The charge for a sale, and its payment when they paid on the spot.
/// Called by `credits::sell`, which has already checked the role.
pub(crate) async fn charge_for_sale(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    sale: Sale,
) -> Result<()> {
    let Sale {
        lot_id,
        kind,
        description,
        amount_paise,
        paid,
    } = sale;
    if amount_paise <= 0 {
        return Ok(()); // a free pack: nothing to collect
    }
    lock_student(tx, student_id).await?;
    let today = tx.studio_today().await?;
    let description: String = description.chars().take(120).collect();
    let charge_id: Uuid = sqlx::query_scalar(
        "INSERT INTO charges (org_id, student_id, kind, description, amount_paise, due_on,
                              credit_lot_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(kind)
    .bind(&description)
    .bind(amount_paise)
    .bind(today)
    .bind(lot_id)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;
    match paid {
        Some(now) => {
            take_payment(
                tx,
                actor,
                student_id,
                NewPayment {
                    amount_paise,
                    method: now.method,
                    reference: now.reference,
                    note: None,
                    received_on: None,
                },
                Some(charge_id),
            )
            .await?;
        }
        None => settle(tx, student_id).await?,
    }
    Ok(())
}

/// Cancel a charge (a waiver, or one entered by mistake). The money that
/// had paid it is freed for the student's other dues, or kept as advance.
pub async fn void_charge(
    tx: &mut TenantTx,
    actor: Actor,
    charge_id: Uuid,
    void: Void,
) -> Result<StudentFees> {
    actor.require(Capability::AdjustFees)?;
    let reason = validate::required(&void.reason, 200, "The reason")?;
    let found: Option<(Uuid, i64, bool)> = sqlx::query_as(
        "SELECT student_id, amount_paise, voided_at IS NOT NULL FROM charges WHERE id = $1",
    )
    .bind(charge_id)
    .fetch_optional(tx.conn())
    .await?;
    let Some((student_id, amount_paise, _)) = found else {
        return Err(Error::NotFound);
    };
    lock_student(tx, student_id).await?;
    // Read again under the lock: someone else may have just cancelled it.
    let already: bool =
        sqlx::query_scalar("SELECT voided_at IS NOT NULL FROM charges WHERE id = $1")
            .bind(charge_id)
            .fetch_one(tx.conn())
            .await?;
    if already {
        return Err(Error::Conflict("That charge was already cancelled.".into()));
    }
    sqlx::query(
        "UPDATE charges SET voided_at = now(), voided_by = $2, void_reason = $3 WHERE id = $1",
    )
    .bind(charge_id)
    .bind(actor.staff_id)
    .bind(&reason)
    .execute(tx.conn())
    .await?;
    settle(tx, student_id).await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "fees.charge_voided".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({ "charge_id": charge_id, "amount_paise": amount_paise, "reason": reason }),
        },
    )
    .await?;
    student_fees(tx, student_id).await
}

// --------------------------------------------------------------- payments

/// Insert a payment and settle. The caller has checked the role and holds
/// (or is about to share) the student's lock.
///
/// `towards` is a charge this money was handed over for (a pack bought and
/// paid on the spot). It's paid first, so the receipt says what the
/// customer thinks they paid for; anything left follows the usual
/// oldest-first rule.
async fn take_payment(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    new: NewPayment,
    towards: Option<Uuid>,
) -> Result<Uuid> {
    let amount_paise = amount(new.amount_paise, "The amount")?;
    let reference = validate::optional(new.reference, 80, "The reference")?;
    let note = validate::optional(new.note, 200, "The note")?;
    let today = tx.studio_today().await?;
    let received_on = new.received_on.unwrap_or(today);
    if received_on > today {
        return Err(Error::Invalid(
            "The payment date can't be in the future.".into(),
        ));
    }
    if received_on < today - Duration::days(PAYMENT_DAYS_BACK) {
        return Err(Error::Invalid(format!(
            "The payment date can be up to {PAYMENT_DAYS_BACK} days back."
        )));
    }
    lock_student(tx, student_id).await?;
    // The next receipt number. The row lock on the counter makes two
    // payments at once take turns; a rollback gives the number back.
    let receipt_no: i64 = sqlx::query_scalar(
        "INSERT INTO receipt_counters (org_id, last_no) VALUES ($1, 1)
         ON CONFLICT (org_id) DO UPDATE SET last_no = receipt_counters.last_no + 1
         RETURNING last_no",
    )
    .bind(tx.org_id())
    .fetch_one(tx.conn())
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (org_id, student_id, receipt_no, amount_paise, method, reference,
                               note, received_on, received_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(receipt_no)
    .bind(amount_paise)
    .bind(new.method)
    .bind(&reference)
    .bind(&note)
    .bind(received_on)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;
    if let Some(charge_id) = towards {
        let open: i64 = sqlx::query_scalar(
            "SELECT (c.amount_paise - coalesce((
                        SELECT sum(a.amount_paise) FROM payment_allocations a
                         WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0))::bigint
               FROM charges c
              WHERE c.id = $1 AND c.student_id = $2 AND c.voided_at IS NULL",
        )
        .bind(charge_id)
        .bind(student_id)
        .fetch_optional(tx.conn())
        .await?
        .unwrap_or(0);
        let take = open.min(amount_paise);
        if take > 0 {
            sqlx::query(
                "INSERT INTO payment_allocations (org_id, student_id, payment_id, charge_id, amount_paise)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(tx.org_id())
            .bind(student_id)
            .bind(id)
            .bind(charge_id)
            .bind(take)
            .execute(tx.conn())
            .await?;
        }
    }
    settle(tx, student_id).await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "fees.payment_recorded".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            // No reference number here: the audit log isn't a place to
            // collect UPI ids.
            detail: json!({
                "payment_id": id,
                "receipt_no": receipt_no,
                "amount_paise": amount_paise,
                "method": new.method,
                "backdated": received_on < today,
            }),
        },
    )
    .await?;
    Ok(id)
}

/// Record money received. Settles the oldest dues first; anything left
/// over is kept as an advance.
pub async fn record_payment(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    new: NewPayment,
) -> Result<Receipt> {
    actor.require(Capability::ManageFees)?;
    student_name(tx, student_id).await?;
    let id = take_payment(tx, actor, student_id, new, None).await?;
    receipt(tx, actor, id).await
}

/// Cancel a recorded payment (entered by mistake, a bounced cheque, a
/// refund). Its receipt number is kept and never reused.
pub async fn void_payment(
    tx: &mut TenantTx,
    actor: Actor,
    payment_id: Uuid,
    void: Void,
) -> Result<StudentFees> {
    actor.require(Capability::AdjustFees)?;
    let reason = validate::required(&void.reason, 200, "The reason")?;
    let found: Option<(Uuid, i64, i64)> =
        sqlx::query_as("SELECT student_id, amount_paise, receipt_no FROM payments WHERE id = $1")
            .bind(payment_id)
            .fetch_optional(tx.conn())
            .await?;
    let Some((student_id, amount_paise, receipt_no)) = found else {
        return Err(Error::NotFound);
    };
    lock_student(tx, student_id).await?;
    let already: bool =
        sqlx::query_scalar("SELECT voided_at IS NOT NULL FROM payments WHERE id = $1")
            .bind(payment_id)
            .fetch_one(tx.conn())
            .await?;
    if already {
        return Err(Error::Conflict(
            "That payment was already cancelled.".into(),
        ));
    }
    sqlx::query(
        "UPDATE payments SET voided_at = now(), voided_by = $2, void_reason = $3 WHERE id = $1",
    )
    .bind(payment_id)
    .bind(actor.staff_id)
    .bind(&reason)
    .execute(tx.conn())
    .await?;
    // Other money the student paid ahead may now cover what this one did.
    settle(tx, student_id).await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "fees.payment_voided".into(),
            entity: "student".into(),
            entity_id: Some(student_id),
            detail: json!({
                "payment_id": payment_id,
                "receipt_no": receipt_no,
                "amount_paise": amount_paise,
                "reason": reason,
            }),
        },
    )
    .await?;
    student_fees(tx, student_id).await
}

/// One payment as a receipt. `NotFound` when it isn't this studio's.
pub async fn receipt(tx: &mut TenantTx, actor: Actor, payment_id: Uuid) -> Result<Receipt> {
    actor.require(Capability::ManageFees)?;
    let payment = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(format!(
        "{PAYMENT_SELECT} WHERE p.id = $1"
    )))
    .bind(payment_id)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(Error::NotFound)?;
    let lines = sqlx::query_as::<_, ReceiptLine>(
        "SELECT c.description, sum(pa.amount_paise)::bigint AS amount_paise
           FROM payment_allocations pa
           JOIN charges c ON c.org_id = pa.org_id AND c.id = pa.charge_id
          WHERE pa.payment_id = $1
          GROUP BY c.id, c.description, c.due_on
          ORDER BY c.due_on, c.description",
    )
    .bind(payment_id)
    .fetch_all(tx.conn())
    .await?;
    let studio_name: String = sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
        .bind(tx.org_id())
        .fetch_one(tx.conn())
        .await?;
    let today = tx.studio_today().await?;
    let (still_due_paise, _, _) = balance(tx, payment.student_id, today).await?;
    Ok(Receipt {
        payment,
        studio_name,
        lines,
        still_due_paise,
    })
}

/// Payments received between two dates, newest first, with totals by
/// method for the cash tally.
pub async fn payments(tx: &mut TenantTx, actor: Actor, query: PaymentsQuery) -> Result<Payments> {
    actor.require(Capability::ManageFees)?;
    let today = tx.studio_today().await?;
    let from = query.from.unwrap_or(today);
    let to = query.to.unwrap_or(from);
    if to < from {
        return Err(Error::Invalid(
            "The end date is before the start date.".into(),
        ));
    }
    if (to - from).whole_days() > MAX_RANGE_DAYS {
        return Err(Error::Invalid("Pick a range of a year or less.".into()));
    }
    if !actor.role.can(Capability::ViewFinancials)
        && from < today - Duration::days(FRONT_DESK_DAYS_BACK)
    {
        return Err(Error::Invalid(format!(
            "You can look back {FRONT_DESK_DAYS_BACK} days. Ask a manager for older payments."
        )));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let items = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(format!(
        "{PAYMENT_SELECT}
          WHERE p.received_on BETWEEN $1 AND $2
          ORDER BY p.received_on DESC, p.receipt_no DESC
          LIMIT $3 OFFSET $4"
    )))
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .fetch_all(tx.conn())
    .await?;
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payments WHERE received_on BETWEEN $1 AND $2")
            .bind(from)
            .bind(to)
            .fetch_one(tx.conn())
            .await?;
    let by_method = sqlx::query_as::<_, MethodTotal>(
        "SELECT method, sum(amount_paise)::bigint AS amount_paise, count(*) AS payments
           FROM payments
          WHERE received_on BETWEEN $1 AND $2 AND voided_at IS NULL
          GROUP BY method
          ORDER BY sum(amount_paise) DESC",
    )
    .bind(from)
    .bind(to)
    .fetch_all(tx.conn())
    .await?;
    let collected_paise = by_method.iter().map(|m| m.amount_paise).sum();
    Ok(Payments {
        from,
        to,
        items,
        total,
        limit,
        offset,
        by_method,
        collected_paise,
    })
}

// ------------------------------------------------------------------ views

async fn student_fees(tx: &mut TenantTx, student_id: Uuid) -> Result<StudentFees> {
    let today = tx.studio_today().await?;
    let (due_paise, overdue_paise, advance_paise) = balance(tx, student_id, today).await?;
    let charges = sqlx::query_as::<_, Charge>(sqlx::AssertSqlSafe(format!(
        "{CHARGE_SELECT}
          WHERE c.student_id = $2
          ORDER BY (c.voided_at IS NOT NULL), c.due_on DESC, c.created_at DESC, c.id
          LIMIT 60"
    )))
    .bind(today)
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;
    let payments = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(format!(
        "{PAYMENT_SELECT}
          WHERE p.student_id = $1
          ORDER BY p.received_on DESC, p.receipt_no DESC
          LIMIT 60"
    )))
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;
    Ok(StudentFees {
        due_paise,
        overdue_paise,
        advance_paise,
        charges,
        payments,
    })
}

/// One student's charges, payments and balance. `NotFound` when they
/// aren't in this studio.
pub async fn for_student(tx: &mut TenantTx, actor: Actor, student_id: Uuid) -> Result<StudentFees> {
    actor.require(Capability::ManageFees)?;
    student_name(tx, student_id).await?;
    ensure_current_month(tx).await?;
    student_fees(tx, student_id).await
}

/// Totals over every student: (due, overdue, how many students owe).
async fn totals(tx: &mut TenantTx, today: Date) -> Result<(i64, i64, i64)> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(o.open), 0)::bigint,
                coalesce(sum(o.open) FILTER (WHERE o.due_on < $1), 0)::bigint,
                count(DISTINCT o.student_id)
           FROM (
             SELECT c.student_id, c.due_on, c.amount_paise - coalesce((
                      SELECT sum(a.amount_paise) FROM payment_allocations a
                       WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open
               FROM charges c
              WHERE c.voided_at IS NULL
           ) o
          WHERE o.open > 0",
    )
    .bind(today)
    .fetch_one(tx.conn())
    .await?;
    Ok(row)
}

/// The numbers at the top of the fees page.
pub async fn summary(tx: &mut TenantTx, actor: Actor) -> Result<FeesSummary> {
    actor.require(Capability::ManageFees)?;
    ensure_current_month(tx).await?;
    let today = tx.studio_today().await?;
    let (fees_from, fee_due_day): (Option<Date>, i16) =
        sqlx::query_as("SELECT fees_from, fee_due_day FROM organizations WHERE id = $1")
            .bind(tx.org_id())
            .fetch_one(tx.conn())
            .await?;
    let (due_paise, overdue_paise, students_owing) = totals(tx, today).await?;
    let collected_this_month_paise = if actor.role.can(Capability::ViewFinancials) {
        let sum: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(amount_paise), 0)::bigint FROM payments
              WHERE voided_at IS NULL AND received_on BETWEEN $1 AND $2",
        )
        .bind(first_of_month(today))
        .bind(today)
        .fetch_one(tx.conn())
        .await?;
        Some(sum)
    } else {
        None
    };
    Ok(FeesSummary {
        fees_from,
        fee_due_day,
        due_paise,
        overdue_paise,
        students_owing,
        collected_this_month_paise,
    })
}

/// For the home page: (still to collect, how many students owe).
pub async fn home_counts(tx: &mut TenantTx) -> Result<(i64, i64)> {
    ensure_current_month(tx).await?;
    let today = tx.studio_today().await?;
    let (due, _, owing) = totals(tx, today).await?;
    Ok((due, owing))
}

/// Students who owe money, the most overdue first.
pub async fn dues(tx: &mut TenantTx, actor: Actor, query: DuesQuery) -> Result<Dues> {
    actor.require(Capability::ManageFees)?;
    ensure_current_month(tx).await?;
    let today = tx.studio_today().await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    const FROM: &str = "
          FROM students s
          LEFT JOIN branches br ON br.org_id = s.org_id AND br.id = s.branch_id
          JOIN LATERAL (
            SELECT sum(o.open)::bigint AS due_paise,
                   coalesce(sum(o.open) FILTER (WHERE o.due_on < $1), 0)::bigint AS overdue_paise,
                   min(o.due_on) AS oldest_due_on
              FROM (
                SELECT c.due_on, c.amount_paise - coalesce((
                         SELECT sum(a.amount_paise) FROM payment_allocations a
                          WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open
                  FROM charges c
                 WHERE c.org_id = s.org_id AND c.student_id = s.id AND c.voided_at IS NULL
              ) o
             WHERE o.open > 0
          ) d ON d.due_paise > 0
         WHERE ($2::uuid IS NULL OR s.branch_id = $2)";
    let items = sqlx::query_as::<_, DueRow>(sqlx::AssertSqlSafe(format!(
        "SELECT s.id AS student_id, s.name AS student_name, br.name AS branch_name,
                d.due_paise, d.overdue_paise, d.oldest_due_on
         {FROM}
         ORDER BY d.overdue_paise DESC, d.oldest_due_on, lower(s.name)
         LIMIT $3 OFFSET $4"
    )))
    .bind(today)
    .bind(query.branch_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(tx.conn())
    .await?;
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {FROM}")))
        .bind(today)
        .bind(query.branch_id)
        .fetch_one(tx.conn())
        .await?;
    Ok(Dues {
        items,
        total,
        limit,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn months_are_named_and_start_on_the_first() {
        assert_eq!(first_of_month(date!(2026 - 11 - 23)), date!(2026 - 11 - 01));
        assert_eq!(month_label(date!(2026 - 12 - 01)), "December 2026");
    }

    #[test]
    fn amounts_must_be_sensible() {
        assert!(amount(0, "The amount").is_err());
        assert!(amount(-500, "The amount").is_err());
        assert!(amount(MAX_AMOUNT_PAISE + 1, "The amount").is_err());
        assert_eq!(amount(150_000, "The amount").unwrap(), 150_000);
    }
}
