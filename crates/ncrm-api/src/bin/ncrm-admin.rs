//! The operator's tool for WhatsApp numbers.
//!
//! Each studio has its own WhatsApp number. Which number belongs to which
//! studio is set here, by whoever runs the server, and not from the
//! dashboard: if a studio could type in a number itself, it could claim
//! another studio's and receive their parents' messages.
//!
//! ```sh
//! ncrm-admin whatsapp list
//! ncrm-admin whatsapp set <studio-slug> <phone-number-id> "<number as shown>"
//! ncrm-admin whatsapp clear <studio-slug>
//! ncrm-admin whatsapp say <studio-slug> <from-number> "<text>"
//! ```
//!
//! `say` is for trying the bot and the inbox without a Meta account: it
//! acts as if `<from-number>` had written to the studio, lets the bot
//! answer, and prints the answer. Nothing is sent anywhere. Refused when
//! WhatsApp is really configured, so it can't put made-up messages into a
//! live studio's inbox.
//!
//! Needs `DATABASE_URL`. `list`, `set` and `clear` look across studios by
//! design (see docs/ARCHITECTURE.md, "Cross-studio queries").

use anyhow::{bail, Context};
use ncrm_core::bot::{self, Ai, AiConfig};
use ncrm_core::db::{self, TenantTx};
use ncrm_core::messaging::{self, Incoming};
use ncrm_core::phone;
use ncrm_core::whatsapp::{MessageSender, WhatsAppConfig};
use uuid::Uuid;

const USAGE: &str = "usage:
  ncrm-admin whatsapp list
  ncrm-admin whatsapp set <studio-slug> <phone-number-id> \"<number as shown>\"
  ncrm-admin whatsapp clear <studio-slug>
  ncrm-admin whatsapp say <studio-slug> <from-number> \"<text>\"";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let pool = db::connect(&database_url, 2)
        .await
        .context("could not connect to the database")?;

    match args.as_slice() {
        ["whatsapp", "list"] => {
            let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT slug, name, wa_phone_number_id, wa_display_phone
                   FROM organizations ORDER BY name",
            )
            .fetch_all(&pool)
            .await?;
            for (slug, name, id, shown) in rows {
                match id {
                    Some(id) => println!(
                        "{slug}  {name}  number id {id}  {}",
                        shown.unwrap_or_default()
                    ),
                    None => println!("{slug}  {name}  (no WhatsApp number)"),
                }
            }
        }
        ["whatsapp", "set", slug, number_id, shown] => {
            if !(5..=25).contains(&number_id.len())
                || !number_id.chars().all(|c| c.is_ascii_digit())
            {
                bail!("the phone number id is the id Meta shows for the number (digits only), not the phone number itself");
            }
            let shown = shown.trim();
            if shown.is_empty() || shown.len() > 30 {
                bail!("give the number as people see it, for example \"+91 98110 00000\"");
            }
            let taken: Option<String> = sqlx::query_scalar(
                "SELECT slug FROM organizations WHERE wa_phone_number_id = $1 AND slug <> $2",
            )
            .bind(number_id)
            .bind(slug)
            .fetch_optional(&pool)
            .await?;
            if let Some(other) = taken {
                bail!("that number already belongs to {other}. Clear it there first.");
            }
            let done = sqlx::query(
                "UPDATE organizations SET wa_phone_number_id = $2, wa_display_phone = $3 WHERE slug = $1",
            )
            .bind(slug)
            .bind(number_id)
            .bind(shown)
            .execute(&pool)
            .await?;
            if done.rows_affected() == 0 {
                bail!("no studio with the slug {slug:?}. Run: ncrm-admin whatsapp list");
            }
            println!(
                "{slug} now receives WhatsApp messages sent to {shown} (number id {number_id})."
            );
        }
        ["whatsapp", "clear", slug] => {
            let done = sqlx::query(
                "UPDATE organizations SET wa_phone_number_id = NULL, wa_display_phone = NULL WHERE slug = $1",
            )
            .bind(slug)
            .execute(&pool)
            .await?;
            if done.rows_affected() == 0 {
                bail!("no studio with the slug {slug:?}. Run: ncrm-admin whatsapp list");
            }
            println!("{slug} no longer has a WhatsApp number. Its conversations are kept.");
        }
        ["whatsapp", "say", slug, from, text] => {
            let live = WhatsAppConfig::from_vars(|name| std::env::var(name).ok())
                .map_err(|why| anyhow::anyhow!("WhatsApp settings: {why}"))?
                .is_some();
            if live {
                bail!("WhatsApp is configured here (WHATSAPP_TOKEN is set), so this is a real studio's inbox. `say` only runs in test mode.");
            }
            let org_id: Uuid = sqlx::query_scalar("SELECT id FROM organizations WHERE slug = $1")
                .bind(slug)
                .fetch_optional(&pool)
                .await?
                .with_context(|| {
                    format!("no studio with the slug {slug:?}. Run: ncrm-admin whatsapp list")
                })?;
            let from = phone::normalize(from)
                .map_err(|_| anyhow::anyhow!("{from:?} is not a phone number"))?;
            let ai = AiConfig::from_vars(|name| std::env::var(name).ok())
                .map_err(|why| anyhow::anyhow!("AI settings: {why}"))?
                .map(Ai::new);

            let mut tx = TenantTx::begin(&pool, org_id).await?;
            let got = messaging::receive(
                &mut tx,
                &Incoming {
                    from: from.clone(),
                    wa_message_id: format!("test.{}", Uuid::new_v4()),
                    text: text.to_string(),
                    profile_name: None,
                },
            )
            .await?;
            tx.commit().await?;
            println!("{from}: {text}");

            let answered = bot::answer(&pool, org_id, got.conversation_id, ai.as_ref()).await?;
            if answered {
                let mut tx = TenantTx::begin(&pool, org_id).await?;
                let reply: String = sqlx::query_scalar(
                    "SELECT body FROM messages
                      WHERE conversation_id = $1 AND author = 'bot'
                      ORDER BY created_at DESC LIMIT 1",
                )
                .bind(got.conversation_id)
                .fetch_one(tx.conn())
                .await?;
                tx.commit().await?;
                println!("bot: {reply}");
                messaging::send_queued(&pool, org_id, &MessageSender::Test).await?;
            } else {
                println!("(The bot stayed quiet: it is off, or this conversation is waiting for staff. See the inbox.)");
            }
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}
