use anyhow::{bail, Context};
use ncrm_api::{router, AppState};
use ncrm_core::auth::{AuthConfig, CodeSender};
use ncrm_core::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let run_migrations = env_flag("RUN_MIGRATIONS", false);
    let cookie_secure = env_flag("COOKIE_SECURE", true);

    let secret = std::env::var("AUTH_SECRET")
        .context("AUTH_SECRET is not set (generate one with: openssl rand -base64 48)")?;
    let auth = AuthConfig::new(secret.into_bytes())?;

    let sender = match std::env::var("CODE_SENDER").as_deref() {
        Ok("log") | Err(_) => CodeSender::Log,
        Ok(other) => bail!("unknown CODE_SENDER {other:?}; only \"log\" exists so far"),
    };

    let pool = db::connect(&database_url, 10)
        .await
        .context("could not connect to the database")?;
    if run_migrations {
        db::migrate(&pool).await.context("migrations failed")?;
        tracing::info!("migrations applied");
    }

    let mut state = AppState::new(pool, auth, sender);
    state.cookie_secure = cookie_secure;
    if !cookie_secure {
        tracing::warn!("COOKIE_SECURE is off; only do this for local development over http");
    }

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("could not listen on {bind_addr}"))?;
    tracing::info!("ncrm-api listening on {bind_addr}");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}
