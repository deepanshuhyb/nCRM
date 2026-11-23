//! Broadcasts and the fee-reminder switch through the HTTP API.

mod common;

use axum::http::{Method, StatusCode};
use common::{hire, studio, TestApp};
use ncrm_core::db::{TenantTx, MIGRATOR};
use ncrm_core::staff::StaffRole;
use serde_json::json;
use sqlx::PgPool;

const OWNER: &str = "+919800000001";
const DESK: &str = "+919800000002";

/// A studio with an owner, a front desk, and two students who each have
/// a main contact.
async fn seeded(pool: &PgPool) {
    let org = studio(pool, "cast").await;
    hire(pool, &org, OWNER, StaffRole::Owner).await;
    hire(pool, &org, DESK, StaffRole::FrontDesk).await;
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    for (student, contact, phone) in [
        ("Aarav", "Meena", "+919811100001"),
        ("Bela", "Ravi", "+919811100002"),
    ] {
        sqlx::query(
            "WITH s AS (INSERT INTO students (org_id, name, joined_on)
                        VALUES ($1, $2, current_date) RETURNING id),
                  k AS (INSERT INTO contacts (org_id, name, phone)
                        VALUES ($1, $3, $4) RETURNING id)
             INSERT INTO student_contacts (org_id, student_id, contact_id, relation, is_primary)
             SELECT $1, s.id, k.id, 'other', true FROM s, k",
        )
        .bind(org.id)
        .bind(student)
        .bind(contact)
        .bind(phone)
        .execute(tx.conn())
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn queued(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM messages WHERE broadcast_id IS NOT NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_owner_previews_then_sends_and_others_cannot(pool: PgPool) {
    seeded(&pool).await;
    let app = TestApp::new(pool.clone());
    let (_, owner) = app.sign_in(OWNER).await;
    let (_, desk) = app.sign_in(DESK).await;
    let everyone = json!({ "kind": "everyone" });

    let preview = app
        .call(
            Method::POST,
            "/broadcasts/preview",
            Some(everyone.clone()),
            Some(&owner),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    assert_eq!(preview.json["recipients"], 2);
    assert_eq!(preview.json["label"], "Everyone");
    assert_eq!(preview.json["sample"], json!(["Meena", "Ravi"]));
    assert_eq!(queued(&pool).await, 0);

    let sent = app
        .call(
            Method::POST,
            "/broadcasts",
            Some(json!({ "audience": everyone, "text": "No class tomorrow." })),
            Some(&owner),
        )
        .await;
    assert_eq!(sent.status, StatusCode::CREATED, "{:?}", sent.json);
    assert_eq!(sent.json["recipients"], 2);
    assert_eq!(sent.json["body"], "No class tomorrow.");
    assert_eq!(queued(&pool).await, 2);

    // The same again at once is refused, and nothing more is queued.
    let again = app
        .call(
            Method::POST,
            "/broadcasts",
            Some(json!({ "audience": everyone, "text": "No class tomorrow." })),
            Some(&owner),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(queued(&pool).await, 2);

    let history = app
        .call(Method::GET, "/broadcasts", None, Some(&owner))
        .await;
    assert_eq!(history.json.as_array().unwrap().len(), 1);
    assert_eq!(history.json[0]["audience_label"], "Everyone");

    // Bad input: an audience that isn't one, a class that isn't an id.
    for body in [
        json!({ "audience": { "kind": "the_whole_city" }, "text": "Hi" }),
        json!({ "audience": { "kind": "batch", "batch_id": "nope" }, "text": "Hi" }),
        json!({ "audience": everyone }),
    ] {
        let r = app
            .call(Method::POST, "/broadcasts", Some(body), Some(&owner))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
    let empty = app
        .call(
            Method::POST,
            "/broadcasts",
            Some(json!({ "audience": everyone, "text": "   " })),
            Some(&owner),
        )
        .await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);

    // Front desk, and nobody at all.
    for (path, method, body) in [
        ("/broadcasts", Method::GET, None),
        ("/broadcasts/preview", Method::POST, Some(everyone.clone())),
        (
            "/broadcasts",
            Method::POST,
            Some(json!({ "audience": everyone, "text": "Sale!" })),
        ),
    ] {
        let r = app
            .call(method.clone(), path, body.clone(), Some(&desk))
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}");
        let r = app.call(method, path, body, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    assert_eq!(queued(&pool).await, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_fee_reminder_switch_is_part_of_fee_settings(pool: PgPool) {
    seeded(&pool).await;
    let app = TestApp::new(pool.clone());
    let (_, owner) = app.sign_in(OWNER).await;
    let (_, desk) = app.sign_in(DESK).await;

    let before = app
        .call(Method::GET, "/fees/summary", None, Some(&owner))
        .await;
    assert_eq!(before.json["fee_reminders"], true);

    let off = app
        .call(
            Method::PUT,
            "/fees/settings",
            Some(json!({ "fees_from": null, "fee_due_day": 5, "fee_reminders": false })),
            Some(&owner),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{:?}", off.json);
    assert_eq!(off.json["fee_reminders"], false);

    // An older dashboard that doesn't send the switch leaves it alone.
    let untouched = app
        .call(
            Method::PUT,
            "/fees/settings",
            Some(json!({ "fees_from": null, "fee_due_day": 7 })),
            Some(&owner),
        )
        .await;
    assert_eq!(untouched.json["fee_reminders"], false);
    assert_eq!(untouched.json["fee_due_day"], 7);

    let denied = app
        .call(
            Method::PUT,
            "/fees/settings",
            Some(json!({ "fees_from": null, "fee_due_day": 5, "fee_reminders": true })),
            Some(&desk),
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}
