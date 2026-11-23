//! Sign-in codes over WhatsApp, against a stand-in for Meta's API running
//! on this machine: what we send, what we keep out of errors, and that a
//! number that isn't staff causes no message at all.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use common::{hire, studio, test_security, DASHBOARD};
use ncrm_api::{router, AppState};
use ncrm_core::auth::{AuthConfig, CodeSender, PendingCode};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use ncrm_core::whatsapp::{WhatsApp, WhatsAppConfig};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Meta {
    /// (path id, authorization header, body) of every request received.
    seen: Arc<Mutex<Vec<(String, String, Value)>>>,
    /// What to answer with.
    reply: Arc<Mutex<(u16, Value)>>,
}

async fn messages(
    State(meta): State<Meta>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, HeaderMap, Json<Value>) {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    meta.seen.lock().unwrap().push((id, auth, body));
    let (status, reply) = meta.reply.lock().unwrap().clone();
    let mut out = HeaderMap::new();
    if status == 302 {
        out.insert("location", "http://127.0.0.1:9/elsewhere".parse().unwrap());
    }
    (StatusCode::from_u16(status).unwrap(), out, Json(reply))
}

/// Start the stand-in and a client pointed at it.
async fn stand_in() -> (Meta, WhatsApp) {
    let meta = Meta::default();
    *meta.reply.lock().unwrap() = (200, json!({ "messages": [{ "id": "wamid.TEST" }] }));
    let app = Router::new()
        .route("/{id}/messages", post(messages))
        .with_state(meta.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let config = WhatsAppConfig::from_vars(|name| match name {
        "WHATSAPP_TOKEN" => Some("test-token".into()),
        "WHATSAPP_PHONE_NUMBER_ID" => Some("1055512345".into()),
        "WHATSAPP_API_BASE" => Some(format!("http://127.0.0.1:{port}")),
        "WHATSAPP_OTP_TEMPLATE" => Some("studio_code".into()),
        "WHATSAPP_TEMPLATE_LANG" => Some("en_US".into()),
        _ => None,
    })
    .unwrap()
    .unwrap();
    (meta, WhatsApp::new(config))
}

#[tokio::test]
async fn the_code_goes_out_as_an_authentication_template() {
    let (meta, client) = stand_in().await;
    client
        .send_sign_in_code("+919800000001", "482913")
        .await
        .unwrap();

    let seen = meta.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (id, auth, body) = &seen[0];
    assert_eq!(id, "1055512345", "sent from our number's id");
    assert_eq!(auth, "Bearer test-token");
    assert_eq!(body["messaging_product"], "whatsapp");
    assert_eq!(body["to"], "919800000001", "no plus sign");
    assert_eq!(body["type"], "template");
    assert_eq!(body["template"]["name"], "studio_code");
    assert_eq!(body["template"]["language"]["code"], "en_US");
    let parts = body["template"]["components"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "body");
    assert_eq!(parts[0]["parameters"][0]["text"], "482913");
    assert_eq!(
        (&parts[1]["type"], &parts[1]["sub_type"], &parts[1]["index"]),
        (&json!("button"), &json!("url"), &json!("0"))
    );
    assert_eq!(parts[1]["parameters"][0]["text"], "482913");
}

#[tokio::test]
async fn failures_never_repeat_the_code() {
    let (meta, client) = stand_in().await;
    let sender = CodeSender::WhatsApp(client.clone());
    let pending = || PendingCode {
        phone: "+919800000001".into(),
        code: "482913".into(),
    };

    // Meta refuses, and (worst case) echoes what we sent in its message.
    *meta.reply.lock().unwrap() = (
        401,
        json!({ "error": { "code": 190, "message": "Bad token for request with text 482913" } }),
    );
    let err = sender.send(pending()).await.unwrap_err().to_string();
    assert!(err.contains("401") && err.contains("190"), "{err}");
    assert!(
        !err.contains("482913"),
        "the code leaked into the error: {err}"
    );
    assert!(!err.contains("test-token"), "the token leaked: {err}");

    // A redirect is refused, not followed (the token would go with it).
    *meta.reply.lock().unwrap() = (302, json!({}));
    let err = sender.send(pending()).await.unwrap_err().to_string();
    assert!(err.contains("302"), "{err}");

    // Nothing listening at all.
    let dead = WhatsAppConfig::from_vars(|name| match name {
        "WHATSAPP_TOKEN" => Some("test-token".into()),
        "WHATSAPP_PHONE_NUMBER_ID" => Some("1".into()),
        "WHATSAPP_API_BASE" => Some("http://127.0.0.1:9".into()),
        _ => None,
    })
    .unwrap()
    .unwrap();
    let err = CodeSender::WhatsApp(WhatsApp::new(dead))
        .send(pending())
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(err, "WhatsApp could not be reached");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_a_staff_number_gets_a_message(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, "+919800000001", StaffRole::Owner).await;
    let (meta, client) = stand_in().await;
    let auth = AuthConfig::new(vec![9u8; 32]).unwrap();
    let state = AppState::with_security(
        pool.clone(),
        auth,
        CodeSender::WhatsApp(client),
        test_security(),
    );
    let app = router(state);
    let ask = |phone: &'static str| {
        let app = app.clone();
        async move {
            let request = axum::http::Request::builder()
                .method(Method::POST)
                .uri("/auth/code")
                .header("content-type", "application/json")
                .header("origin", DASHBOARD)
                .header("sec-fetch-site", "same-origin")
                .body(axum::body::Body::from(
                    json!({ "phone": phone }).to_string(),
                ))
                .unwrap();
            app.oneshot(request).await.unwrap().status()
        }
    };

    // A stranger and a staff member get the same answer...
    assert_eq!(ask("+919811111111").await, StatusCode::ACCEPTED);
    assert_eq!(ask("+919800000001").await, StatusCode::ACCEPTED);
    // ...but only one message leaves, in the background.
    for _ in 0..50 {
        if !meta.seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let seen = meta.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "one message, for the staff number only");
    assert_eq!(seen[0].2["to"], "919800000001");
    let code = seen[0].2["template"]["components"][0]["parameters"][0]["text"]
        .as_str()
        .unwrap();
    assert!(
        code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()),
        "{code}"
    );
}
