//! Classes that meet every week. The dashboard calls them classes; the
//! database calls them batches, because "class" is also one session.
//!
//! A batch belongs to a branch, may use a room and have a usual instructor,
//! and meets in one or more weekly slots ("Monday 18:00–19:00"). Slot times
//! are the branch's wall-clock time.
//!
//! Two rules keep the timetable possible:
//! - a room can't hold two classes at once;
//! - an instructor can't teach two classes at once.
//!
//! Checking them is "read, then write", which two people saving at the same
//! moment could both pass. Every schedule change therefore takes a
//! per-studio lock first (`TenantTx::lock_studio("schedule")`), so they run
//! one after another. Schedule edits are rare, so the wait is never felt.

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, OffsetDateTime, Time};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::{validate, Error, Result};

time::serde::format_description!(hhmm, Time, "[hour]:[minute]");

/// Most weekly slots one class can have.
pub const MAX_SLOTS: usize = 14;

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// One weekly meeting. Times travel as "18:00".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Slot {
    /// 1 = Monday … 7 = Sunday.
    pub weekday: i16,
    #[serde(with = "hhmm")]
    pub starts_at: Time,
    #[serde(with = "hhmm")]
    pub ends_at: Time,
}

impl Slot {
    fn describe(&self) -> String {
        format!(
            "{} {}–{}",
            WEEKDAYS[(self.weekday - 1) as usize],
            clock(self.starts_at),
            clock(self.ends_at)
        )
    }
}

fn clock(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Batch {
    pub id: Uuid,
    pub org_id: Uuid,
    pub branch_id: Uuid,
    pub room_id: Option<Uuid>,
    pub instructor_id: Option<Uuid>,
    pub name: String,
    pub style: Option<String>,
    pub capacity: Option<i32>,
    pub starts_on: Date,
    pub ends_on: Option<Date>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// A class with the names and numbers the timetable shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct BatchSummary {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub batch: Batch,
    pub branch_name: String,
    pub room_name: Option<String>,
    pub instructor_name: Option<String>,
    /// Students currently in the class (or starting soon).
    pub enrolled: i64,
    #[sqlx(skip)]
    pub slots: Vec<Slot>,
}

/// One student in a class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RosterEntry {
    pub enrollment_id: Uuid,
    pub student_id: Uuid,
    pub student_name: String,
    pub student_status: crate::students::StudentStatus,
    pub plan_id: Option<Uuid>,
    pub plan_name: Option<String>,
    pub starts_on: Date,
    /// The studio's date when they were put in the class; a `starts_on`
    /// earlier than this was backdated.
    pub entered_on: Date,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchDetail {
    #[serde(flatten)]
    pub summary: BatchSummary,
    /// Who is in the class. `None` when the person asking may not see it:
    /// only staff who manage students, and the class's own instructor, can.
    pub roster: Option<Vec<RosterEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewBatch {
    pub name: String,
    pub style: Option<String>,
    pub branch_id: Uuid,
    pub room_id: Option<Uuid>,
    pub instructor_id: Option<Uuid>,
    /// `None` takes the room's capacity, or no limit without a room.
    pub capacity: Option<i32>,
    /// `None` means today.
    pub starts_on: Option<Date>,
    pub ends_on: Option<Date>,
    pub slots: Vec<Slot>,
}

/// Everything editable about a class, sent whole. Setting `active` to
/// false archives the class and takes every student out of it.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchEdit {
    pub name: String,
    pub style: Option<String>,
    pub branch_id: Uuid,
    pub room_id: Option<Uuid>,
    pub instructor_id: Option<Uuid>,
    pub capacity: Option<i32>,
    pub starts_on: Date,
    pub ends_on: Option<Date>,
    pub active: bool,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub branch_id: Option<Uuid>,
    /// Include archived classes.
    #[serde(default)]
    pub archived: bool,
}

/// The cleaned fields shared by create and update.
struct Checked {
    name: String,
    style: Option<String>,
    capacity: Option<i32>,
    starts_on: Date,
    slots: Vec<Slot>,
}

const SUMMARY_SELECT: &str = "
    SELECT b.id, b.org_id, b.branch_id, b.room_id, b.instructor_id, b.name, b.style,
           b.capacity, b.starts_on, b.ends_on, b.active, b.created_at, b.updated_at,
           br.name AS branch_name, r.name AS room_name, st.name AS instructor_name,
           (SELECT count(*) FROM enrollments e
             WHERE e.org_id = b.org_id AND e.batch_id = b.id AND e.ended_on IS NULL) AS enrolled
      FROM batches b
      JOIN branches br ON br.org_id = b.org_id AND br.id = b.branch_id
      LEFT JOIN rooms r ON r.org_id = b.org_id AND r.id = b.room_id
      LEFT JOIN staff st ON st.org_id = b.org_id AND st.id = b.instructor_id";

/// Classes for the timetable, with their slots.
pub async fn list(tx: &mut TenantTx, query: ListQuery) -> Result<Vec<BatchSummary>> {
    let mut batches = sqlx::query_as::<_, BatchSummary>(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT}
          WHERE ($1::uuid IS NULL OR b.branch_id = $1)
            AND ($2 OR b.active)
          ORDER BY b.active DESC, lower(b.name)"
    )))
    .bind(query.branch_id)
    .bind(query.archived)
    .fetch_all(tx.conn())
    .await?;
    attach_slots(tx, &mut batches).await?;
    Ok(batches)
}

