//! Background worker for ncrm.
//!
//! It will drain the message outbox, send reminders and build daily
//! summaries. For now it only proves it can reach the database.

use anyhow::Context;
use ncrm_core::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let pool = db::connect(&database_url, 5)
        .await
        .context("could not connect to the database")?;
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .context("database check failed")?;
    tracing::info!("ncrm-worker started; no jobs are defined yet");

    tokio::signal::ctrl_c().await?;
    tracing::info!("ncrm-worker stopping");
    Ok(())
}
