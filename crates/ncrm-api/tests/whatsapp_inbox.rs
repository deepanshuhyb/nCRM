//! The WhatsApp webhook, the inbox and the bot through the HTTP API,
//! against real Postgres. Meta and the AI provider are played by small
//! servers on this machine; nothing leaves it.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use common::{hire, studio, test_security, TestApp};
use ncrm_core::bot::{Ai, AiConfig};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::orgs::Organization;
use ncrm_core::staff::StaffRole;
use ncrm_core::whatsapp::WebhookConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

const OWNER: &str = "+919800000001";
const DESK: &str = "+919800000002";
const TEACHER: &str = "+919800000003";
const PARENT: &str = "919811100001";
const NUMBER_ID: &str = "555000111";

fn hook() -> WebhookConfig {
    WebhookConfig {
        verify_token: "a-long-verify-token-123".into(),
        app_secret: "0123456789abcdef0123456789abcdef".into(),
    }
}

/// A studio with a WhatsApp number, three staff, and one student whose
/// mother is `PARENT`.
async fn seeded(pool: &PgPool, slug: &str, number_id: &str) -> Organization {
    let org = studio(pool, slug).await;
    let n = slug.len() % 10;
    hire(
        pool,
        &org,
        &OWNER.replace("0000000", &format!("00000{n}0")),
        StaffRole::Owner,
    )
    .await;
    hire(
        pool,
        &org,
        &DESK.replace("0000000", &format!("00000{n}0")),
        StaffRole::FrontDesk,
    )
    .await;
    hire(
        pool,
        &org,
        &TEACHER.replace("0000000", &format!("00000{n}0")),
        StaffRole::Instructor,
    )
    .await;
    sqlx::query("UPDATE organizations SET wa_phone_number_id = $2 WHERE id = $1")
        .bind(org.id)
        .bind(number_id)
        .execute(pool)
        .await
        .unwrap();
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    let student: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO students (org_id, name, joined_on) VALUES ($1, 'Aarav', current_date) RETURNING id")
            .bind(org.id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    let contact: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO contacts (org_id, name, phone) VALUES ($1, 'Meena', $2) RETURNING id",
    )
    .bind(org.id)
    .bind(format!("+{PARENT}"))
    .fetch_one(tx.conn())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO student_contacts (org_id, student_id, contact_id, relation, is_primary)
         VALUES ($1, $2, $3, 'mother', true)",
    )
    .bind(org.id)
    .bind(student)
    .bind(contact)
    .execute(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();
    org
}

fn phone(base: &str, slug: &str) -> String {
    base.replace("0000000", &format!("00000{}0", slug.len() % 10))
}

fn delivery(number_id: &str, from: &str, id: &str, text: &str) -> Value {
    json!({ "object": "whatsapp_business_account", "entry": [{ "id": "1", "changes": [{
    "field": "messages", "value": {
        "metadata": { "phone_number_id": number_id },
        "contacts": [{ "profile": { "name": "Meena on WhatsApp" }, "wa_id": from }],
        "messages": [{ "from": from, "id": id, "type": "text", "text": { "body": text } }]
    }}]}]})
}

