//! Background worker for ncrm.
//!
//! Wakes on a timer and runs `ncrm_core::jobs::housekeeping`: tidy old
//! sign-in records, and for every studio make sure the week's class
//! meetings and the month's fees exist. Every job is safe to run twice, so
//! a restart mid-pass, or two workers by mistake, does no harm.
//!
//! Configuration (environment):
//!   DATABASE_URL           the same database the API uses
//!   WORKER_INTERVAL_SECS   seconds between passes (default 300, minimum 30)
//!   WHATSAPP_*             as for the API; unset means messages run in test mode

use std::time::Duration;

use anyhow::Context;
use ncrm_core::whatsapp::{MessageSender, WhatsApp, WhatsAppConfig};
use ncrm_core::{db, jobs};

/// Seconds between checks of the WhatsApp outbox.
const OUTBOX_SECS: u64 = 20;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let every = std::env::var("WORKER_INTERVAL_SECS")
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()
        .context("WORKER_INTERVAL_SECS must be a whole number of seconds")?
        .unwrap_or(300)
        .max(30);
    // The same WhatsApp settings the API reads. Without them, messages
    // are "sent" in test mode: marked sent, logged, delivered to nobody.
    let sender = match WhatsAppConfig::from_vars(|name| std::env::var(name).ok())
        .map_err(|why| anyhow::anyhow!("WhatsApp settings: {why}"))?
    {
        Some(config) => MessageSender::Cloud(WhatsApp::new(config)),
        None => MessageSender::Test,
    };
    let pool = db::connect(&database_url, 3)
        .await
        .context("could not connect to the database")?;
    tracing::info!(
        every_secs = every,
        whatsapp_live = sender.is_live(),
        "ncrm-worker started"
    );

    // Messages are checked far more often than housekeeping runs: someone
    // is waiting for them.
    let mut outbox = tokio::time::interval(Duration::from_secs(OUTBOX_SECS));
    outbox.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut timer = tokio::time::interval(Duration::from_secs(every));
    // If a pass overruns, wait for the next tick instead of catching up.
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut stop = std::pin::pin!(shutdown());
    loop {
        tokio::select! {
            _ = timer.tick() => match jobs::housekeeping(&pool).await {
                Ok(done) => {
                    for (org_id, why) in &done.failed {
                        tracing::error!(%org_id, why, "upkeep failed for a studio");
                    }
                    tracing::info!(
                        studios = done.studios,
                        fees_created = done.fees_created,
                        codes_removed = done.cleanup.codes,
                        sessions_removed = done.cleanup.sessions,
                        failed = done.failed.len(),
                        "housekeeping pass finished"
                    );
                }
                // Usually the database being away for a moment; try again
                // on the next tick.
                Err(err) => tracing::error!(%err, "housekeeping pass failed"),
            },
            _ = outbox.tick() => match jobs::send_messages(&pool, &sender).await {
                Ok(done) => {
                    for (org_id, why) in &done.errors {
                        tracing::error!(%org_id, why, "could not send a studio's messages");
                    }
                    // Quiet when there was nothing to do.
                    if done.studios > 0 {
                        tracing::info!(
                            studios = done.studios,
                            sent = done.sent,
                            failed = done.failed,
                            retrying = done.retrying,
                            "outbox pass finished"
                        );
                    }
                }
                Err(err) => tracing::error!(%err, "outbox pass failed"),
            },
            _ = &mut stop => break,
        }
    }
    tracing::info!("ncrm-worker stopping");
    Ok(())
}

/// Resolves on Ctrl+C or, on Unix, SIGTERM (what Docker and systemd send).
async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
