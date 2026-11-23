//! The numbers behind the dashboard's graphs.
//!
//! Counts and sums only: nothing here returns a name, a phone number or
//! anything about one person. That is why most of it needs no more than
//! "view the studio". Money follows the same split as the fees pages: what
//! is owed for anyone who handles fees, what was collected only for roles
//! that may see financial reports.
//!
//! The one place a name appears is the period report's list of teachers
//! (how many classes each took), which is team information and only for
//! roles that manage the team.
//!
//! Every series is zero-filled (a day with no classes is a row with
//! zeros), so the dashboard can draw it without guessing at gaps.

use serde::{Deserialize, Serialize};
use time::{Date, Duration, Time};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::attendance::SessionStatus;
use crate::db::TenantTx;
use crate::fees::PaymentMethod;
use crate::{fees, Error, Result};

time::serde::format_description!(hhmm, Time, "[hour]:[minute]");

/// The longest attendance series one request can ask for.
pub const MAX_DAYS: i64 = 92;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AttendanceQuery {
    /// How many days, ending today. 28 when left out.
    #[serde(default)]
    pub days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct DayAttendance {
    pub date: Date,
    /// Marked present or late.
    pub came: i64,
    /// Marked absent or excused.
    pub away: i64,
}

/// Marks per day across the studio, oldest first, one row for every day.
pub async fn attendance_by_day(
    tx: &mut TenantTx,
    actor: Actor,
    query: AttendanceQuery,
) -> Result<Vec<DayAttendance>> {
    actor.require(Capability::ViewStudio)?;
    let days = query.days.unwrap_or(28);
    if !(7..=MAX_DAYS).contains(&days) {
        return Err(Error::Invalid(format!(
            "Ask for between 7 and {MAX_DAYS} days."
        )));
    }
    let today = tx.studio_today().await?;
    let rows = sqlx::query_as::<_, DayAttendance>(
        "SELECT d::date AS date,
                count(a.id) FILTER (WHERE a.status IN ('present', 'late')) AS came,
                count(a.id) FILTER (WHERE a.status IN ('absent', 'excused')) AS away
           FROM generate_series($1::date, $2::date, interval '1 day') AS d
           LEFT JOIN sessions s ON s.on_date = d::date AND s.status = 'scheduled'
           LEFT JOIN attendance a ON a.org_id = s.org_id AND a.session_id = s.id
          GROUP BY d
          ORDER BY d",
    )
    .bind(today - Duration::days(days - 1))
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct MonthStudents {
    /// The first day of the month.
    pub month: Date,
    pub joined: i64,
    #[sqlx(rename = "left_count")]
    pub left: i64,
}

/// Students who joined and who left in each of the last six months,
/// oldest first.
pub async fn students_by_month(tx: &mut TenantTx, actor: Actor) -> Result<Vec<MonthStudents>> {
    actor.require(Capability::ViewStudio)?;
    let today = tx.studio_today().await?;
    let this_month = today.replace_day(1).expect("every month has a first day");
    let rows = sqlx::query_as::<_, MonthStudents>(
        "SELECT m::date AS month,
                (SELECT count(*) FROM students s
                  WHERE s.joined_on >= m::date
                    AND s.joined_on < (m + interval '1 month')::date) AS joined,
                (SELECT count(*) FROM students s
                  WHERE s.left_on >= m::date
                    AND s.left_on < (m + interval '1 month')::date) AS left_count
           FROM generate_series(($1::date - interval '5 months')::date, $1::date,
                                interval '1 month') AS m
          ORDER BY m",
    )
    .bind(this_month)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgeBucket {
    /// `not_due`, `week` (1 to 7 days late), `month` (8 to 30) or `older`.
    pub key: &'static str,
    pub due_paise: i64,
    pub students: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct DayCollected {
    pub date: Date,
    pub collected_paise: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    /// Everything that falls due this month (cancelled charges left out).
    pub charged_paise: i64,
    /// Money received this month so far.
    pub collected_paise: i64,
    /// From the 1st to today, one row per day.
    pub days: Vec<DayCollected>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeesReport {
    /// The first day of this month.
    pub month: Date,
    /// What is owed, by how late it is. Always four buckets, in order.
    pub ageing: Vec<AgeBucket>,
    /// `None` for roles that can't see financial reports.
    pub collection: Option<Collection>,
}

/// Dues by age, and this month's collection for those allowed to see it.
pub async fn fees(tx: &mut TenantTx, actor: Actor) -> Result<FeesReport> {
    actor.require(Capability::ManageFees)?;
    fees::ensure_current_month(tx).await?;
    let today = tx.studio_today().await?;
    let month = today.replace_day(1).expect("every month has a first day");

    let found: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT o.bucket, coalesce(sum(o.open), 0)::bigint, count(DISTINCT o.student_id)
           FROM (
             SELECT c.student_id,
                    CASE WHEN c.due_on >= $1::date THEN 'not_due'
                         WHEN c.due_on >= $1::date - 7 THEN 'week'
                         WHEN c.due_on >= $1::date - 30 THEN 'month'
                         ELSE 'older' END AS bucket,
                    c.amount_paise - coalesce((
                      SELECT sum(a.amount_paise) FROM payment_allocations a
                       WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open
               FROM charges c
              WHERE c.voided_at IS NULL
           ) o
          WHERE o.open > 0
          GROUP BY o.bucket",
    )
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    let ageing = ["not_due", "week", "month", "older"]
        .into_iter()
        .map(|key| {
            let (due_paise, students) = found
                .iter()
                .find(|(bucket, _, _)| bucket == key)
                .map_or((0, 0), |(_, due, students)| (*due, *students));
            AgeBucket {
                key,
                due_paise,
                students,
            }
        })
        .collect();

    let collection = if actor.role.can(Capability::ViewFinancials) {
        let charged_paise: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(amount_paise), 0)::bigint FROM charges
              WHERE voided_at IS NULL
                AND due_on >= $1::date AND due_on < ($1::date + interval '1 month')::date",
        )
        .bind(month)
        .fetch_one(tx.conn())
        .await?;
        let days = sqlx::query_as::<_, DayCollected>(
            "SELECT d::date AS date, coalesce(sum(p.amount_paise), 0)::bigint AS collected_paise
               FROM generate_series($1::date, $2::date, interval '1 day') AS d
               LEFT JOIN payments p ON p.received_on = d::date AND p.voided_at IS NULL
              GROUP BY d
              ORDER BY d",
        )
        .bind(month)
        .bind(today)
        .fetch_all(tx.conn())
        .await?;
        Some(Collection {
            charged_paise,
            collected_paise: days.iter().map(|d| d.collected_paise).sum(),
            days,
        })
    } else {
        None
    };

    Ok(FeesReport {
        month,
        ageing,
        collection,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct MeetingCount {
    pub on_date: Date,
    #[serde(with = "hhmm")]
    pub starts_at: Time,
    pub status: SessionStatus,
    pub came: i64,
    pub away: i64,
}

/// How one class's last eight meetings went, oldest first. Only meetings
/// someone marked or cancelled: an unmarked meeting says nothing about
/// who came. `NotFound` when the class isn't in this studio.
pub async fn class_history(
    tx: &mut TenantTx,
    actor: Actor,
    batch_id: Uuid,
) -> Result<Vec<MeetingCount>> {
    actor.require(Capability::ViewStudio)?;
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM batches WHERE id = $1")
        .bind(batch_id)
        .fetch_optional(tx.conn())
        .await?;
    if exists.is_none() {
        return Err(Error::NotFound);
    }
    let today = tx.studio_today().await?;
    let mut rows = sqlx::query_as::<_, MeetingCount>(
        "SELECT s.on_date, s.starts_at, s.status,
                (SELECT count(*) FROM attendance a
                  WHERE a.org_id = s.org_id AND a.session_id = s.id
                    AND a.status IN ('present', 'late')) AS came,
                (SELECT count(*) FROM attendance a
                  WHERE a.org_id = s.org_id AND a.session_id = s.id
                    AND a.status IN ('absent', 'excused')) AS away
           FROM sessions s
          WHERE s.batch_id = $1 AND s.on_date <= $2
            AND (s.status = 'cancelled'
                 OR EXISTS (SELECT 1 FROM attendance a
                             WHERE a.org_id = s.org_id AND a.session_id = s.id))
          ORDER BY s.on_date DESC, s.starts_at DESC
          LIMIT 8",
    )
    .bind(batch_id)
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    rows.reverse();
    Ok(rows)
}

// ---------------------------------------------------------------------------
// A period, for the Reports page
// ---------------------------------------------------------------------------

/// The longest period one report can cover.
pub const MAX_PERIOD_DAYS: i64 = 366;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PeriodQuery {
    /// First day. Defaults to the first of this month.
    #[serde(default)]
    pub from: Option<Date>,
    /// Last day, inclusive. Defaults to today.
    #[serde(default)]
    pub to: Option<Date>,
}

/// One class over the period.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClassPeriod {
    pub batch_id: Uuid,
    pub name: String,
    pub branch_name: String,
    /// Meetings that took place (not cancelled, not in the future).
    pub meetings: i64,
    pub cancelled: i64,
    /// Marks of "present" or "late".
    pub came: i64,
    /// Marks of "absent" or "excused".
    pub away: i64,
}

/// One teacher over the period.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TeacherPeriod {
    pub staff_id: Uuid,
    pub name: String,
    /// Meetings they were down to teach that took place.
    pub classes: i64,
    /// Of those, marked as taught by them.
    pub present: i64,
    pub absent: i64,
    /// Nobody said either way.
    pub unmarked: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MethodTotal {
    pub method: PaymentMethod,
    pub paise: i64,
    pub payments: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MonthMoney {
    /// First day of the month.
    pub month: Date,
    pub collected_paise: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoneyPeriod {
    /// Money received in the period (cancelled receipts left out).
    pub collected_paise: i64,
    pub payments: i64,
    /// Fees that fell due in the period (cancelled ones left out).
    pub charged_paise: i64,
    pub by_method: Vec<MethodTotal>,
    pub by_month: Vec<MonthMoney>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudentsPeriod {
    pub joined: i64,
    pub left: i64,
    /// Active right now, whatever the period.
    pub active: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodReport {
    pub from: Date,
    pub to: Date,
    pub classes: Vec<ClassPeriod>,
    pub students: StudentsPeriod,
    /// Only for roles that manage the team.
    pub teachers: Option<Vec<TeacherPeriod>>,
    /// Only for roles that may see financial reports.
    pub money: Option<MoneyPeriod>,
}

/// Everything the Reports page shows for a stretch of days.
pub async fn period(tx: &mut TenantTx, actor: Actor, query: PeriodQuery) -> Result<PeriodReport> {
    actor.require(Capability::ViewStudio)?;
    let today = tx.studio_today().await?;
    let from = query
        .from
        .unwrap_or_else(|| today.replace_day(1).expect("every month has a first day"));
    let to = query.to.unwrap_or(today);
    if to < from {
        return Err(Error::Invalid(
            "The period ends before it starts. Swap the dates.".into(),
        ));
    }
    if (to - from).whole_days() >= MAX_PERIOD_DAYS {
        return Err(Error::Invalid(
            "A report covers at most a year. Pick a shorter period.".into(),
        ));
    }
    // Classes that haven't happened yet have nothing to report.
    let until = to.min(today);

    let classes = sqlx::query_as::<_, ClassPeriod>(
        "SELECT b.id AS batch_id, b.name, br.name AS branch_name,
                count(DISTINCT s.id) FILTER (WHERE s.status = 'scheduled') AS meetings,
                count(DISTINCT s.id) FILTER (WHERE s.status = 'cancelled') AS cancelled,
                count(a.id) FILTER (WHERE s.status = 'scheduled'
                                      AND a.status IN ('present', 'late')) AS came,
                count(a.id) FILTER (WHERE s.status = 'scheduled'
                                      AND a.status IN ('absent', 'excused')) AS away
           FROM batches b
           JOIN branches br ON br.org_id = b.org_id AND br.id = b.branch_id
           JOIN sessions s ON s.org_id = b.org_id AND s.batch_id = b.id
                          AND s.on_date BETWEEN $1 AND $2
           LEFT JOIN attendance a ON a.org_id = s.org_id AND a.session_id = s.id
          GROUP BY b.id, b.name, br.name
          ORDER BY b.name, br.name",
    )
    .bind(from)
    .bind(until)
    .fetch_all(tx.conn())
    .await?;

    let (joined, left, active): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE joined_on BETWEEN $1 AND $2),
                count(*) FILTER (WHERE left_on BETWEEN $1 AND $2),
                count(*) FILTER (WHERE status = 'active')
           FROM students",
    )
    .bind(from)
    .bind(to)
    .fetch_one(tx.conn())
    .await?;

    let teachers = if actor.role.can(Capability::ManageStaff) {
        let rows = sqlx::query_as::<_, TeacherPeriod>(
            "SELECT st.id AS staff_id, st.name,
                    count(*) AS classes,
                    count(*) FILTER (WHERE s.teacher_present) AS present,
                    count(*) FILTER (WHERE NOT s.teacher_present) AS absent,
                    count(*) FILTER (WHERE s.teacher_present IS NULL) AS unmarked
               FROM sessions s
               JOIN staff st ON st.org_id = s.org_id AND st.id = s.instructor_id
              WHERE s.status = 'scheduled' AND s.on_date BETWEEN $1 AND $2
              GROUP BY st.id, st.name
              ORDER BY st.name",
        )
        .bind(from)
        .bind(until)
        .fetch_all(tx.conn())
        .await?;
        Some(rows)
    } else {
        None
    };

    let money = if actor.role.can(Capability::ViewFinancials) {
        let by_method = sqlx::query_as::<_, MethodTotal>(
            "SELECT method, sum(amount_paise)::bigint AS paise, count(*) AS payments
               FROM payments
              WHERE voided_at IS NULL AND received_on BETWEEN $1 AND $2
              GROUP BY method
              ORDER BY sum(amount_paise) DESC, method",
        )
        .bind(from)
        .bind(to)
        .fetch_all(tx.conn())
        .await?;
        // Every month the period touches, including ones with nothing.
        let by_month = sqlx::query_as::<_, MonthMoney>(
            "SELECT m::date AS month,
                    coalesce((SELECT sum(p.amount_paise) FROM payments p
                               WHERE p.voided_at IS NULL
                                 AND p.received_on BETWEEN $1 AND $2
                                 AND p.received_on >= m::date
                                 AND p.received_on < (m + interval '1 month')::date), 0)::bigint
                        AS collected_paise
               FROM generate_series(date_trunc('month', $1::date), $2::date,
                                    interval '1 month') AS m
              ORDER BY m",
        )
        .bind(from)
        .bind(to)
        .fetch_all(tx.conn())
        .await?;
        let charged_paise: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(amount_paise), 0)::bigint FROM charges
              WHERE voided_at IS NULL AND due_on BETWEEN $1 AND $2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(tx.conn())
        .await?;
        Some(MoneyPeriod {
            collected_paise: by_method.iter().map(|m| m.paise).sum(),
            payments: by_method.iter().map(|m| m.payments).sum(),
            charged_paise,
            by_method,
            by_month,
        })
    } else {
        None
    };

    Ok(PeriodReport {
        from,
        to,
        classes,
        students: StudentsPeriod {
            joined,
            left,
            active,
        },
        teachers,
        money,
    })
}
