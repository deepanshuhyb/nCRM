//! The numbers behind the dashboard's graphs.
//!
//! Counts and sums only: nothing here returns a name, a phone number or
//! anything about one person. That is why most of it needs no more than
//! "view the studio". Money follows the same split as the fees pages: what
//! is owed for anyone who handles fees, what was collected only for roles
//! that may see financial reports.
//!
//! Every series is zero-filled (a day with no classes is a row with
//! zeros), so the dashboard can draw it without guessing at gaps.

use serde::{Deserialize, Serialize};
use time::{Date, Duration, Time};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::attendance::SessionStatus;
use crate::db::TenantTx;
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
