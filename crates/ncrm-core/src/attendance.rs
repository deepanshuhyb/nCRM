//! Attendance: who came to each class meeting, and whether the teacher did.
//!
//! A *session* is one meeting of a class on one date. Sessions are created
//! on demand from the weekly slots, only for today and the past, so editing
//! a class's schedule never leaves wrong future sessions behind.
//!
//! Who may mark: staff with "mark any attendance" (owner, manager, front
//! desk), and an instructor for a session they teach (as the class's usual
//! teacher or that day's substitute).
//!
//! Credits: a student who pays with class credits (no monthly plan on the
//! enrollment) or who drops in spends one credit when marked present or
//! late. Changing the mark gives the credit back to the same lot. With no
//! credit left the mark is still saved, flagged `owes_credit`, and settled
//! automatically by the next pack they buy (`settle_owed`).

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, Duration, Time};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::credits;
use crate::db::TenantTx;
use crate::{validate, Error, Result};

time::serde::format_description!(hhmm, Time, "[hour]:[minute]");

/// How far back the day view and marking go.
pub const MAX_DAYS_BACK: i64 = 90;
/// Instructors can correct their own classes this many days back; older
/// corrections need the front desk.
pub const INSTRUCTOR_DAYS_BACK: i64 = 7;
/// Alerts look at this many days.
const ALERT_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "attendance_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    Present,
    Late,
    Absent,
    /// Away with notice. Doesn't count towards absence streaks.
    Excused,
}

impl AttendanceStatus {
    /// Came to the class (and so uses a credit).
    pub fn attended(self) -> bool {
        matches!(self, AttendanceStatus::Present | AttendanceStatus::Late)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "session_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Scheduled,
    Cancelled,
}

/// A session with the names and counts a day view shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SessionSummary {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub batch_name: String,
    pub branch_id: Uuid,
    pub branch_name: String,
    pub room_name: Option<String>,
    pub on_date: Date,
    #[serde(with = "hhmm")]
    pub starts_at: Time,
    #[serde(with = "hhmm")]
    pub ends_at: Time,
    pub status: SessionStatus,
    pub cancel_reason: Option<String>,
    pub instructor_id: Option<Uuid>,
    pub instructor_name: Option<String>,
    /// `None` until the teacher's attendance is marked.
    pub teacher_present: Option<bool>,
    /// Members expected (not counting students on a break).
    pub expected: i64,
    /// Came: present or late, drop-ins included.
    pub came: i64,
    /// Marked absent or excused.
    pub away: i64,
    /// Expected students nobody has marked yet.
    pub unmarked: i64,
    /// The class is over, by the studio's clock.
    pub ended: bool,
}

