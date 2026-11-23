//! Fee reminders on WhatsApp.
//!
//! When a fee is coming due or is late, the student's main contact gets a
//! message: three days before the due date, on the day, and three and
//! seven days after. Then it stops; after a week it is a conversation for
//! a person to have, not a fifth automatic message.
//!
//! What this module is careful about:
//! - **Once each.** Every reminder made is recorded per charge and step
//!   (`fee_reminders_sent`) in the same transaction as the message, and a
//!   studio-wide lock means two passes can't overlap. Running this every
//!   five minutes, or twice at once, sends nothing twice.
//! - **One message per person per pass.** A parent with two children, or
//!   one child with two fees due together, gets a single message naming
//!   everyone, not one per fee.
//! - **Daytime only**, by the studio's own clock.
//! - **No surprise backlog.** A fee more than ten days late when
//!   reminders start (a studio switching them on, or its first day on
//!   ncrm) is never reminded about. Only fees inside the schedule are.
//! - **Nobody who has left.** Fees owed by a student marked "left" are
//!   for staff to follow up.
//!
//! The message is an approved WhatsApp template (see `whatsapp::Templates`)
//! because the person usually hasn't written in the last 24 hours.

use sqlx::FromRow;
use time::{Date, Month, Time};
use uuid::Uuid;

use crate::bot::rupees;
use crate::db::TenantTx;
use crate::messaging::{self, Author};
use crate::whatsapp::{template_value, Templates};
use crate::Result;

/// Reminders go out from this hour of the studio's day...
pub const FROM_HOUR: u8 = 10;
/// ...until this one.
pub const UNTIL_HOUR: u8 = 20;

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reminded {
    /// Messages queued (one per contact).
    pub messages: u32,
    /// Fees those messages were about.
    pub charges: u32,
    /// Fees whose student has no main contact to tell.
    pub no_contact: u32,
}

#[derive(Debug, FromRow)]
struct Due {
    charge_id: Uuid,
    student_id: Uuid,
    student_name: String,
    step: i16,
    contact_name: Option<String>,
    phone: Option<String>,
}

fn day_month(d: Date) -> String {
    let month = match d.month() {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    };
    format!("{} {month}", d.day())
}

