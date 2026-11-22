use anyhow::Context;
use ncrm_api::{router, AppState};
use ncrm_core::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let run_migrations = std::env::var("RUN_MIGRATIONS").is_ok_and(|v| v == "1" || v == "true");

    let pool = db::connect(&database_url, 10)
        .await
        .context("could not connect to the database")?;
    if run_migrations {
        db::migrate(&pool).await.context("migrations failed")?;
        tracing::info!("migrations applied");
    }

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("could not listen on {bind_addr}"))?;
    tracing::info!("ncrm-api listening on {bind_addr}");

    axum::serve(listener, router(AppState { pool }))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
