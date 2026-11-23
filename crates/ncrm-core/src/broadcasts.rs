//! Broadcasts: one message from the studio to many people at once.
//! "No class tomorrow, the hall is being painted."
//!
//! Staff choose who it is for (everyone, a branch, a class, students who
//! owe, or students picked by hand) and write one paragraph. Each
//! student's main contact gets it once, however many of their children
//! are in the group.
//!
//! What this module is careful about:
//! - **It costs money and can't be unsent**, so only owners and managers
//!   may send one, the dashboard shows exactly how many people will get
//!   it before asking for a second tap, there is a ceiling per broadcast,
//!   and sending the same thing to the same group twice within two
//!   minutes is refused (a double tap, a retry after a slow response).
//! - **One paragraph.** It goes out as an approved WhatsApp template with
//!   the text as a value, and WhatsApp refuses values with line breaks.
//! - **Each copy is an ordinary message** in that person's conversation
//!   (`messages.broadcast_id` points back here), so a reply lands in the
//!   inbox with what they are replying to above it, and deliveries can be
//!   counted.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::audit::{self, NewAuditEntry};
use crate::db::TenantTx;
use crate::messaging::{self, Author};
use crate::whatsapp::{template_value, Templates};
use crate::{Error, Result};

/// The most people one broadcast can go to.
pub const MAX_RECIPIENTS: i64 = 1000;
/// The longest a broadcast's text can be.
pub const MAX_TEXT: usize = 600;
/// The most students that can be picked by hand.
pub const MAX_PICKED: usize = 500;

/// Who a broadcast is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Audience {
    /// Every student who hasn't left.
    Everyone,
    /// Students of a branch, or in a class that meets there.
    Branch { branch_id: Uuid },
    /// Students in a class, including those due to start.
    Batch { batch_id: Uuid },
    /// Students with anything left to pay.
    Owing,
    /// Exactly these students.
    Students { student_ids: Vec<Uuid> },
}

/// What sending to an audience would do, before anything is sent.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    /// The audience in words: "Everyone", "Hip-hop Beginners".
    pub label: String,
    /// Students in the group.
    pub students: i64,
    /// People who would get the message (main contacts, each once).
    pub recipients: i64,
    /// Students in the group with no main contact; they would be missed.
    pub without_contact: i64,
    /// A few of the people it would go to, to check it's the right group.
    pub sample: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewBroadcast {
    pub audience: Audience,
    pub text: String,
}

/// A broadcast that was sent, with how its copies have fared.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Broadcast {
    pub id: Uuid,
    pub body: String,
    pub audience_label: String,
    pub recipients: i32,
    pub sent_by: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The studio's date and clock time for `created_at`.
    pub on_date: Date,
    pub at_time: String,
    /// Not gone out yet.
    pub waiting: i64,
    /// Accepted by WhatsApp, including those since delivered or read.
    pub sent: i64,
    pub delivered: i64,
    pub read: i64,
    pub failed: i64,
}

#[derive(Debug, FromRow)]
struct Recipient {
    name: String,
    phone: String,
}

/// The students an audience means, as a SQL condition on `s` (students).
/// `$1` is the studio's today, `$2` a uuid, `$3` a uuid array; each
/// audience uses what it needs.
fn students_where(audience: &Audience) -> &'static str {
    match audience {
        Audience::Everyone => "s.status <> 'left'",
        Audience::Branch { .. } => {
            "s.status <> 'left' AND (s.branch_id = $2 OR EXISTS (
                SELECT 1 FROM enrollments e
                  JOIN batches b ON b.org_id = e.org_id AND b.id = e.batch_id
                 WHERE e.org_id = s.org_id AND e.student_id = s.id AND b.branch_id = $2
                   AND b.active AND (e.ended_on IS NULL OR e.ended_on > $1)))"
        }
        Audience::Batch { .. } => {
            "s.status <> 'left' AND EXISTS (
                SELECT 1 FROM enrollments e
                 WHERE e.org_id = s.org_id AND e.student_id = s.id AND e.batch_id = $2
                   AND (e.ended_on IS NULL OR e.ended_on > $1))"
        }
        Audience::Owing => {
            "s.status <> 'left' AND EXISTS (
                SELECT 1 FROM charges c
                 WHERE c.org_id = s.org_id AND c.student_id = s.id AND c.voided_at IS NULL
                   AND c.amount_paise > coalesce((
                       SELECT sum(a.amount_paise) FROM payment_allocations a
                        WHERE a.org_id = c.org_id AND a.charge_id = c.id), 0))"
        }
        Audience::Students { .. } => "s.id = ANY($3)",
    }
}

