//! Putting students in classes, and taking them out.
//!
//! An enrollment says "this student is a regular in this class from
//! `starts_on`". Leaving sets `ended_on` (the first day they're no longer
//! in it); rows are never deleted, so history stays.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, Duration};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::batches::Slot;
use crate::db::TenantTx;
use crate::plans::PlanKind;
use crate::students::StudentStatus;
use crate::{Error, Result};

/// A student's class, as their profile shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct StudentEnrollment {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub batch_name: String,
    pub style: Option<String>,
    pub branch_name: String,
    pub plan_id: Option<Uuid>,
    pub plan_name: Option<String>,
    pub plan_price_paise: Option<i64>,
    pub starts_on: Date,
    pub ended_on: Option<Date>,
    #[sqlx(skip)]
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewEnrollment {
    pub batch_id: Uuid,
    /// A monthly plan, or `None` to pay with class credits.
    pub plan_id: Option<Uuid>,
    /// `None` means today.
    pub starts_on: Option<Date>,
}

/// Everything a student is in now, then what they left (newest first).
pub async fn for_student(tx: &mut TenantTx, student_id: Uuid) -> Result<Vec<StudentEnrollment>> {
    let mut rows = sqlx::query_as::<_, StudentEnrollment>(
        "SELECT e.id, e.batch_id, b.name AS batch_name, b.style, br.name AS branch_name,
                e.plan_id, p.name AS plan_name, p.price_paise AS plan_price_paise,
                e.starts_on, e.ended_on
           FROM enrollments e
           JOIN batches b ON b.org_id = e.org_id AND b.id = e.batch_id
           JOIN branches br ON br.org_id = b.org_id AND br.id = b.branch_id
           LEFT JOIN plans p ON p.org_id = e.org_id AND p.id = e.plan_id
          WHERE e.student_id = $1
          ORDER BY e.ended_on IS NOT NULL, e.ended_on DESC, lower(b.name)
          LIMIT 50",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.batch_id).collect();
    let slots: Vec<(Uuid, i16, time::Time, time::Time)> = sqlx::query_as(
        "SELECT batch_id, weekday, starts_at, ends_at FROM batch_slots
          WHERE batch_id = ANY($1) ORDER BY weekday, starts_at",
    )
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    for (batch_id, weekday, starts_at, ends_at) in slots {
        for r in rows.iter_mut().filter(|r| r.batch_id == batch_id) {
            r.slots.push(Slot {
                weekday,
                starts_at,
                ends_at,
            });
        }
    }
    Ok(rows)
}

/// Put a student in a class. Refuses a full class, a class they're already
/// in, and students who are on a break or have left.
pub async fn enroll(
    tx: &mut TenantTx,
    actor: Actor,
    student_id: Uuid,
    new: NewEnrollment,
) -> Result<Vec<StudentEnrollment>> {
    actor.require(Capability::ManageStudents)?;
    let (student_name, status): (String, StudentStatus) =
        sqlx::query_as("SELECT name, status FROM students WHERE id = $1")
            .bind(student_id)
            .fetch_one(tx.conn())
            .await?;
    match status {
        StudentStatus::Active => {}
        StudentStatus::Paused => {
            return Err(Error::Invalid(format!(
                "{student_name} is on a break. Set them as active first."
            )));
        }
        StudentStatus::Left => {
            return Err(Error::Invalid(format!(
                "{student_name} has left. Set them as active first."
            )));
        }
    }

    // Lock the class row: two enrollments for the last place, saved at the
    // same moment, must not both get in.
    let batch: Option<(String, Option<i32>, bool, Option<Date>)> = sqlx::query_as(
        "SELECT name, capacity, active, ends_on FROM batches WHERE id = $1 FOR UPDATE",
    )
    .bind(new.batch_id)
    .fetch_optional(tx.conn())
    .await?;
    let Some((batch_name, capacity, active, batch_ends_on)) = batch else {
        return Err(Error::Invalid("Choose one of your classes.".into()));
    };
    if !active {
        return Err(Error::Invalid(format!("{batch_name} is archived.")));
    }

    let today = tx.studio_today().await?;
    let starts_on = new.starts_on.unwrap_or(today);
    if starts_on < today - Duration::days(366) || starts_on > today + Duration::days(366) {
        return Err(Error::Invalid(
            "The start date has to be within a year of today.".into(),
        ));
    }
    if batch_ends_on.is_some_and(|end| end < starts_on) {
        return Err(Error::Invalid(format!(
            "{batch_name} has finished by then."
        )));
    }

    if let Some(plan_id) = new.plan_id {
        let plan: Option<(PlanKind, bool)> =
            sqlx::query_as("SELECT kind, active FROM plans WHERE id = $1")
                .bind(plan_id)
                .fetch_optional(tx.conn())
                .await?;
        match plan {
            None => return Err(Error::Invalid("Choose one of your plans.".into())),
            Some((PlanKind::Monthly, true)) => {}
            Some((PlanKind::Monthly, false)) => {
                return Err(Error::Invalid("That plan is no longer offered.".into()));
            }
            Some(_) => {
                return Err(Error::Invalid(
                    "Packs and drop-ins are added as class credits. Choose a monthly plan, or none to pay with credits.".into(),
                ));
            }
        }
    }

    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM enrollments
                         WHERE student_id = $1 AND batch_id = $2 AND ended_on IS NULL)",
    )
    .bind(student_id)
    .bind(new.batch_id)
    .fetch_one(tx.conn())
    .await?;
    if already {
        return Err(Error::Conflict(format!(
            "{student_name} is already in {batch_name}."
        )));
    }

    if let Some(cap) = capacity {
        let enrolled: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM enrollments WHERE batch_id = $1 AND ended_on IS NULL",
        )
        .bind(new.batch_id)
        .fetch_one(tx.conn())
        .await?;
        if enrolled >= i64::from(cap) {
            return Err(Error::Conflict(format!(
                "{batch_name} is full ({enrolled} of {cap})."
            )));
        }
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO enrollments (org_id, student_id, batch_id, plan_id, starts_on, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(student_id)
    .bind(new.batch_id)
    .bind(new.plan_id)
    .bind(starts_on)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "enrollment.created".into(),
            entity: "enrollment".into(),
            entity_id: Some(id),
            detail: json!({
                "student_id": student_id,
                "batch_id": new.batch_id,
                "plan_id": new.plan_id,
            }),
        },
    )
    .await?;
    for_student(tx, student_id).await
}