/// "Aarav", "Aarav and Anya", "Aarav, Anya and Zoya".
fn names(list: &[String]) -> String {
    match list {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What is owed, in words: the fourth value of the reminder template.
fn owed(total: i64, overdue: i64, earliest: Date, today: Date) -> String {
    if overdue == 0 && earliest == today {
        format!("{} is due today", rupees(total))
    } else if overdue == 0 {
        format!("{} is due on {}", rupees(total), day_month(earliest))
    } else if overdue == total {
        format!("{} is overdue", rupees(total))
    } else {
        format!(
            "{} to pay, {} of it overdue",
            rupees(total),
            rupees(overdue)
        )
    }
}

/// Queue the reminders that are due now for this studio. Safe to call as
/// often as you like.
pub async fn queue_due(tx: &mut TenantTx, templates: &Templates) -> Result<Reminded> {
    let (today, now) = tx.studio_now().await?;
    let from = Time::from_hms(FROM_HOUR, 0, 0).expect("a valid hour");
    let until = Time::from_hms(UNTIL_HOUR, 0, 0).expect("a valid hour");
    if now < from || now >= until {
        return Ok(Reminded::default());
    }
    let (studio, on): (String, bool) =
        sqlx::query_as("SELECT name, fee_reminders FROM organizations WHERE id = $1")
            .bind(tx.org_id())
            .fetch_one(tx.conn())
            .await?;
    if !on {
        return Ok(Reminded::default());
    }
    // One pass at a time per studio: the check below and the record
    // written after it must not interleave with another pass.
    tx.lock_studio("fee-reminders").await?;

    // Every unpaid, uncancelled fee that is inside the schedule today and
    // hasn't had this step's reminder. `step` is the latest one reached:
    // a fee two days late is on step 0 ("on the day") until day 3. If the
    // worker was off on the exact day, the reminder goes when it is back,
    // as long as the next step hasn't begun.
    let due: Vec<Due> = sqlx::query_as(
        "WITH open AS (
            SELECT c.id, c.student_id, c.due_on,
                   c.amount_paise - coalesce((
                       SELECT sum(a.amount_paise) FROM payment_allocations a
                        WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open,
                   CASE WHEN $1 - c.due_on >= 7 THEN 7
                        WHEN $1 - c.due_on >= 3 THEN 3
                        WHEN $1 - c.due_on >= 0 THEN 0
                        ELSE -3 END::smallint AS step
              FROM charges c
             WHERE c.voided_at IS NULL AND $1 - c.due_on BETWEEN -3 AND 9
         )
         SELECT o.id AS charge_id, o.student_id, s.name AS student_name, o.step,
                k.name AS contact_name, k.phone
           FROM open o
           JOIN students s ON s.org_id = $2 AND s.id = o.student_id
           LEFT JOIN student_contacts sc
                  ON sc.org_id = s.org_id AND sc.student_id = s.id AND sc.is_primary
           LEFT JOIN contacts k ON k.org_id = sc.org_id AND k.id = sc.contact_id
          WHERE o.open > 0 AND s.status <> 'left'
            AND NOT EXISTS (SELECT 1 FROM fee_reminders_sent r
                             WHERE r.org_id = $2 AND r.charge_id = o.id AND r.step = o.step)
          ORDER BY k.phone NULLS LAST, s.name, o.due_on",
    )
    .bind(today)
    .bind(tx.org_id())
    .fetch_all(tx.conn())
    .await?;

    let mut done = Reminded::default();
    let mut rest = due.as_slice();
    while let Some(first) = rest.first() {
        // Rows are ordered by phone, so one contact's fees are together.
        let len = rest.iter().take_while(|d| d.phone == first.phone).count();
        let (group, tail) = rest.split_at(len);
        rest = tail;

        let message_id = match (&first.phone, &first.contact_name) {
            (Some(phone), Some(contact)) => {
                let mut students: Vec<(Uuid, String)> = group
                    .iter()
                    .map(|d| (d.student_id, d.student_name.clone()))
                    .collect();
                students.sort_by(|a, b| a.1.cmp(&b.1));
                students.dedup_by(|a, b| a.0 == b.0);
                let ids: Vec<Uuid> = students.iter().map(|s| s.0).collect();
                // The amount is everything these students owe that is due
                // within three days, not just the fee that set this off:
                // "₹2,500 is due" while ₹5,000 more is already late would
                // mislead.
                let (total, overdue, earliest): (i64, i64, Option<Date>) = sqlx::query_as(
                    "SELECT coalesce(sum(o.open), 0)::bigint,
                            coalesce(sum(o.open) FILTER (WHERE o.due_on < $2), 0)::bigint,
                            min(o.due_on)
                       FROM (SELECT c.due_on, c.amount_paise - coalesce((
                                      SELECT sum(a.amount_paise) FROM payment_allocations a
                                       WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0) AS open
                               FROM charges c
                              WHERE c.student_id = ANY($1) AND c.voided_at IS NULL
                                AND c.due_on <= $2 + 3) o
                      WHERE o.open > 0",
                )
                .bind(&ids)
                .bind(today)
                .fetch_one(tx.conn())
                .await?;
                let who = names(&students.into_iter().map(|s| s.1).collect::<Vec<_>>());
                let params = [
                    template_value(contact, 60),
                    template_value(&studio, 80),
                    template_value(&who, 200),
                    owed(total, overdue, earliest.unwrap_or(today), today),
                ];
                let body = format!(
                    "Hi {}, this is {}. Fees for {}: {}. You can pay at the front desk by cash, UPI or card.",
                    params[0], params[1], params[2], params[3]
                );
                let id = messaging::queue_template(
                    tx,
                    phone,
                    Author::System,
                    None,
                    &body,
                    &templates.fee_reminder,
                    &params,
                    None,
                )
                .await?;
                done.messages += 1;
                done.charges += group.len() as u32;
                Some(id)
            }
            _ => {
                done.no_contact += group.len() as u32;
                None
            }
        };
        for d in group {
            sqlx::query(
                "INSERT INTO fee_reminders_sent (org_id, charge_id, step, message_id)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(tx.org_id())
            .bind(d.charge_id)
            .bind(d.step)
            .bind(message_id)
            .execute(tx.conn())
            .await?;
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn what_is_owed_reads_naturally() {
        let today = date!(2026 - 12 - 05);
        assert_eq!(owed(250_000, 0, today, today), "₹2,500 is due today");
        assert_eq!(
            owed(250_000, 0, date!(2026 - 12 - 08), today),
            "₹2,500 is due on 8 Dec"
        );
        assert_eq!(
            owed(250_000, 250_000, date!(2026 - 12 - 02), today),
            "₹2,500 is overdue"
        );
        assert_eq!(
            owed(750_000, 500_000, date!(2026 - 11 - 05), today),
            "₹7,500 to pay, ₹5,000 of it overdue"
        );
    }

    #[test]
    fn several_students_are_named_in_one_sentence() {
        let n = |v: &[&str]| names(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(n(&["Aarav"]), "Aarav");
        assert_eq!(n(&["Aarav", "Anya"]), "Aarav and Anya");
        assert_eq!(n(&["Aarav", "Anya", "Zoya"]), "Aarav, Anya and Zoya");
    }
}