struct Resolved {
    label: String,
    students: i64,
    without_contact: i64,
    recipients: Vec<Recipient>,
}

/// Work out who an audience is. Checks that a named branch or class is
/// this studio's (row-level security makes anyone else's invisible, so
/// it is simply "not found").
async fn resolve(tx: &mut TenantTx, audience: &Audience) -> Result<Resolved> {
    let today = tx.studio_today().await?;
    let (id, ids, label): (Option<Uuid>, Vec<Uuid>, String) = match audience {
        Audience::Everyone => (None, vec![], "Everyone".into()),
        Audience::Owing => (None, vec![], "Students who owe".into()),
        Audience::Branch { branch_id } => {
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM branches WHERE id = $1")
                    .bind(branch_id)
                    .fetch_optional(tx.conn())
                    .await?;
            (Some(*branch_id), vec![], name.ok_or(Error::NotFound)?)
        }
        Audience::Batch { batch_id } => {
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM batches WHERE id = $1")
                .bind(batch_id)
                .fetch_optional(tx.conn())
                .await?;
            (Some(*batch_id), vec![], name.ok_or(Error::NotFound)?)
        }
        Audience::Students { student_ids } => {
            if student_ids.is_empty() {
                return Err(Error::Invalid("Choose at least one student.".into()));
            }
            if student_ids.len() > MAX_PICKED {
                return Err(Error::Invalid(format!(
                    "That's more than {MAX_PICKED} students. Use a branch or a class instead."
                )));
            }
            let mut ids = student_ids.clone();
            ids.sort();
            ids.dedup();
            let label = match ids.len() {
                1 => "1 chosen student".to_string(),
                n => format!("{n} chosen students"),
            };
            (None, ids, label)
        }
    };
    let who = students_where(audience);
    // Each audience's condition uses some of the three values and not
    // others, but a prepared statement must be given exactly the values
    // it mentions. So both queries mention all three up front, in tests
    // that are always true.
    let (students, without_contact): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT count(*),
                count(*) FILTER (WHERE NOT EXISTS (
                    SELECT 1 FROM student_contacts sc
                     WHERE sc.org_id = s.org_id AND sc.student_id = s.id AND sc.is_primary))
           FROM students s
          WHERE ($1::date IS NOT NULL) AND ($2::uuid IS NULL OR true) AND ($3::uuid[] IS NOT NULL)
            AND {who}"
    )))
    .bind(today)
    .bind(id)
    .bind(&ids)
    .fetch_one(tx.conn())
    .await?;
    // Each main contact once, whichever of their students matched.
    let recipients: Vec<Recipient> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT DISTINCT ON (k.id) k.name, k.phone
           FROM students s
           JOIN student_contacts sc
             ON sc.org_id = s.org_id AND sc.student_id = s.id AND sc.is_primary
           JOIN contacts k ON k.org_id = sc.org_id AND k.id = sc.contact_id
          WHERE ($1::date IS NOT NULL) AND ($2::uuid IS NULL OR true) AND ($3::uuid[] IS NOT NULL)
            AND {who}
          ORDER BY k.id
          LIMIT {limit}",
        limit = MAX_RECIPIENTS + 1
    )))
    .bind(today)
    .bind(id)
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    Ok(Resolved {
        label,
        students,
        without_contact,
        recipients,
    })
}

/// How many people an audience is, without sending anything.
pub async fn preview(tx: &mut TenantTx, actor: Actor, audience: &Audience) -> Result<Preview> {
    actor.require(Capability::ManageMessaging)?;
    let found = resolve(tx, audience).await?;
    let mut sample: Vec<String> = found.recipients.iter().map(|r| r.name.clone()).collect();
    sample.sort();
    sample.truncate(5);
    Ok(Preview {
        label: found.label,
        students: found.students,
        recipients: found.recipients.len() as i64,
        without_contact: found.without_contact,
        sample,
    })
}