/// Take a student out of a class from today. Returns the student's classes.
pub async fn end(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<Vec<StudentEnrollment>> {
    actor.require(Capability::ManageStudents)?;
    let (student_id, ended_on): (Uuid, Option<Date>) =
        sqlx::query_as("SELECT student_id, ended_on FROM enrollments WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    if ended_on.is_some() {
        return Err(Error::Conflict("They've already left that class.".into()));
    }
    let today = tx.studio_today().await?;
    // Leaving before the start date makes the enrollment zero days long.
    sqlx::query("UPDATE enrollments SET ended_on = greatest($2, starts_on) WHERE id = $1")
        .bind(id)
        .bind(today)
        .execute(tx.conn())
        .await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "enrollment.ended".into(),
            entity: "enrollment".into(),
            entity_id: Some(id),
            detail: json!({ "student_id": student_id }),
        },
    )
    .await?;
    for_student(tx, student_id).await
}

/// Take a student out of every class (used when they leave the studio).
/// Returns how many classes they were in.
pub(crate) async fn end_all_for_student(tx: &mut TenantTx, student_id: Uuid) -> Result<u64> {
    let today = tx.studio_today().await?;
    let n = sqlx::query(
        "UPDATE enrollments SET ended_on = greatest($2, starts_on)
          WHERE student_id = $1 AND ended_on IS NULL",
    )
    .bind(student_id)
    .bind(today)
    .execute(tx.conn())
    .await?
    .rows_affected();
    Ok(n)
}
