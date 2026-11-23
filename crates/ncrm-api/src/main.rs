use std::net::SocketAddr;

use anyhow::{bail, Context};
use axum::http::HeaderName;
use ncrm_api::security::SecurityConfig;
use ncrm_api::{router, AppState};
use ncrm_core::auth::{AuthConfig, CodeSender};
use ncrm_core::bot::{Ai, AiConfig};
use ncrm_core::db;
use ncrm_core::whatsapp::{MessageSender, WebhookConfig, WhatsApp, WhatsAppConfig};

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

    let whatsapp = WhatsAppConfig::from_vars(|name| std::env::var(name).ok())
        .map_err(|why| anyhow::anyhow!("WhatsApp settings: {why}"))?;
    // Studio messages (inbox replies, the bot, reminders) really go out
    // only when WhatsApp is configured. Otherwise they are stored, shown
    // in the inbox and marked sent, and a line is logged: "test mode".
    let messages = match &whatsapp {
        Some(config) => MessageSender::Cloud(WhatsApp::new(config.clone())),
        None => {
            tracing::warn!(
                "WhatsApp is not configured: studio messages run in test mode and reach nobody"
            );
            MessageSender::Test
        }
    };
    let webhook = WebhookConfig::from_vars(|name| std::env::var(name).ok())
        .map_err(|why| anyhow::anyhow!("WhatsApp webhook settings: {why}"))?;
    if webhook.is_none() {
        tracing::warn!("WHATSAPP_VERIFY_TOKEN and WHATSAPP_APP_SECRET are not set: incoming WhatsApp messages are refused");
    }
    let ai = AiConfig::from_vars(|name| std::env::var(name).ok())
        .map_err(|why| anyhow::anyhow!("AI settings: {why}"))?;
    match &ai {
        Some(config) => tracing::info!(?config, "the bot's AI mode is available"),
        None => tracing::info!("AI_API_KEY is not set: a bot in AI mode answers with the menu"),
    }
    let sender = match std::env::var("CODE_SENDER").as_deref() {
        Ok("log") | Err(_) => {
            tracing::warn!("sign-in codes are written to this log (CODE_SENDER=log): fine for development, not for real use");
            CodeSender::Log
        }
        Ok("whatsapp") => match whatsapp {
            Some(config) => {
                tracing::info!(?config, "sign-in codes go out over WhatsApp");
                CodeSender::WhatsApp(WhatsApp::new(config))
            }
            None => bail!("CODE_SENDER=whatsapp needs WHATSAPP_TOKEN and WHATSAPP_PHONE_NUMBER_ID"),
        },
        Ok(other) => bail!("unknown CODE_SENDER {other:?}; use \"log\" or \"whatsapp\""),
    };

    let pool = db::connect(&database_url, 10)
        .await
        .context("could not connect to the database")?;
    if run_migrations {
        db::migrate(&pool).await.context("migrations failed")?;
        tracing::info!("migrations applied");
    }

    let mut security = SecurityConfig {
        allowed_origins: std::env::var("ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|o| o.trim().trim_end_matches('/').to_string())
            .filter(|o| !o.is_empty())
            .collect(),
        ..SecurityConfig::default()
    };
    if let Ok(name) = std::env::var("CLIENT_IP_HEADER") {
        if !name.trim().is_empty() {
            security.client_ip_header = Some(
                HeaderName::from_bytes(name.trim().to_ascii_lowercase().as_bytes())
                    .context("CLIENT_IP_HEADER is not a valid header name")?,
            );
        }
    }
    if security.allowed_origins.is_empty() {
        tracing::warn!(
            "ALLOWED_ORIGINS is empty: browsers will not be able to sign in or make changes"
        );
    }

    let mut state = AppState::with_security(pool, auth, sender, security);
    state.cookie_secure = cookie_secure;
    state.messages = messages;
    state.webhook = webhook.map(std::sync::Arc::new);
    state.ai = ai.map(Ai::new);
    if !cookie_secure {
        tracing::warn!("COOKIE_SECURE is off; only do this for local development over http");
    }

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("could not listen on {bind_addr}"))?;
    tracing::info!("ncrm-api listening on {bind_addr}");

    // ConnectInfo gives handlers the TCP peer address, used for per-IP rate
    // limits when no trusted proxy header is configured.
    let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app)
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