/// Send a broadcast: record it and queue one message per recipient.
pub async fn send(
    tx: &mut TenantTx,
    actor: Actor,
    new: NewBroadcast,
    templates: &Templates,
) -> Result<Broadcast> {
    actor.require(Capability::ManageMessaging)?;
    // One paragraph, whatever was typed: line breaks become spaces.
    let text = template_value(&new.text, MAX_TEXT + 1);
    if text.is_empty() {
        return Err(Error::Invalid("Write the message first.".into()));
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Error::Invalid(format!(
            "That's longer than {MAX_TEXT} characters. Shorten it a little."
        )));
    }
    // One broadcast at a time per studio, so the double-send check below
    // can't be raced by a second tap.
    tx.lock_studio("broadcast").await?;
    let found = resolve(tx, &new.audience).await?;
    if found.recipients.is_empty() {
        return Err(Error::Invalid(
            "Nobody in that group has a contact to message.".into(),
        ));
    }
    if found.recipients.len() as i64 > MAX_RECIPIENTS {
        return Err(Error::Invalid(format!(
            "That's more than {MAX_RECIPIENTS} people in one go. Send it branch by branch."
        )));
    }
    let audience = serde_json::to_value(&new.audience).expect("an audience is plain data");
    let repeat: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM broadcasts
                         WHERE body = $1 AND audience = $2
                           AND created_at > now() - interval '2 minutes')",
    )
    .bind(&text)
    .bind(&audience)
    .fetch_one(tx.conn())
    .await?;
    if repeat {
        return Err(Error::Conflict(
            "This was sent to the same people a moment ago. It hasn't been sent again.".into(),
        ));
    }
    let studio: String = sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
        .bind(tx.org_id())
        .fetch_one(tx.conn())
        .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO broadcasts (org_id, body, audience, audience_label, recipients, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(&text)
    .bind(&audience)
    .bind(&found.label)
    .bind(found.recipients.len() as i32)
    .bind(actor.staff_id)
    .fetch_one(tx.conn())
    .await?;
    let studio = template_value(&studio, 80);
    for r in &found.recipients {
        let params = [template_value(&r.name, 60), studio.clone(), text.clone()];
        messaging::queue_template(
            tx,
            &r.phone,
            Author::Staff,
            Some(actor.staff_id),
            &text,
            &templates.broadcast,
            &params,
            Some(id),
        )
        .await?;
    }
    // Who sent how many to which group. Not the text: it is in the
    // broadcast itself, and audit entries are kept longest.
    audit::record(
        tx,
        NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "broadcast.sent".into(),
            entity: "broadcast".into(),
            entity_id: Some(id),
            detail: json!({ "audience": found.label, "recipients": found.recipients.len() }),
        },
    )
    .await?;
    one(tx, id).await
}

const SELECT: &str = "
    SELECT b.id, b.body, b.audience_label, b.recipients, st.name AS sent_by, b.created_at,
           studio_date(b.org_id, b.created_at) AS on_date,
           to_char(b.created_at AT TIME ZONE o.timezone, 'HH24:MI') AS at_time,
           count(m.id) FILTER (WHERE m.status IN ('queued', 'sending')) AS waiting,
           count(m.id) FILTER (WHERE m.status IN ('sent', 'delivered', 'read')) AS sent,
           count(m.id) FILTER (WHERE m.status IN ('delivered', 'read')) AS delivered,
           count(m.id) FILTER (WHERE m.status = 'read') AS read,
           count(m.id) FILTER (WHERE m.status = 'failed') AS failed
      FROM broadcasts b
      JOIN organizations o ON o.id = b.org_id
      JOIN staff st ON st.org_id = b.org_id AND st.id = b.created_by
      LEFT JOIN messages m ON m.org_id = b.org_id AND m.broadcast_id = b.id";

async fn one(tx: &mut TenantTx, id: Uuid) -> Result<Broadcast> {
    sqlx::query_as::<_, Broadcast>(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE b.id = $1 GROUP BY b.id, o.timezone, st.name"
    )))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(Error::NotFound)
}

/// The latest broadcasts, newest first, with how many copies have gone,
/// arrived and been read.
pub async fn list(tx: &mut TenantTx, actor: Actor) -> Result<Vec<Broadcast>> {
    actor.require(Capability::ManageMessaging)?;
    let rows = sqlx::query_as::<_, Broadcast>(sqlx::AssertSqlSafe(format!(
        "{SELECT} GROUP BY b.id, o.timezone, st.name ORDER BY b.created_at DESC LIMIT 30"
    )))
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}