/// One student on a session's roster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RosterMark {
    pub student_id: Uuid,
    pub student_name: String,
    pub status: Option<AttendanceStatus>,
    pub drop_in: bool,
    /// Coming to this class spends a class credit.
    pub pays_with_credits: bool,
    /// Credits usable on the session's date.
    pub credits_left: i32,
    pub owes_credit: bool,
    /// Absences in a row in this class before this session.
    pub absences_in_a_row: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionDetail {
    #[serde(flatten)]
    pub summary: SessionSummary,
    /// Whether the person asking may mark this session.
    pub can_mark: bool,
    /// `None` when they may not see who's in the class.
    pub roster: Option<Vec<RosterMark>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mark {
    pub student_id: Uuid,
    pub status: AttendanceStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Marks {
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DropIn {
    pub student_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeacherMark {
    pub present: bool,
    /// Who taught, when it wasn't the usual teacher. `None` keeps whoever
    /// is set.
    #[serde(default)]
    pub instructor_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cancel {
    pub reason: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DayQuery {
    /// `None` means today.
    pub date: Option<Date>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Streak {
    pub student_id: Uuid,
    pub student_name: String,
    pub batch_id: Uuid,
    pub batch_name: String,
    pub absences: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alerts {
    /// Classes in the last week that are over and not fully marked.
    pub unmarked: Vec<SessionSummary>,
    /// Students absent three or more times in a row.
    pub streaks: Vec<Streak>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct PastMark {
    pub on_date: Date,
    pub batch_name: String,
    pub status: AttendanceStatus,
    pub drop_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StudentAttendance {
    /// In the last 30 days.
    pub came: i64,
    pub missed: i64,
    pub recent: Vec<PastMark>,
}

// --------------------------------------------------------------- sessions

/// Create the sessions the weekly timetable implies for `from..=to`, for
/// dates up to today. Safe to call any number of times.
pub async fn ensure(tx: &mut TenantTx, from: Date, to: Date) -> Result<()> {
    let today = tx.studio_today().await?;
    let to = to.min(today);
    if from > to {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO sessions (org_id, batch_id, on_date, starts_at, ends_at, instructor_id)
         SELECT b.org_id, b.id, d::date, s.starts_at, s.ends_at, b.instructor_id
           FROM generate_series($1::date, $2::date, interval '1 day') AS d
           JOIN batch_slots s ON s.weekday = extract(isodow FROM d)::int
           JOIN batches b ON b.org_id = s.org_id AND b.id = s.batch_id
          WHERE b.active
            AND b.starts_on <= d::date
            AND (b.ends_on IS NULL OR b.ends_on >= d::date)
         ON CONFLICT (org_id, batch_id, on_date, starts_at) DO NOTHING",
    )
    .bind(from)
    .bind(to)
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Remove a class's untouched sessions from today on, after its schedule
/// changed, so they're created again from the new slots.
pub(crate) async fn forget_untouched(tx: &mut TenantTx, batch_id: Uuid) -> Result<()> {
    let today = tx.studio_today().await?;
    sqlx::query(
        "DELETE FROM sessions s
          WHERE s.batch_id = $1 AND s.on_date >= $2
            AND s.status = 'scheduled' AND s.teacher_present IS NULL
            AND NOT EXISTS (SELECT 1 FROM attendance a
                             WHERE a.org_id = s.org_id AND a.session_id = s.id)",
    )
    .bind(batch_id)
    .bind(today)
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Members on the session's date who aren't on a break, as a SQL fragment
/// over session alias `s`. `$1` is today's date, `$2` the time now.
const SUMMARY_SELECT: &str = "
    SELECT s.id, s.batch_id, b.name AS batch_name, b.branch_id, br.name AS branch_name,
           r.name AS room_name, s.on_date, s.starts_at, s.ends_at, s.status, s.cancel_reason,
           s.instructor_id, st.name AS instructor_name, s.teacher_present,
           m.expected, k.came, k.away, m.unmarked,
           (s.on_date < $1 OR s.ends_at <= $2) AS ended
      FROM sessions s
      JOIN batches b ON b.org_id = s.org_id AND b.id = s.batch_id
      JOIN branches br ON br.org_id = b.org_id AND br.id = b.branch_id
      LEFT JOIN rooms r ON r.org_id = b.org_id AND r.id = b.room_id
      LEFT JOIN staff st ON st.org_id = s.org_id AND st.id = s.instructor_id
      CROSS JOIN LATERAL (
        SELECT count(*) AS expected,
               count(*) FILTER (WHERE a.id IS NULL) AS unmarked
          FROM enrollments e
          JOIN students su ON su.org_id = e.org_id AND su.id = e.student_id
          LEFT JOIN attendance a
            ON a.org_id = e.org_id AND a.session_id = s.id AND a.student_id = e.student_id
         WHERE e.org_id = s.org_id AND e.batch_id = s.batch_id
           AND e.starts_on <= s.on_date AND (e.ended_on IS NULL OR s.on_date < e.ended_on)
           AND (su.status <> 'paused' OR a.id IS NOT NULL)
      ) m
      CROSS JOIN LATERAL (
        SELECT count(*) FILTER (WHERE a.status IN ('present', 'late')) AS came,
               count(*) FILTER (WHERE a.status IN ('absent', 'excused')) AS away
          FROM attendance a WHERE a.org_id = s.org_id AND a.session_id = s.id
      ) k";

/// The sessions of one day (today by default), earliest first.
pub async fn day(tx: &mut TenantTx, actor: Actor, query: DayQuery) -> Result<Vec<SessionSummary>> {
    actor.require(Capability::ViewStudio)?;
    let (today, now) = tx.studio_now().await?;
    let date = query.date.unwrap_or(today);
    if date > today {
        return Err(Error::Invalid(
            "Attendance is for today and earlier. The timetable shows what's coming.".into(),
        ));
    }
    if date < today - Duration::days(MAX_DAYS_BACK) {
        return Err(Error::Invalid(format!(
            "Attendance goes back {MAX_DAYS_BACK} days."
        )));
    }
    ensure(tx, date, date).await?;
    let rows = sqlx::query_as::<_, SessionSummary>(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT}
          WHERE s.on_date = $3 AND ($4::uuid IS NULL OR b.branch_id = $4)
          ORDER BY s.starts_at, lower(b.name)"
    )))
    .bind(today)
    .bind(now)
    .bind(date)
    .bind(query.branch_id)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

async fn summary(tx: &mut TenantTx, id: Uuid) -> Result<SessionSummary> {
    let (today, now) = tx.studio_now().await?;
    let row = sqlx::query_as::<_, SessionSummary>(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} WHERE s.id = $3"
    )))
    .bind(today)
    .bind(now)
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(row)
}

/// Whether `actor` may mark this session.
async fn may_mark(tx: &mut TenantTx, actor: Actor, s: &SessionSummary) -> Result<bool> {
    if actor.role.can(Capability::MarkAnyAttendance) {
        return Ok(true);
    }
    if !actor.role.can(Capability::MarkOwnAttendance) {
        return Ok(false);
    }
    if s.instructor_id == Some(actor.staff_id) {
        return Ok(true);
    }
    let usual: Option<Uuid> = sqlx::query_scalar("SELECT instructor_id FROM batches WHERE id = $1")
        .bind(s.batch_id)
        .fetch_one(tx.conn())
        .await?;
    Ok(usual == Some(actor.staff_id))
}

/// One session, with its roster when `actor` may mark it.
pub async fn get(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<SessionDetail> {
    actor.require(Capability::ViewStudio)?;
    let summary = summary(tx, id).await?;
    let can_mark = may_mark(tx, actor, &summary).await?;
    let roster = if can_mark || actor.role.can(Capability::ManageStudents) {
        Some(roster(tx, &summary).await?)
    } else {
        None
    };
    Ok(SessionDetail {
        summary,
        can_mark,
        roster,
    })
}

async fn roster(tx: &mut TenantTx, s: &SessionSummary) -> Result<Vec<RosterMark>> {
    let rows = sqlx::query_as::<_, RosterMark>(
        "WITH members AS (
           SELECT DISTINCT ON (e.student_id) e.student_id, (e.plan_id IS NULL) AS pays_with_credits
             FROM enrollments e
            WHERE e.batch_id = $2 AND e.starts_on <= $3 AND (e.ended_on IS NULL OR $3 < e.ended_on)
            ORDER BY e.student_id, e.starts_on DESC
         ), marks AS (
           SELECT student_id, status, drop_in, owes_credit FROM attendance WHERE session_id = $1
         ), everyone AS (
           SELECT student_id FROM members UNION SELECT student_id FROM marks
         )
         SELECT su.id AS student_id, su.name AS student_name, mk.status,
                coalesce(mk.drop_in, false) AS drop_in,
                coalesce(m.pays_with_credits, true) AS pays_with_credits,
                coalesce((
                  SELECT sum(l.classes + coalesce((SELECT sum(mv.change) FROM credit_moves mv
                                                     WHERE mv.org_id = l.org_id AND mv.lot_id = l.id), 0))
                    FROM credit_lots l
                   WHERE l.org_id = su.org_id AND l.student_id = su.id
                     AND (l.expires_on IS NULL OR l.expires_on >= $3)
                ), 0)::int4 AS credits_left,
                coalesce(mk.owes_credit, false) AS owes_credit,
                (SELECT count(*) FROM (
                   SELECT count(*) FILTER (WHERE a.status <> 'absent')
                            OVER (ORDER BY s2.on_date DESC, s2.starts_at DESC) AS not_absent_so_far
                     FROM attendance a
                     JOIN sessions s2 ON s2.org_id = a.org_id AND s2.id = a.session_id
                    WHERE a.student_id = su.id AND s2.batch_id = $2
                      AND (s2.on_date, s2.starts_at) < ($3, $4)
                 ) x WHERE x.not_absent_so_far = 0) AS absences_in_a_row
           FROM everyone ev
           JOIN students su ON su.id = ev.student_id
           LEFT JOIN members m ON m.student_id = ev.student_id
           LEFT JOIN marks mk ON mk.student_id = ev.student_id
          WHERE mk.student_id IS NOT NULL OR su.status <> 'paused'
          ORDER BY coalesce(mk.drop_in, false), lower(su.name)",
    )
    .bind(s.id)
    .bind(s.batch_id)
    .bind(s.on_date)
    .bind(s.starts_at)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------- marking

/// The session, checked for marking by `actor`: allowed, not cancelled,
/// and not too old.
async fn markable(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<SessionSummary> {
    // Lock the session so two people saving the same class take turns.
    let _: Uuid = sqlx::query_scalar("SELECT id FROM sessions WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    let s = summary(tx, id).await?;
    if !may_mark(tx, actor, &s).await? {
        return Err(Error::Forbidden);
    }
    if s.status == SessionStatus::Cancelled {
        return Err(Error::Invalid(
            "This class was cancelled. Bring it back before marking.".into(),
        ));
    }
    let today = tx.studio_today().await?;
    if s.on_date < today - Duration::days(INSTRUCTOR_DAYS_BACK)
        && !actor.role.can(Capability::MarkAnyAttendance)
    {
        return Err(Error::Invalid(
            "This class is more than a week old. Ask the front desk to change it.".into(),
        ));
    }
    if s.on_date < today - Duration::days(MAX_DAYS_BACK) {
        return Err(Error::Invalid(format!(
            "Attendance older than {MAX_DAYS_BACK} days can't be changed."
        )));
    }
    Ok(s)
}

/// Save one student's mark, spending or giving back a credit as needed.
async fn save_mark(
    tx: &mut TenantTx,
    actor: Actor,
    s: &SessionSummary,
    student_id: Uuid,
    status: AttendanceStatus,
    drop_in: bool,
    pays_with_credits: bool,
) -> Result<()> {
    let before: Option<(AttendanceStatus, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, credit_lot_id FROM attendance WHERE session_id = $1 AND student_id = $2",
    )
    .bind(s.id)
    .bind(student_id)
    .fetch_optional(tx.conn())
    .await?;
    let was_attended = before.is_some_and(|(st, _)| st.attended());
    let mut lot = before.and_then(|(_, lot)| lot);
    let mut owes = false;

    if pays_with_credits {
        if status.attended() {
            if lot.is_none() {
                lot = credits::use_one(tx, actor, student_id, s.on_date).await?;
            }
            owes = lot.is_none();
        } else if was_attended || lot.is_some() {
            if let Some(lot_id) = lot.take() {
                credits::return_one(tx, actor, lot_id).await?;
            }
        }
    }

    sqlx::query(
        "INSERT INTO attendance (org_id, session_id, student_id, status, drop_in,
                                 credit_lot_id, owes_credit, marked_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (org_id, session_id, student_id) DO UPDATE
            SET status = EXCLUDED.status, credit_lot_id = EXCLUDED.credit_lot_id,
                owes_credit = EXCLUDED.owes_credit, marked_by = EXCLUDED.marked_by,
                marked_at = now()",
    )
    .bind(tx.org_id())
    .bind(s.id)
    .bind(student_id)
    .bind(status)
    .bind(drop_in)
    .bind(lot)
    .bind(owes)
    .bind(actor.staff_id)
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Mark students. Each mark replaces that student's earlier one.
pub async fn mark(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    marks: Marks,
) -> Result<SessionDetail> {
    let s = markable(tx, actor, id).await?;
    if marks.marks.len() > 600 {
        return Err(Error::Invalid("That's too many marks at once.".into()));
    }
    let roster = roster(tx, &s).await?;
    let mut counts = std::collections::BTreeMap::new();
    for m in &marks.marks {
        let Some(row) = roster.iter().find(|r| r.student_id == m.student_id) else {
            return Err(Error::Invalid(
                "Someone in that list isn't in this class. Add them as a drop-in first.".into(),
            ));
        };
        save_mark(
            tx,
            actor,
            &s,
            m.student_id,
            m.status,
            row.drop_in,
            row.pays_with_credits,
        )
        .await?;
        *counts
            .entry(format!("{:?}", m.status).to_lowercase())
            .or_insert(0) += 1;
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "attendance.marked".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({ "marks": counts }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Add someone who isn't a member of the class, as present. Spends a credit.
pub async fn add_drop_in(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    drop_in: DropIn,
) -> Result<SessionDetail> {
    let s = markable(tx, actor, id).await?;
    let found: Option<(String, crate::students::StudentStatus)> =
        sqlx::query_as("SELECT name, status FROM students WHERE id = $1")
            .bind(drop_in.student_id)
            .fetch_optional(tx.conn())
            .await?;
    let Some((name, status)) = found else {
        return Err(Error::Invalid("Choose one of your students.".into()));
    };
    if status == crate::students::StudentStatus::Left {
        return Err(Error::Invalid(format!(
            "{name} has left. Set them as active first."
        )));
    }
    let roster = roster(tx, &s).await?;
    if roster.iter().any(|r| r.student_id == drop_in.student_id) {
        return Err(Error::Conflict(format!("{name} is already on this list.")));
    }
    save_mark(
        tx,
        actor,
        &s,
        drop_in.student_id,
        AttendanceStatus::Present,
        true,
        true,
    )
    .await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "attendance.drop_in_added".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({ "student_id": drop_in.student_id }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Take a drop-in off the list (added by mistake). Gives the credit back.
pub async fn remove_drop_in(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    student_id: Uuid,
) -> Result<SessionDetail> {
    markable(tx, actor, id).await?;
    let row: Option<(bool, Option<Uuid>)> = sqlx::query_as(
        "SELECT drop_in, credit_lot_id FROM attendance WHERE session_id = $1 AND student_id = $2",
    )
    .bind(id)
    .bind(student_id)
    .fetch_optional(tx.conn())
    .await?;
    match row {
        Some((true, lot)) => {
            if let Some(lot_id) = lot {
                credits::return_one(tx, actor, lot_id).await?;
            }
        }
        Some((false, _)) => {
            return Err(Error::Invalid(
                "They're a member of this class. Mark them absent instead.".into(),
            ));
        }
        None => return Err(Error::NotFound),
    }
    sqlx::query("DELETE FROM attendance WHERE session_id = $1 AND student_id = $2")
        .bind(id)
        .bind(student_id)
        .execute(tx.conn())
        .await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "attendance.drop_in_removed".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({ "student_id": student_id }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Mark the teacher present or absent, and optionally say who taught.
/// Instructors can check themselves in; anything else needs the front desk.
pub async fn set_teacher(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    mark: TeacherMark,
) -> Result<SessionDetail> {
    let s = markable(tx, actor, id).await?;
    let any = actor.role.can(Capability::MarkAnyAttendance);
    let mut instructor = s.instructor_id;
    if let Some(who) = mark.instructor_id {
        if Some(who) != s.instructor_id {
            if !any {
                return Err(Error::Forbidden);
            }
            let active: Option<bool> = sqlx::query_scalar("SELECT active FROM staff WHERE id = $1")
                .bind(who)
                .fetch_optional(tx.conn())
                .await?;
            if active != Some(true) {
                return Err(Error::Invalid("Choose someone active on your team.".into()));
            }
            instructor = Some(who);
        }
    }
    if !any && !mark.present {
        // "I wasn't there" from the teacher themselves is the front desk's call.
        return Err(Error::Forbidden);
    }
    sqlx::query(
        "UPDATE sessions
            SET teacher_present = $2, instructor_id = $3, teacher_marked_by = $4,
                teacher_marked_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(mark.present)
    .bind(instructor)
    .bind(actor.staff_id)
    .execute(tx.conn())
    .await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "attendance.teacher_marked".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({
                "present": mark.present,
                "instructor_id": instructor,
                "substitute": instructor != s.instructor_id,
            }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Cancel a class meeting. Marks are cleared and credits given back.
pub async fn cancel(
    tx: &mut TenantTx,
    actor: Actor,
    id: Uuid,
    cancel: Cancel,
) -> Result<SessionDetail> {
    actor.require(Capability::MarkAnyAttendance)?;
    let reason = validate::required(&cancel.reason, 200, "The reason")?;
    let s = markable(tx, actor, id).await?;
    let lots: Vec<Uuid> = sqlx::query_scalar(
        "SELECT credit_lot_id FROM attendance WHERE session_id = $1 AND credit_lot_id IS NOT NULL",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    for lot in &lots {
        credits::return_one(tx, actor, *lot).await?;
    }
    sqlx::query("DELETE FROM attendance WHERE session_id = $1")
        .bind(id)
        .execute(tx.conn())
        .await?;
    sqlx::query(
        "UPDATE sessions SET status = 'cancelled', cancel_reason = $2, teacher_present = NULL
          WHERE id = $1",
    )
    .bind(id)
    .bind(&reason)
    .execute(tx.conn())
    .await?;
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "session.cancelled".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({ "batch_id": s.batch_id, "credits_returned": lots.len() }),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Undo a cancellation.
pub async fn restore(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<SessionDetail> {
    actor.require(Capability::MarkAnyAttendance)?;
    let changed = sqlx::query(
        "UPDATE sessions SET status = 'scheduled', cancel_reason = NULL
          WHERE id = $1 AND status = 'cancelled'",
    )
    .bind(id)
    .execute(tx.conn())
    .await?
    .rows_affected();
    if changed == 0 {
        // Not found here, or not cancelled: `get` tells which.
        return get(tx, actor, id).await;
    }
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "session.restored".into(),
            entity: "session".into(),
            entity_id: Some(id),
            detail: json!({}),
        },
    )
    .await?;
    get(tx, actor, id).await
}

/// Pay for classes attended without a credit, oldest first, from credits
/// the student has now. Called when credits arrive. Returns how many were
/// settled.
pub(crate) async fn settle_owed(tx: &mut TenantTx, actor: Actor, student_id: Uuid) -> Result<u64> {
    let owed: Vec<(Uuid, Date)> = sqlx::query_as(
        "SELECT a.id, s.on_date
           FROM attendance a
           JOIN sessions s ON s.org_id = a.org_id AND s.id = a.session_id
          WHERE a.student_id = $1 AND a.owes_credit
          ORDER BY s.on_date, s.starts_at",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;
    let mut settled = 0;
    for (attendance_id, on_date) in owed {
        let Some(lot) = credits::use_one(tx, actor, student_id, on_date).await? else {
            break;
        };
        sqlx::query("UPDATE attendance SET credit_lot_id = $2, owes_credit = false WHERE id = $1")
            .bind(attendance_id)
            .bind(lot)
            .execute(tx.conn())
            .await?;
        settled += 1;
    }
    Ok(settled)
}

// ----------------------------------------------------------------- alerts

/// What needs someone's attention: classes that are over and not fully
/// marked, and students missing class after class. Instructors see their
/// own classes only.
pub async fn alerts(tx: &mut TenantTx, actor: Actor) -> Result<Alerts> {
    actor.require(Capability::ViewStudio)?;
    let (today, now) = tx.studio_now().await?;
    let from = today - Duration::days(ALERT_DAYS);
    ensure(tx, from, today).await?;
    let only_mine = (!actor.role.can(Capability::MarkAnyAttendance)).then_some(actor.staff_id);

    let unmarked = sqlx::query_as::<_, SessionSummary>(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT}
          WHERE s.on_date >= $3 AND s.status = 'scheduled'
            AND (s.on_date < $1 OR s.ends_at <= $2)
            AND m.unmarked > 0
            AND ($4::uuid IS NULL OR s.instructor_id = $4 OR b.instructor_id = $4)
          ORDER BY s.on_date DESC, s.starts_at DESC
          LIMIT 50"
    )))
    .bind(today)
    .bind(now)
    .bind(from)
    .bind(only_mine)
    .fetch_all(tx.conn())
    .await?;

    // For each student still in a class: how many of their latest marks in
    // that class are absences, counting back until one that isn't.
    let streaks = sqlx::query_as::<_, Streak>(
        "SELECT su.id AS student_id, su.name AS student_name, b.id AS batch_id,
                b.name AS batch_name, x.absences
           FROM enrollments e
           JOIN students su ON su.org_id = e.org_id AND su.id = e.student_id
           JOIN batches b ON b.org_id = e.org_id AND b.id = e.batch_id
           CROSS JOIN LATERAL (
             SELECT count(*) AS absences FROM (
               SELECT count(*) FILTER (WHERE a.status <> 'absent')
                        OVER (ORDER BY s.on_date DESC, s.starts_at DESC) AS not_absent_so_far
                 FROM attendance a
                 JOIN sessions s ON s.org_id = a.org_id AND s.id = a.session_id
                WHERE a.org_id = e.org_id AND a.student_id = e.student_id AND s.batch_id = e.batch_id
             ) y WHERE y.not_absent_so_far = 0
           ) x
          WHERE e.ended_on IS NULL AND su.status = 'active' AND b.active AND x.absences >= 3
            AND ($1::uuid IS NULL OR b.instructor_id = $1)
          ORDER BY x.absences DESC, lower(su.name)
          LIMIT 50",
    )
    .bind(only_mine)
    .fetch_all(tx.conn())
    .await?;
    Ok(Alerts { unmarked, streaks })
}

/// Today's class count and how many recent classes still need marking,
/// for the home page.
pub async fn counts(tx: &mut TenantTx) -> Result<(i64, i64)> {
    let (today, now) = tx.studio_now().await?;
    ensure(tx, today - Duration::days(ALERT_DAYS), today).await?;
    let today_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sessions WHERE on_date = $1 AND status = 'scheduled'",
    )
    .bind(today)
    .fetch_one(tx.conn())
    .await?;
    let unmarked: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM ({SUMMARY_SELECT}
          WHERE s.on_date >= $3 AND s.status = 'scheduled'
            AND (s.on_date < $1 OR s.ends_at <= $2) AND m.unmarked > 0) q"
    )))
    .bind(today)
    .bind(now)
    .bind(today - Duration::days(ALERT_DAYS))
    .fetch_one(tx.conn())
    .await?;
    Ok((today_count, unmarked))
}

/// A student's recent attendance. `NotFound` when they aren't in this studio.
pub async fn for_student(tx: &mut TenantTx, student_id: Uuid) -> Result<StudentAttendance> {
    let _: Uuid = sqlx::query_scalar("SELECT id FROM students WHERE id = $1")
        .bind(student_id)
        .fetch_one(tx.conn())
        .await?;
    let today = tx.studio_today().await?;
    let (came, missed): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE a.status IN ('present', 'late')),
                count(*) FILTER (WHERE a.status = 'absent')
           FROM attendance a
           JOIN sessions s ON s.org_id = a.org_id AND s.id = a.session_id
          WHERE a.student_id = $1 AND s.on_date > $2",
    )
    .bind(student_id)
    .bind(today - Duration::days(30))
    .fetch_one(tx.conn())
    .await?;
    let recent = sqlx::query_as::<_, PastMark>(
        "SELECT s.on_date, b.name AS batch_name, a.status, a.drop_in
           FROM attendance a
           JOIN sessions s ON s.org_id = a.org_id AND s.id = a.session_id
           JOIN batches b ON b.org_id = s.org_id AND b.id = s.batch_id
          WHERE a.student_id = $1
          ORDER BY s.on_date DESC, s.starts_at DESC
          LIMIT 12",
    )
    .bind(student_id)
    .fetch_all(tx.conn())
    .await?;
    Ok(StudentAttendance {
        came,
        missed,
        recent,
    })
}