pub async fn summary(tx: &mut TenantTx, id: Uuid) -> Result<BatchSummary> {
    let mut found = vec![
        sqlx::query_as::<_, BatchSummary>(sqlx::AssertSqlSafe(format!(
            "{SUMMARY_SELECT} WHERE b.id = $1"
        )))
        .bind(id)
        .fetch_one(tx.conn())
        .await?,
    ];
    attach_slots(tx, &mut found).await?;
    Ok(found.remove(0))
}

/// A class with its roster when `actor` may see it.
pub async fn get(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<BatchDetail> {
    actor.require(Capability::ViewStudio)?;
    let summary = summary(tx, id).await?;
    let may_see_roster = actor.role.can(Capability::ManageStudents)
        || summary.batch.instructor_id == Some(actor.staff_id);
    let roster = if may_see_roster {
        Some(roster(tx, id).await?)
    } else {
        None
    };
    Ok(BatchDetail { summary, roster })
}

pub async fn count_active(tx: &mut TenantTx) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM batches WHERE active")
        .fetch_one(tx.conn())
        .await?;
    Ok(n)
}

pub async fn create(tx: &mut TenantTx, actor: Actor, new: NewBatch) -> Result<BatchDetail> {
    actor.require(Capability::ManageClasses)?;
    tx.lock_studio("schedule").await?;
    let today = tx.studio_today().await?;
    let checked = check(
        tx,
        None,
        Fields {
            name: &new.name,
            style: new.style,
            branch_id: new.branch_id,
            room_id: new.room_id,
            instructor_id: new.instructor_id,
            capacity: new.capacity,
            starts_on: new.starts_on.unwrap_or(today),
            ends_on: new.ends_on,
            active: true,
            slots: new.slots,
        },
    )
    .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO batches
            (org_id, branch_id, room_id, instructor_id, name, style, capacity, starts_on, ends_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(new.branch_id)
    .bind(new.room_id)
    .bind(new.instructor_id)
    .bind(&checked.name)
    .bind(&checked.style)
    .bind(checked.capacity)
    .bind(checked.starts_on)
    .bind(new.ends_on)
    .fetch_one(tx.conn())
    .await?;
    write_slots(tx, id, &checked.slots).await?;

    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "class.created".into(),
            entity: "batch".into(),
            entity_id: Some(id),
            detail: json!({ "name": checked.name, "slots": checked.slots.len() }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

pub async fn update(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    edit: BatchEdit,
) -> Result<BatchDetail> {
    actor.require(Capability::ManageClasses)?;
    tx.lock_studio("schedule").await?;
    // Lock the class row too, so an enrollment can't slip in while the
    // capacity is being lowered.
    let before = sqlx::query_as::<_, Batch>(
        "SELECT id, org_id, branch_id, room_id, instructor_id, name, style, capacity,
                starts_on, ends_on, active, created_at, updated_at
           FROM batches WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;

    let checked = check(
        tx,
        Some(id),
        Fields {
            name: &edit.name,
            style: edit.style,
            branch_id: edit.branch_id,
            room_id: edit.room_id,
            instructor_id: edit.instructor_id,
            capacity: edit.capacity,
            starts_on: edit.starts_on,
            ends_on: edit.ends_on,
            active: edit.active,
            slots: edit.slots,
        },
    )
    .await?;

    let enrolled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM enrollments WHERE batch_id = $1 AND ended_on IS NULL",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    if edit.active {
        if let Some(cap) = checked.capacity {
            if i64::from(cap) < enrolled {
                return Err(Error::Invalid(format!(
                    "{enrolled} students are in this class, so its capacity can't be lower than that."
                )));
            }
        }
        if edit.branch_id != before.branch_id && enrolled > 0 {
            return Err(Error::Invalid(
                "Students are in this class. Create a new class at the other branch instead of moving this one.".into(),
            ));
        }
    }

    sqlx::query(
        "UPDATE batches
            SET branch_id = $2, room_id = $3, instructor_id = $4, name = $5, style = $6,
                capacity = $7, starts_on = $8, ends_on = $9, active = $10, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(edit.branch_id)
    .bind(edit.room_id)
    .bind(edit.instructor_id)
    .bind(&checked.name)
    .bind(&checked.style)
    .bind(checked.capacity)
    .bind(checked.starts_on)
    .bind(edit.ends_on)
    .bind(edit.active)
    .execute(tx.conn())
    .await?;
    sqlx::query("DELETE FROM batch_slots WHERE batch_id = $1")
        .bind(id)
        .execute(tx.conn())
        .await?;
    write_slots(tx, id, &checked.slots).await?;

    // Archiving takes everyone out, from today.
    let mut students_removed = 0;
    if before.active && !edit.active {
        let today = tx.studio_today().await?;
        students_removed = sqlx::query(
            "UPDATE enrollments SET ended_on = greatest($2, starts_on)
              WHERE batch_id = $1 AND ended_on IS NULL",
        )
        .bind(id)
        .bind(today)
        .execute(tx.conn())
        .await?
        .rows_affected();
    }

    let mut detail = serde_json::Map::new();
    detail.insert("name".into(), json!(checked.name));
    if before.active != edit.active {
        detail.insert(
            "active".into(),
            json!({ "from": before.active, "to": edit.active }),
        );
    }
    if students_removed > 0 {
        detail.insert("students_removed".into(), json!(students_removed));
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "class.updated".into(),
            entity: "batch".into(),
            entity_id: Some(id),
            detail: serde_json::Value::Object(detail),
        },
    )
    .await?;
    get(tx, actor, id).await
}

struct Fields<'a> {
    name: &'a str,
    style: Option<String>,
    branch_id: Uuid,
    room_id: Option<Uuid>,
    instructor_id: Option<Uuid>,
    capacity: Option<i32>,
    starts_on: Date,
    ends_on: Option<Date>,
    active: bool,
    slots: Vec<Slot>,
}

/// Validate a class, fill in defaults, and refuse clashes.
async fn check(tx: &mut TenantTx, id: Option<Uuid>, f: Fields<'_>) -> Result<Checked> {
    let name = validate::required(f.name, 80, "Class names")?;
    let style = validate::optional(f.style, 40, "Dance style")?;
    let mut capacity = validate::range(f.capacity, 1, 500, "Class capacity")?;
    if f.ends_on.is_some_and(|end| end < f.starts_on) {
        return Err(Error::Invalid(
            "The last day can't be before the first day.".into(),
        ));
    }
    let slots = check_slots(f.slots)?;

    // The branch, room and instructor must be this studio's. Checked here
    // for clear messages; the composite foreign keys refuse it anyway.
    let branch: Option<bool> = sqlx::query_scalar("SELECT active FROM branches WHERE id = $1")
        .bind(f.branch_id)
        .fetch_optional(tx.conn())
        .await?;
    match branch {
        None => return Err(Error::Invalid("Choose one of your branches.".into())),
        Some(false) if f.active => {
            return Err(Error::Invalid("That branch is closed.".into()));
        }
        _ => {}
    }

    let mut room_name = None;
    if let Some(room_id) = f.room_id {
        let room: Option<(Uuid, String, Option<i32>, bool)> =
            sqlx::query_as("SELECT branch_id, name, capacity, active FROM rooms WHERE id = $1")
                .bind(room_id)
                .fetch_optional(tx.conn())
                .await?;
        let Some((room_branch, rname, room_capacity, room_active)) = room else {
            return Err(Error::Invalid("Choose one of your rooms.".into()));
        };
        if room_branch != f.branch_id {
            return Err(Error::Invalid(format!("{rname} is in a different branch.")));
        }
        if !room_active && f.active {
            return Err(Error::Invalid(format!("{rname} is closed.")));
        }
        match (capacity, room_capacity) {
            (None, Some(rc)) => capacity = Some(rc.min(500)),
            (Some(c), Some(rc)) if c > rc => {
                return Err(Error::Invalid(format!(
                    "{rname} holds {rc} people, so the class can't take more than that."
                )));
            }
            _ => {}
        }
        room_name = Some(rname);
    }

    if let Some(instructor_id) = f.instructor_id {
        let person: Option<(String, bool)> =
            sqlx::query_as("SELECT name, active FROM staff WHERE id = $1")
                .bind(instructor_id)
                .fetch_optional(tx.conn())
                .await?;
        match person {
            None => return Err(Error::Invalid("Choose someone on your team.".into())),
            Some((pname, false)) if f.active => {
                return Err(Error::Invalid(format!("{pname} is deactivated.")));
            }
            _ => {}
        }
    }

    if f.active && (f.room_id.is_some() || f.instructor_id.is_some()) {
        if let Some(message) = clash(
            tx,
            id,
            f.room_id,
            f.instructor_id,
            f.starts_on,
            f.ends_on,
            &slots,
            room_name.as_deref(),
        )
        .await?
        {
            return Err(Error::Conflict(message));
        }
    }

    Ok(Checked {
        name,
        style,
        capacity,
        starts_on: f.starts_on,
        slots,
    })
}

/// Sort slots and refuse impossible ones, including two that overlap on
/// the same day.
fn check_slots(mut slots: Vec<Slot>) -> Result<Vec<Slot>> {
    if slots.is_empty() {
        return Err(Error::Invalid(
            "Add at least one day and time the class meets.".into(),
        ));
    }
    if slots.len() > MAX_SLOTS {
        return Err(Error::Invalid(format!(
            "A class can meet at most {MAX_SLOTS} times a week."
        )));
    }
    for s in &slots {
        if !(1..=7).contains(&s.weekday) {
            return Err(Error::Invalid("Choose a day of the week.".into()));
        }
        if s.ends_at <= s.starts_at {
            return Err(Error::Invalid(format!(
                "On {}, the class has to end after it starts.",
                WEEKDAYS[(s.weekday - 1) as usize]
            )));
        }
        if s.ends_at - s.starts_at > time::Duration::hours(6) {
            return Err(Error::Invalid(
                "A class can be at most 6 hours long.".into(),
            ));
        }
    }
    slots.sort_by_key(|s| (s.weekday, s.starts_at));
    for pair in slots.windows(2) {
        if pair[0].weekday == pair[1].weekday && pair[1].starts_at < pair[0].ends_at {
            return Err(Error::Invalid(format!(
                "{} overlaps {}.",
                pair[1].describe(),
                pair[0].describe()
            )));
        }
    }
    Ok(slots)
}

/// The first clash with another active class, as a sentence, if any.
#[allow(clippy::too_many_arguments)]
async fn clash(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    room_id: Option<Uuid>,
    instructor_id: Option<Uuid>,
    starts_on: Date,
    ends_on: Option<Date>,
    slots: &[Slot],
    room_name: Option<&str>,
) -> Result<Option<String>> {
    let weekdays: Vec<i16> = slots.iter().map(|s| s.weekday).collect();
    let starts: Vec<Time> = slots.iter().map(|s| s.starts_at).collect();
    let ends: Vec<Time> = slots.iter().map(|s| s.ends_at).collect();

    // Two time ranges overlap when each starts before the other ends.
    // Two date ranges overlap per Postgres's `&&` on daterange; a NULL end
    // means "no end".
    let found: Option<(String, bool, Option<String>, i16, Time, Time)> = sqlx::query_as(
        "SELECT b.name,
                ($2::uuid IS NOT NULL AND b.room_id = $2) AS same_room,
                st.name,
                s.weekday, s.starts_at, s.ends_at
           FROM batch_slots s
           JOIN batches b ON b.org_id = s.org_id AND b.id = s.batch_id
           LEFT JOIN staff st ON st.org_id = b.org_id AND st.id = b.instructor_id
           JOIN unnest($5::int2[], $6::time[], $7::time[]) AS c(weekday, starts_at, ends_at)
             ON c.weekday = s.weekday AND s.starts_at < c.ends_at AND c.starts_at < s.ends_at
          WHERE b.active
            AND b.id IS DISTINCT FROM $1
            AND (($2::uuid IS NOT NULL AND b.room_id = $2)
                 OR ($3::uuid IS NOT NULL AND b.instructor_id = $3))
            AND daterange(b.starts_on, b.ends_on, '[]') && daterange($4::date, $8::date, '[]')
          ORDER BY same_room DESC, s.weekday, s.starts_at
          LIMIT 1",
    )
    .bind(id)
    .bind(room_id)
    .bind(instructor_id)
    .bind(starts_on)
    .bind(&weekdays)
    .bind(&starts)
    .bind(&ends)
    .bind(ends_on)
    .fetch_optional(tx.conn())
    .await?;

    Ok(
        found.map(|(other, same_room, teacher, weekday, starts_at, ends_at)| {
            let when = Slot {
                weekday,
                starts_at,
                ends_at,
            }
            .describe();
            if same_room {
                format!(
                    "{} is booked for {other} on {when}.",
                    room_name.unwrap_or("That room")
                )
            } else {
                format!(
                    "{} teaches {other} on {when}.",
                    teacher.as_deref().unwrap_or("That instructor")
                )
            }
        }),
    )
}

async fn write_slots(tx: &mut TenantTx, batch_id: Uuid, slots: &[Slot]) -> Result<()> {
    let weekdays: Vec<i16> = slots.iter().map(|s| s.weekday).collect();
    let starts: Vec<Time> = slots.iter().map(|s| s.starts_at).collect();
    let ends: Vec<Time> = slots.iter().map(|s| s.ends_at).collect();
    sqlx::query(
        "INSERT INTO batch_slots (org_id, batch_id, weekday, starts_at, ends_at)
         SELECT $1, $2, w, s, e FROM unnest($3::int2[], $4::time[], $5::time[]) AS t(w, s, e)",
    )
    .bind(tx.org_id())
    .bind(batch_id)
    .bind(&weekdays)
    .bind(&starts)
    .bind(&ends)
    .execute(tx.conn())
    .await?;
    Ok(())
}

async fn attach_slots(tx: &mut TenantTx, batches: &mut [BatchSummary]) -> Result<()> {
    if batches.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = batches.iter().map(|b| b.batch.id).collect();
    let rows: Vec<(Uuid, i16, Time, Time)> = sqlx::query_as(
        "SELECT batch_id, weekday, starts_at, ends_at FROM batch_slots
          WHERE batch_id = ANY($1) ORDER BY weekday, starts_at",
    )
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    for (batch_id, weekday, starts_at, ends_at) in rows {
        if let Some(b) = batches.iter_mut().find(|b| b.batch.id == batch_id) {
            b.slots.push(Slot {
                weekday,
                starts_at,
                ends_at,
            });
        }
    }
    Ok(())
}

async fn roster(tx: &mut TenantTx, batch_id: Uuid) -> Result<Vec<RosterEntry>> {
    let rows = sqlx::query_as::<_, RosterEntry>(
        "SELECT e.id AS enrollment_id, s.id AS student_id, s.name AS student_name,
                s.status AS student_status, e.plan_id, p.name AS plan_name, e.starts_on,
                studio_date(e.org_id, e.created_at) AS entered_on
           FROM enrollments e
           JOIN students s ON s.org_id = e.org_id AND s.id = e.student_id
           LEFT JOIN plans p ON p.org_id = e.org_id AND p.id = e.plan_id
          WHERE e.batch_id = $1 AND e.ended_on IS NULL
          ORDER BY lower(s.name)",
    )
    .bind(batch_id)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::time;

    fn slot(weekday: i16, from: Time, to: Time) -> Slot {
        Slot {
            weekday,
            starts_at: from,
            ends_at: to,
        }
    }

    #[test]
    fn slots_are_sorted_and_checked() {
        let sorted = check_slots(vec![
            slot(3, time!(18:00), time!(19:00)),
            slot(1, time!(18:00), time!(19:00)),
        ])
        .unwrap();
        assert_eq!(sorted[0].weekday, 1);

        assert!(check_slots(vec![]).is_err());
        assert!(check_slots(vec![slot(8, time!(18:00), time!(19:00))]).is_err());
        assert!(check_slots(vec![slot(1, time!(19:00), time!(18:00))]).is_err());
        assert!(check_slots(vec![slot(1, time!(10:00), time!(16:01))]).is_err());
        let overlap = check_slots(vec![
            slot(2, time!(18:00), time!(19:00)),
            slot(2, time!(18:30), time!(19:30)),
        ])
        .unwrap_err();
        assert!(overlap
            .to_string()
            .contains("Tuesday 18:30–19:30 overlaps Tuesday 18:00–19:00"));
        // Back to back is fine.
        assert!(check_slots(vec![
            slot(2, time!(18:00), time!(19:00)),
            slot(2, time!(19:00), time!(20:00)),
        ])
        .is_ok());
    }

    #[test]
    fn times_travel_as_hours_and_minutes() {
        let s: Slot =
            serde_json::from_str(r#"{"weekday":1,"starts_at":"06:30","ends_at":"18:05"}"#).unwrap();
        assert_eq!(s.starts_at, time!(6:30));
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"weekday":1,"starts_at":"06:30","ends_at":"18:05"}"#
        );
        assert!(serde_json::from_str::<Slot>(
            r#"{"weekday":1,"starts_at":"25:00","ends_at":"26:00"}"#
        )
        .is_err());
    }
}