/// POST a delivery the way Meta does: no cookie, no Origin, signed.
async fn deliver(app: &TestApp, body: &Value, signature: Option<String>) -> StatusCode {
    let bytes = serde_json::to_vec(body).unwrap();
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/webhooks/whatsapp")
        .header("content-type", "application/json");
    if let Some(s) = signature.or_else(|| Some(hook().sign(&bytes))) {
        req = req.header("x-hub-signature-256", s);
    }
    app.router
        .clone()
        .oneshot(req.body(Body::from(bytes)).unwrap())
        .await
        .unwrap()
        .status()
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Wait for the bot (which answers after the webhook has replied).
async fn wait_for(pool: &PgPool, sql: &'static str, at_least: i64) {
    for _ in 0..100 {
        if count(pool, sql).await >= at_least {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("never reached {at_least}: {sql}");
}

fn app(pool: &PgPool) -> TestApp {
    TestApp::with_state(pool.clone(), test_security(), |state| {
        state.webhook = Some(Arc::new(hook()));
    })
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_webhook_is_closed_until_configured_and_proves_itself_to_meta(pool: PgPool) {
    let closed = TestApp::new(pool.clone());
    let r = closed
        .call(
            Method::GET,
            "/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=x&hub.challenge=1",
            None,
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        deliver(&closed, &delivery(NUMBER_ID, PARENT, "wamid.1", "hi"), None).await,
        StatusCode::NOT_FOUND
    );

    let open = app(&pool);
    let ok = open
        .call(
            Method::GET,
            "/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=a-long-verify-token-123&hub.challenge=4242",
            None,
            None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.json, json!(4242));
    let wrong = open
        .call(
            Method::GET,
            "/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=guess&hub.challenge=4242",
            None,
            None,
        )
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_signed_deliveries_are_stored_and_each_only_once(pool: PgPool) {
    let org = seeded(&pool, "hook", NUMBER_ID).await;
    let other = seeded(&pool, "neighbour", "999000111").await;
    let app = app(&pool);
    let body = delivery(NUMBER_ID, PARENT, "wamid.1", "Hi");

    // Unsigned, wrongly signed, and signed-for-another-body: all refused,
    // and nothing is written.
    assert_eq!(
        deliver(&app, &body, Some(String::new())).await,
        StatusCode::UNAUTHORIZED
    );
    let forged = WebhookConfig {
        app_secret: "ffffffffffffffffffffffffffffffff".into(),
        ..hook()
    }
    .sign(&serde_json::to_vec(&body).unwrap());
    assert_eq!(
        deliver(&app, &body, Some(forged)).await,
        StatusCode::UNAUTHORIZED
    );
    let other_body = hook().sign(b"{}");
    assert_eq!(
        deliver(&app, &body, Some(other_body)).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM messages").await, 0);

    // Signed: stored for the studio that owns the number, and the bot
    // answers. Meta retrying the same delivery adds nothing.
    assert_eq!(deliver(&app, &body, None).await, StatusCode::OK);
    assert_eq!(deliver(&app, &body, None).await, StatusCode::OK);
    wait_for(
        &pool,
        "SELECT count(*) FROM messages WHERE author = 'bot' AND status = 'sent'",
        1,
    )
    .await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM messages WHERE direction = 'in'"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM messages WHERE author = 'bot'").await,
        1
    );
    let owner: uuid::Uuid = sqlx::query_scalar("SELECT DISTINCT org_id FROM messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, org.id);
    let reply: String = sqlx::query_scalar("SELECT body FROM messages WHERE author = 'bot'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(reply.contains("Hi Meena, this is Studio hook"), "{reply}");

    // A delivery can't choose its studio: a number nobody has is dropped
    // (with a 200, so Meta stops retrying), and the neighbour got nothing.
    assert_eq!(
        deliver(&app, &delivery("123123123", PARENT, "wamid.2", "Hi"), None).await,
        StatusCode::OK
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM messages WHERE direction = 'in'"
        )
        .await,
        1
    );
    let theirs: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations WHERE org_id = $1")
        .bind(other.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(theirs, 0);

    // Signed nonsense is accepted and ignored.
    assert_eq!(
        deliver(&app, &json!({ "entry": 7 }), None).await,
        StatusCode::OK
    );

    // A delivery report moves our message forward.
    sqlx::query("UPDATE messages SET wa_message_id = 'wamid.out' WHERE author = 'bot'")
        .execute(&pool)
        .await
        .unwrap();
    let report = json!({ "entry": [{ "changes": [{ "value": {
        "metadata": { "phone_number_id": NUMBER_ID },
        "statuses": [{ "id": "wamid.out", "status": "read" }]
    }}]}]});
    assert_eq!(deliver(&app, &report, None).await, StatusCode::OK);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM messages WHERE status = 'read'").await,
        1
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn staff_read_and_answer_in_the_inbox_within_their_role_and_studio(pool: PgPool) {
    seeded(&pool, "desk", NUMBER_ID).await;
    seeded(&pool, "elsewhere", "999000111").await;
    let app = app(&pool);
    deliver(
        &app,
        &delivery(NUMBER_ID, PARENT, "wamid.1", "Aarav twisted his ankle"),
        None,
    )
    .await;
    wait_for(
        &pool,
        "SELECT count(*) FROM conversations WHERE needs_staff",
        1,
    )
    .await;

    let (_, desk) = app.sign_in(&phone(DESK, "desk")).await;
    let inbox = app.call(Method::GET, "/inbox", None, Some(&desk)).await;
    assert_eq!(inbox.status, StatusCode::OK);
    assert_eq!(inbox.json[0]["contact_name"], "Meena");
    assert_eq!(inbox.json[0]["students"], "Aarav");
    assert_eq!(inbox.json[0]["needs_staff"], true);
    let id = inbox.json[0]["id"].as_str().unwrap().to_string();

    let thread = app
        .call(Method::GET, &format!("/inbox/{id}"), None, Some(&desk))
        .await;
    assert_eq!(
        thread.json["messages"][0]["body"],
        "Aarav twisted his ankle"
    );

    let sent = app
        .call(
            Method::POST,
            &format!("/inbox/{id}/reply"),
            Some(json!({ "text": "Sorry to hear that. We'll mark him excused." })),
            Some(&desk),
        )
        .await;
    assert_eq!(sent.status, StatusCode::CREATED, "{:?}", sent.json);
    assert_eq!(sent.json["needs_staff"], false);
    assert_eq!(sent.json["bot_paused"], true);
    // With WhatsApp not configured the reply is "sent" in test mode.
    wait_for(
        &pool,
        "SELECT count(*) FROM messages WHERE author = 'staff' AND status = 'sent'",
        1,
    )
    .await;

    let empty = app
        .call(
            Method::POST,
            &format!("/inbox/{id}/reply"),
            Some(json!({ "text": " " })),
            Some(&desk),
        )
        .await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);

    let back = app
        .call(
            Method::POST,
            &format!("/inbox/{id}/hand-back"),
            None,
            Some(&desk),
        )
        .await;
    assert_eq!(back.json["bot_paused"], false);

    // Settings: everyone with the inbox can read them, owners change them.
    let settings = app
        .call(Method::GET, "/messaging/settings", None, Some(&desk))
        .await;
    assert_eq!(settings.json["bot_mode"], "menu");
    assert_eq!(settings.json["connected"], true);
    assert_eq!(settings.json["live"], false);
    assert_eq!(settings.json["ai_available"], false);
    let denied = app
        .call(
            Method::PUT,
            "/messaging/settings",
            Some(json!({ "bot_mode": "off" })),
            Some(&desk),
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let (_, owner) = app.sign_in(&phone(OWNER, "desk")).await;
    let changed = app
        .call(
            Method::PUT,
            "/messaging/settings",
            Some(json!({ "bot_mode": "ai" })),
            Some(&owner),
        )
        .await;
    assert_eq!(changed.json["bot_mode"], "ai");
    let bad = app
        .call(
            Method::PUT,
            "/messaging/settings",
            Some(json!({ "bot_mode": "loud" })),
            Some(&owner),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Instructors have no inbox; nobody reads it signed out.
    let (_, teacher) = app.sign_in(&phone(TEACHER, "desk")).await;
    for path in [
        "/inbox".to_string(),
        format!("/inbox/{id}"),
        "/messaging/settings".into(),
    ] {
        let r = app.call(Method::GET, &path, None, Some(&teacher)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}");
        let r = app.call(Method::GET, &path, None, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    // Another studio's owner can't see or answer this conversation.
    let (_, outsider) = app.sign_in(&phone(OWNER, "elsewhere")).await;
    let none = app.call(Method::GET, "/inbox", None, Some(&outsider)).await;
    assert_eq!(none.json, json!([]));
    let r = app
        .call(Method::GET, &format!("/inbox/{id}"), None, Some(&outsider))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = app
        .call(
            Method::POST,
            &format!("/inbox/{id}/reply"),
            Some(json!({ "text": "hello" })),
            Some(&outsider),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = app
        .call(Method::GET, "/inbox/not-an-id", None, Some(&desk))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// AI mode, against a stand-in for the provider
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Provider {
    /// (x-api-key, request body) for every call.
    seen: Arc<Mutex<Vec<(String, Value)>>>,
    /// The status and text to answer with.
    reply: Arc<Mutex<(u16, String)>>,
}

async fn complete(
    State(p): State<Provider>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    p.seen.lock().unwrap().push((key, body));
    let (status, text) = p.reply.lock().unwrap().clone();
    (
        StatusCode::from_u16(status).unwrap(),
        Json(json!({ "content": [{ "type": "text", "text": text }] })),
    )
}

async fn provider() -> (Provider, Ai) {
    let p = Provider::default();
    let router = Router::new()
        .route("/v1/messages", post(complete))
        .with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let config = AiConfig::from_vars(|name| match name {
        "AI_API_KEY" => Some("test-ai-key".into()),
        "AI_API_BASE" => Some(format!("http://127.0.0.1:{port}")),
        _ => None,
    })
    .unwrap()
    .unwrap();
    (p, Ai::new(config))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_ai_answers_from_the_senders_own_facts_and_the_menu_covers_for_it(pool: PgPool) {
    let org = seeded(&pool, "ai", NUMBER_ID).await;
    // A second family in the same studio, whom the AI must not hear about.
    let mut tx = TenantTx::begin(&pool, org.id).await.unwrap();
    sqlx::query(
        "INSERT INTO students (org_id, name, joined_on) VALUES ($1, 'Zoya Secret', current_date)",
    )
    .bind(org.id)
    .execute(tx.conn())
    .await
    .unwrap();
    sqlx::query("UPDATE organizations SET bot_mode = 'ai' WHERE id = $1")
        .bind(org.id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let (provider, ai) = provider().await;
    let app = TestApp::with_state(pool.clone(), test_security(), |state| {
        state.webhook = Some(Arc::new(hook()));
        state.ai = Some(ai);
    });
    let bot_said = |n: i64| {
        let pool = pool.clone();
        async move {
            wait_for(
                &pool,
                "SELECT count(*) FROM messages WHERE author = 'bot'",
                n,
            )
            .await;
            sqlx::query_scalar::<_, String>(
                "SELECT body FROM messages WHERE author = 'bot' ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    // 1. The model's words are sent, and it was told only about Aarav.
    *provider.reply.lock().unwrap() = (200, "Aarav has nothing to pay right now.".into());
    deliver(
        &app,
        &delivery(NUMBER_ID, PARENT, "wamid.1", "Do we owe anything?"),
        None,
    )
    .await;
    assert_eq!(bot_said(1).await, "Aarav has nothing to pay right now.");
    let (key, request) = provider.seen.lock().unwrap()[0].clone();
    assert_eq!(key, "test-ai-key");
    let system = request["system"].as_str().unwrap();
    assert!(system.contains("Aarav") && system.contains("Studio ai"));
    assert!(
        !system.contains("Zoya"),
        "another family's student reached the AI"
    );
    assert_eq!(request["messages"][0]["role"], "user");
    assert_eq!(request["messages"][0]["content"], "Do we owe anything?");

    // 2. The provider is down: the menu answers instead.
    *provider.reply.lock().unwrap() = (500, String::new());
    deliver(&app, &delivery(NUMBER_ID, PARENT, "wamid.2", "hello"), None).await;
    assert!(bot_said(2).await.contains("Reply with a number"));

    // 3. The model asks for a person: the marker is removed and staff are flagged.
    *provider.reply.lock().unwrap() = (200, "The team will reply here. [[TEAM]]".into());
    deliver(
        &app,
        &delivery(NUMBER_ID, PARENT, "wamid.3", "I want a refund"),
        None,
    )
    .await;
    assert_eq!(bot_said(3).await, "The team will reply here.");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM conversations WHERE needs_staff"
        )
        .await,
        1
    );

    // 4. A number the studio doesn't know: the AI hears about no student at all.
    deliver(
        &app,
        &delivery(NUMBER_ID, "919822200002", "wamid.4", "what does Aarav owe?"),
        None,
    )
    .await;
    wait_for(
        &pool,
        "SELECT count(*) FROM messages WHERE author = 'bot'",
        4,
    )
    .await;
    let (_, request) = provider.seen.lock().unwrap().last().unwrap().clone();
    let system = request["system"].as_str().unwrap();
    assert!(system.contains("not in the studio's records"));
    // (The customer's own words are in `messages`, not in what we know.)
    assert!(!system.contains("Aarav") && !system.contains("Zoya") && !system.contains('₹'));
}
