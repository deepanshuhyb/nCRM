//! WhatsApp conversations: the inbox staff read, the messages in it, and
//! the queue of messages waiting to go out.
//!
//! A *conversation* is one phone number talking to one studio. Messages
//! come in through the webhook (`receive`), and go out by being written
//! as `queued` rows that the worker sends (`send_queued`). Nothing here
//! talks to WhatsApp directly except `send_queued`.
//!
//! Three rules shape the code:
//! - WhatsApp only lets a business write freely for 24 hours after the
//!   person last wrote. `reply` refuses outside that window instead of
//!   queueing something Meta would reject.
//! - Meta delivers webhooks at least once. An incoming message carries
//!   WhatsApp's own id; a second copy hits a unique index and is ignored.
//! - When staff reply, the bot goes quiet in that conversation for a
//!   while, so a person and the bot never talk over each other.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::access::{Actor, Capability};
use crate::db::TenantTx;
use crate::whatsapp::{MessageSender, SendError};
use crate::{validate, Error, Result};

/// How long the bot stays quiet after a person on the team replies.
pub const STAFF_TAKEOVER_HOURS: i64 = 12;
/// WhatsApp's customer-service window.
pub const REPLY_WINDOW_HOURS: i64 = 24;
/// How many times sending one message is tried.
const MAX_ATTEMPTS: i16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "bot_mode", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BotMode {
    Off,
    Menu,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "message_direction", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "message_author", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Author {
    Contact,
    Bot,
    Staff,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "message_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Received,
    Queued,
    Sending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Conversation {
    pub id: Uuid,
    pub phone: String,
    /// The contact's name from the studio's own records, when the number
    /// is one of theirs.
    pub contact_name: Option<String>,
    /// What WhatsApp calls them. Unverified.
    pub profile_name: Option<String>,
    /// Their students' names, comma-separated.
    pub students: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_message_at: OffsetDateTime,
    /// The same moment on the studio's calendar and clock ("18:05"), so
    /// the dashboard never has to guess a time zone.
    pub last_on: Date,
    pub last_at: String,
    /// The latest message, cut short.
    pub preview: Option<String>,
    pub unread: i32,
    pub needs_staff: bool,
    /// Staff have taken over and the bot is quiet.
    pub bot_paused: bool,
    /// Whether a free-text reply would be delivered right now.
    pub can_reply: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub direction: Direction,
    pub author: Author,
    pub staff_name: Option<String>,
    pub body: String,
    pub status: MessageStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The studio's date and clock time for `created_at`.
    pub on_date: Date,
    pub at_time: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    #[serde(flatten)]
    pub conversation: Conversation,
    /// Oldest first, the latest 200.
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reply {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// The studio has a WhatsApp number set.
    pub connected: bool,
    pub display_phone: Option<String>,
    pub bot_mode: BotMode,
    /// Messages really go out (WhatsApp is configured on the server).
    /// When false they are only written to the server's log.
    pub live: bool,
    /// An AI key is configured, so "AI" mode will use it.
    pub ai_available: bool,
    pub waiting: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsEdit {
    pub bot_mode: BotMode,
}

const CONVERSATION_SELECT: &str = "
    SELECT c.id, c.phone, k.name AS contact_name, c.profile_name,
           (SELECT string_agg(s.name, ', ' ORDER BY s.name)
              FROM student_contacts sc
              JOIN students s ON s.org_id = sc.org_id AND s.id = sc.student_id
             WHERE sc.org_id = k.org_id AND sc.contact_id = k.id) AS students,
           c.last_message_at,
           studio_date(c.org_id, c.last_message_at) AS last_on,
           to_char(c.last_message_at AT TIME ZONE o.timezone, 'HH24:MI') AS last_at,
           (SELECT left(m.body, 120) FROM messages m
             WHERE m.org_id = c.org_id AND m.conversation_id = c.id
             ORDER BY m.created_at DESC, m.id DESC LIMIT 1) AS preview,
           c.unread, c.needs_staff,
           coalesce(c.bot_paused_until > now(), false) AS bot_paused,
           coalesce(c.last_inbound_at > now() - make_interval(hours => $1), false) AS can_reply
      FROM conversations c
      JOIN organizations o ON o.id = c.org_id
      LEFT JOIN contacts k ON k.org_id = c.org_id AND k.phone = c.phone";

// ------------------------------------------------------------------ inbox

/// Conversations, the ones waiting for a person first, then the newest.
pub async fn inbox(tx: &mut TenantTx, actor: Actor) -> Result<Vec<Conversation>> {
    actor.require(Capability::UseInbox)?;
    let rows = sqlx::query_as::<_, Conversation>(sqlx::AssertSqlSafe(format!(
        "{CONVERSATION_SELECT}
          ORDER BY c.needs_staff DESC, c.last_message_at DESC
          LIMIT 200"
    )))
    .bind(REPLY_WINDOW_HOURS as i32)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

async fn conversation(tx: &mut TenantTx, id: Uuid) -> Result<Conversation> {
    sqlx::query_as::<_, Conversation>(sqlx::AssertSqlSafe(format!(
        "{CONVERSATION_SELECT} WHERE c.id = $2"
    )))
    .bind(REPLY_WINDOW_HOURS as i32)
    .bind(id)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(Error::NotFound)
}

/// One conversation with its messages. Opening it marks it read.
pub async fn thread(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<Thread> {
    actor.require(Capability::UseInbox)?;
    sqlx::query("UPDATE conversations SET unread = 0 WHERE id = $1 AND unread > 0")
        .bind(id)
        .execute(tx.conn())
        .await?;
    let conversation = conversation(tx, id).await?;
    let mut messages = sqlx::query_as::<_, Message>(
        "SELECT m.id, m.direction, m.author, st.name AS staff_name, m.body, m.status, m.created_at,
                studio_date(m.org_id, m.created_at) AS on_date,
                to_char(m.created_at AT TIME ZONE o.timezone, 'HH24:MI') AS at_time
           FROM messages m
           JOIN organizations o ON o.id = m.org_id
           LEFT JOIN staff st ON st.org_id = m.org_id AND st.id = m.staff_id
          WHERE m.conversation_id = $1
          ORDER BY m.created_at DESC, m.id DESC
          LIMIT 200",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    messages.reverse();
    Ok(Thread {
        conversation,
        messages,
    })
}

/// A person on the team replies. The bot goes quiet in this conversation
/// for `STAFF_TAKEOVER_HOURS`.
pub async fn reply(tx: &mut TenantTx, actor: Actor, id: Uuid, reply: Reply) -> Result<Thread> {
    actor.require(Capability::UseInbox)?;
    let text = validate::required(&reply.text, 1000, "The message")?;
    let c = conversation(tx, id).await?;
    if !c.can_reply {
        return Err(Error::Invalid(format!(
            "They last wrote more than {REPLY_WINDOW_HOURS} hours ago. WhatsApp only delivers a reply within {REPLY_WINDOW_HOURS} hours of their message."
        )));
    }
    insert_outgoing(tx, id, Author::Staff, Some(actor.staff_id), &text).await?;
    sqlx::query(
        "UPDATE conversations
            SET needs_staff = false, unread = 0, last_message_at = now(),
                bot_paused_until = now() + make_interval(hours => $2)
          WHERE id = $1",
    )
    .bind(id)
    .bind(STAFF_TAKEOVER_HOURS as i32)
    .execute(tx.conn())
    .await?;
    thread(tx, actor, id).await
}

/// Let the bot answer again, and clear "needs a person".
pub async fn hand_back(tx: &mut TenantTx, actor: Actor, id: Uuid) -> Result<Thread> {
    actor.require(Capability::UseInbox)?;
    let done = sqlx::query(
        "UPDATE conversations SET bot_paused_until = NULL, needs_staff = false WHERE id = $1",
    )
    .bind(id)
    .execute(tx.conn())
    .await?
    .rows_affected();
    if done == 0 {
        return Err(Error::NotFound);
    }
    thread(tx, actor, id).await
}

async fn insert_outgoing(
    tx: &mut TenantTx,
    conversation_id: Uuid,
    author: Author,
    staff_id: Option<Uuid>,
    text: &str,
) -> Result<Uuid> {
    let id = sqlx::query_scalar(
        "INSERT INTO messages (org_id, conversation_id, direction, author, staff_id, body, status)
         VALUES ($1, $2, 'out', $3, $4, $5, 'queued')
         RETURNING id",
    )
    .bind(tx.org_id())
    .bind(conversation_id)
    .bind(author)
    .bind(staff_id)
    .bind(text)
    .fetch_one(tx.conn())
    .await?;
    Ok(id)
}

// --------------------------------------------------------------- settings

pub async fn settings(
    tx: &mut TenantTx,
    actor: Actor,
    live: bool,
    ai_available: bool,
) -> Result<Settings> {
    actor.require(Capability::UseInbox)?;
    let (number, display_phone, bot_mode): (Option<String>, Option<String>, BotMode) =
        sqlx::query_as(
            "SELECT wa_phone_number_id, wa_display_phone, bot_mode FROM organizations WHERE id = $1",
        )
        .bind(tx.org_id())
        .fetch_one(tx.conn())
        .await?;
    let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations WHERE needs_staff")
        .fetch_one(tx.conn())
        .await?;
    Ok(Settings {
        connected: number.is_some(),
        display_phone,
        bot_mode,
        live,
        ai_available,
        waiting,
    })
}

/// Choose how the bot answers.
pub async fn set_bot_mode(tx: &mut TenantTx, actor: Actor, edit: SettingsEdit) -> Result<()> {
    actor.require(Capability::ManageMessaging)?;
    sqlx::query("UPDATE organizations SET bot_mode = $2 WHERE id = $1")
        .bind(tx.org_id())
        .bind(edit.bot_mode)
        .execute(tx.conn())
        .await?;
    crate::audit::record(
        tx,
        crate::audit::NewAuditEntry {
            actor_staff_id: Some(actor.staff_id),
            action: "messaging.bot_mode_changed".into(),
            entity: "organization".into(),
            entity_id: Some(tx.org_id()),
            detail: serde_json::json!({ "bot_mode": edit.bot_mode }),
        },
    )
    .await?;
    Ok(())
}

/// How many conversations are waiting for a person, for the home page.
pub async fn waiting(tx: &mut TenantTx) -> Result<i64> {
    let n = sqlx::query_scalar("SELECT count(*) FROM conversations WHERE needs_staff")
        .fetch_one(tx.conn())
        .await?;
    Ok(n)
}

// ---------------------------------------------------------------- incoming

/// A text message as the webhook hands it over.
#[derive(Debug, Clone)]
pub struct Incoming {
    /// The sender, E.164 with the plus.
    pub from: String,
    /// WhatsApp's id for this message.
    pub wa_message_id: String,
    pub text: String,
    pub profile_name: Option<String>,
}

/// What `receive` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Received {
    pub conversation_id: Uuid,
    /// False when this message had already been stored (a retry).
    pub new: bool,
    /// The conversation was created by this message.
    pub first_contact: bool,
}

/// The studio a WhatsApp number belongs to. One of the documented
/// cross-studio lookups: it runs before any studio is known, by
/// definition, and returns an id only.
pub async fn studio_for_number(pool: &PgPool, phone_number_id: &str) -> Result<Option<Uuid>> {
    let id = sqlx::query_scalar("SELECT id FROM organizations WHERE wa_phone_number_id = $1")
        .bind(phone_number_id)
        .fetch_optional(pool)
        .await?;
    Ok(id)
}

/// Store an incoming message. Safe to call twice with the same message.
pub async fn receive(tx: &mut TenantTx, incoming: &Incoming) -> Result<Received> {
    let text: String = incoming.text.chars().take(4096).collect();
    let text = if text.trim().is_empty() {
        "(empty message)".to_string()
    } else {
        text
    };
    let name = incoming
        .profile_name
        .as_deref()
        .map(|n| n.chars().take(120).collect::<String>())
        .filter(|n| !n.trim().is_empty());
    let (conversation_id, first_contact): (Uuid, bool) = sqlx::query_as(
        "INSERT INTO conversations (org_id, phone, profile_name)
         VALUES ($1, $2, $3)
         ON CONFLICT (org_id, phone) DO UPDATE SET profile_name = coalesce(EXCLUDED.profile_name, conversations.profile_name)
         RETURNING id, (xmax = 0)",
    )
    .bind(tx.org_id())
    .bind(&incoming.from)
    .bind(&name)
    .fetch_one(tx.conn())
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO messages (org_id, conversation_id, direction, author, body, status, wa_message_id)
         VALUES ($1, $2, 'in', 'contact', $3, 'received', $4)
         ON CONFLICT (org_id, wa_message_id) WHERE wa_message_id IS NOT NULL DO NOTHING",
    )
    .bind(tx.org_id())
    .bind(conversation_id)
    .bind(&text)
    .bind(&incoming.wa_message_id)
    .execute(tx.conn())
    .await?
    .rows_affected();
    if inserted == 1 {
        sqlx::query(
            "UPDATE conversations
                SET last_message_at = now(), last_inbound_at = now(), unread = unread + 1
              WHERE id = $1",
        )
        .bind(conversation_id)
        .execute(tx.conn())
        .await?;
    }
    Ok(Received {
        conversation_id,
        new: inserted == 1,
        first_contact,
    })
}

/// Whether the bot may answer in this conversation right now.
pub async fn bot_may_answer(tx: &mut TenantTx, conversation_id: Uuid) -> Result<bool> {
    let ok: bool = sqlx::query_scalar(
        "SELECT NOT needs_staff AND coalesce(bot_paused_until <= now(), true)
           FROM conversations WHERE id = $1",
    )
    .bind(conversation_id)
    .fetch_one(tx.conn())
    .await?;
    Ok(ok)
}

/// Queue the bot's answer, and flag the conversation when a person is
/// needed.
pub async fn bot_says(
    tx: &mut TenantTx,
    conversation_id: Uuid,
    text: &str,
    needs_staff: bool,
) -> Result<()> {
    let text: String = text.chars().take(1500).collect();
    if !text.trim().is_empty() {
        insert_outgoing(tx, conversation_id, Author::Bot, None, &text).await?;
    }
    sqlx::query(
        "UPDATE conversations SET last_message_at = now(), needs_staff = needs_staff OR $2 WHERE id = $1",
    )
    .bind(conversation_id)
    .bind(needs_staff)
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// A delivery report from WhatsApp for a message we sent. Reports can
/// arrive out of order, so the status only ever moves forward.
pub async fn delivery_report(
    tx: &mut TenantTx,
    wa_message_id: &str,
    status: MessageStatus,
    error_code: Option<i32>,
) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE messages
            SET status = $2, error_code = coalesce($3, error_code)
          WHERE wa_message_id = $1 AND direction = 'out'
            AND (($2 = 'failed' AND status <> 'read')
                 OR ($2 = 'delivered' AND status IN ('sent'))
                 OR ($2 = 'read' AND status IN ('sent', 'delivered'))
                 OR ($2 = 'sent' AND status = 'sending'))",
    )
    .bind(wa_message_id)
    .bind(status)
    .bind(error_code)
    .execute(tx.conn())
    .await?
    .rows_affected();
    Ok(done > 0)
}

// ---------------------------------------------------------------- outgoing

#[derive(Debug, sqlx::FromRow)]
struct Outgoing {
    id: Uuid,
    phone: String,
    body: String,
    template: Option<String>,
    template_params: Option<serde_json::Value>,
    attempts: i16,
}

/// What one sending pass did for one studio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sent {
    pub sent: u32,
    pub failed: u32,
    pub retrying: u32,
}

/// Send this studio's queued messages. Called by the worker.
///
/// Each message is claimed in a short transaction (`queued` → `sending`),
/// sent with no transaction open, then marked in another. A worker that
/// dies in between leaves the message `sending`; after two minutes it is
/// picked up again, so a message is sent at least once and very rarely
/// twice, never lost.
pub async fn send_queued(pool: &PgPool, org_id: Uuid, sender: &MessageSender) -> Result<Sent> {
    let mut tx = TenantTx::begin(pool, org_id).await?;
    let from: Option<String> =
        sqlx::query_scalar("SELECT wa_phone_number_id FROM organizations WHERE id = $1")
            .bind(org_id)
            .fetch_one(tx.conn())
            .await?;
    let batch = sqlx::query_as::<_, Outgoing>(
        "UPDATE messages m
            SET status = 'sending', attempts = attempts + 1, sent_at = now()
           FROM conversations c
          WHERE c.org_id = m.org_id AND c.id = m.conversation_id
            AND m.id IN (
              SELECT id FROM messages
               WHERE status = 'queued'
                  OR (status = 'sending' AND sent_at < now() - interval '2 minutes')
               ORDER BY created_at
               LIMIT 20
               FOR UPDATE SKIP LOCKED)
         RETURNING m.id, c.phone, m.body, m.template, m.template_params, m.attempts",
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;

    let mut done = Sent::default();
    for message in batch {
        let outcome = match (sender, &from) {
            (MessageSender::Test, _) => {
                // Test mode: nothing leaves the server. The line says
                // that something would have gone out, not what or to
                // whom: test mode is also what a real deployment runs
                // until WhatsApp is set up, and its logs must not collect
                // parents' numbers and fee reminders. The inbox shows the
                // message itself.
                let tail: String = message
                    .phone
                    .chars()
                    .rev()
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                eprintln!(
                    "[ncrm test mode] a WhatsApp message to a number ending {tail} was not really sent ({} characters). See it in the inbox.",
                    message.body.chars().count()
                );
                Ok(format!("test-{}", message.id))
            }
            (MessageSender::Cloud(_), None) => Err(SendError::Refused {
                status: 0,
                code: None,
            }),
            (MessageSender::Cloud(client), Some(from)) => match &message.template {
                Some(template) => {
                    let params: Vec<String> = message
                        .template_params
                        .as_ref()
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    client
                        .send_template(from, &message.phone, template, &params)
                        .await
                }
                None => client.send_text(from, &message.phone, &message.body).await,
            },
        };
        let mut tx = TenantTx::begin(pool, org_id).await?;
        match outcome {
            Ok(wa_id) => {
                sqlx::query(
                    "UPDATE messages SET status = 'sent', wa_message_id = nullif($2, ''), sent_at = now()
                      WHERE id = $1 AND status = 'sending'",
                )
                .bind(message.id)
                .bind(&wa_id)
                .execute(tx.conn())
                .await?;
                done.sent += 1;
            }
            // Meta said no: trying again won't change its mind.
            Err(SendError::Refused { code, .. }) => {
                sqlx::query("UPDATE messages SET status = 'failed', error_code = $2 WHERE id = $1")
                    .bind(message.id)
                    .bind(code.map(|c| c as i32))
                    .execute(tx.conn())
                    .await?;
                done.failed += 1;
            }
            // Couldn't reach Meta: back in the queue, a few times.
            Err(SendError::Unreachable) => {
                let give_up = message.attempts >= MAX_ATTEMPTS;
                sqlx::query("UPDATE messages SET status = $2 WHERE id = $1")
                    .bind(message.id)
                    .bind(if give_up {
                        MessageStatus::Failed
                    } else {
                        MessageStatus::Queued
                    })
                    .execute(tx.conn())
                    .await?;
                if give_up {
                    done.failed += 1;
                } else {
                    done.retrying += 1;
                }
            }
        }
        tx.commit().await?;
    }
    Ok(done)
}
