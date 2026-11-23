//! Fees through the HTTP API: charges, payments, receipts, cancelling,
//! each role, bad input, another studio.

mod common;

use axum::http::{Method, StatusCode};
use common::{hire, studio, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::{json, Value};
use sqlx::PgPool;

const OWNER: &str = "+919800000001";
const DESK: &str = "+919800000003";
const TEACHER: &str = "+919800000004";
const ELSEWHERE: &str = "+919800000009";

#[sqlx::test(migrator = "MIGRATOR")]
async fn taking_money_at_the_front_desk(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    for (phone, role) in [
        (OWNER, StaffRole::Owner),
        (DESK, StaffRole::FrontDesk),
        (TEACHER, StaffRole::Instructor),
    ] {
        hire(&pool, &alpha, phone, role).await;
    }
    let beta = studio(&pool, "beta").await;
    hire(&pool, &beta, ELSEWHERE, StaffRole::Owner).await;
    let app = TestApp::new(pool.clone());
    let owner = app.sign_in(OWNER).await.1;
    let desk = app.sign_in(DESK).await.1;
    let teacher = app.sign_in(TEACHER).await.1;
    let elsewhere = app.sign_in(ELSEWHERE).await.1;
    let get = |path: String, who: String| {
        let app = &app;
        async move { app.call(Method::GET, &path, None, Some(&who)).await }
    };
    let send = |method: Method, path: String, body: Value, who: String| {
        let app = &app;
        async move { app.call(method, &path, Some(body), Some(&who)).await }
    };

    // Who may cancel is a capability the dashboard can read.
    let caps = |me: Value| {
        me["studio"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "adjust_fees")
    };
    assert!(caps(get("/auth/me".into(), owner.clone()).await.json));
    assert!(!caps(get("/auth/me".into(), desk.clone()).await.json));

    let kid = send(
        Method::POST,
        "/students".into(),
        json!({ "name": "Bela" }),
        desk.clone(),
    )
    .await
    .json["id"]
        .as_str()
        .unwrap()
        .to_string();

    // A charge, then a part payment.
    let charged = send(
        Method::POST,
        format!("/students/{kid}/charges"),
        json!({ "description": "Costume", "amount_paise": 50000 }),
        desk.clone(),
    )
    .await;
    assert_eq!(charged.status, StatusCode::CREATED, "{:?}", charged.json);
    assert_eq!(charged.json["due_paise"], 50000);
    let charge_id = charged.json["charges"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let paid = send(
        Method::POST,
        format!("/students/{kid}/payments"),
        json!({ "amount_paise": 30000, "method": "upi", "reference": "UPI 991" }),
        desk.clone(),
    )
    .await;
    assert_eq!(paid.status, StatusCode::CREATED, "{:?}", paid.json);
    assert_eq!(paid.json["payment"]["receipt_no"], 1);
    assert_eq!(paid.json["payment"]["method"], "upi");
    assert_eq!(paid.json["lines"][0]["description"], "Costume");
    assert_eq!(paid.json["lines"][0]["amount_paise"], 30000);
    assert_eq!(paid.json["still_due_paise"], 20000);
    assert_eq!(paid.json["studio_name"], alpha.name);
    let payment_id = paid.json["payment"]["id"].as_str().unwrap().to_string();

    // The receipt can be opened again.
    let again = get(format!("/payments/{payment_id}"), desk.clone()).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json["payment"]["student_name"], "Bela");

    // Bodies that don't make sense get a sentence, not a stack trace.
    for (body, needle) in [
        (
            json!({ "amount_paise": 100, "method": "bitcoin" }),
            "method",
        ),
        (
            json!({ "amount_paise": "lots", "method": "cash" }),
            "amount_paise",
        ),
        (json!({ "method": "cash" }), "amount_paise"),
    ] {
        let r = send(
            Method::POST,
            format!("/students/{kid}/payments"),
            body,
            desk.clone(),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
        let message = r.json["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains(needle), "{message}");
    }
    let zero = send(
        Method::POST,
        format!("/students/{kid}/payments"),
        json!({ "amount_paise": 0, "method": "cash" }),
        desk.clone(),
    )
    .await;
    assert_eq!(zero.status, StatusCode::BAD_REQUEST);
    assert_eq!(zero.error_code(), "invalid_input");

    // The lists.
    let dues = get("/fees/dues".into(), desk.clone()).await;
    assert_eq!(dues.status, StatusCode::OK);
    assert_eq!(dues.json["total"], 1);
    assert_eq!(dues.json["items"][0]["student_name"], "Bela");
    assert_eq!(dues.json["items"][0]["due_paise"], 20000);
    let today = get("/payments".into(), desk.clone()).await;
    assert_eq!(today.json["collected_paise"], 30000);
    assert_eq!(today.json["by_method"][0]["method"], "upi");
    assert_eq!(
        get("/payments?from=yesterday".into(), desk.clone())
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let home = get("/summary".into(), desk.clone()).await.json;
    assert_eq!(
        (home["dues_paise"].as_i64(), home["students_owing"].as_i64()),
        (Some(20000), Some(1))
    );
    assert_eq!(
        get("/fees/summary".into(), desk.clone()).await.json["collected_this_month_paise"],
        Value::Null
    );
    assert_eq!(
        get("/fees/summary".into(), owner.clone()).await.json["collected_this_month_paise"],
        30000
    );

    // Teachers see none of it, and their home page has no money on it.
    let home = get("/summary".into(), teacher.clone()).await.json;
    assert_eq!(home["dues_paise"], Value::Null);
    for path in [
        "/fees/summary".to_string(),
        "/fees/dues".to_string(),
        "/payments".to_string(),
        format!("/payments/{payment_id}"),
        format!("/students/{kid}/fees"),
    ] {
        let r = get(path.clone(), teacher.clone()).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}");
    }
    let r = send(
        Method::POST,
        format!("/students/{kid}/payments"),
        json!({ "amount_paise": 100, "method": "cash" }),
        teacher.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Cancelling: not the front desk; with a reason; once.
    let reason = json!({ "reason": "Entered twice" });
    for path in [
        format!("/payments/{payment_id}/void"),
        format!("/charges/{charge_id}/void"),
    ] {
        let r = send(Method::POST, path, reason.clone(), desk.clone()).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    let no_reason = send(
        Method::POST,
        format!("/payments/{payment_id}/void"),
        json!({ "reason": "" }),
        owner.clone(),
    )
    .await;
    assert_eq!(no_reason.status, StatusCode::BAD_REQUEST);
    let voided = send(
        Method::POST,
        format!("/payments/{payment_id}/void"),
        reason.clone(),
        owner.clone(),
    )
    .await;
    assert_eq!(voided.status, StatusCode::OK, "{:?}", voided.json);
    assert_eq!(voided.json["due_paise"], 50000);
    assert_eq!(voided.json["payments"][0]["void_reason"], "Entered twice");
    let twice = send(
        Method::POST,
        format!("/payments/{payment_id}/void"),
        reason.clone(),
        owner.clone(),
    )
    .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);

    // Another studio can't see, cancel or pay into any of it; ids that
    // aren't ids are simply not found.
    for path in [
        format!("/payments/{payment_id}"),
        format!("/students/{kid}/fees"),
        "/payments/not-an-id".to_string(),
    ] {
        let r = get(path.clone(), elsewhere.clone()).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
    }
    for path in [
        format!("/charges/{charge_id}/void"),
        format!("/payments/{payment_id}/void"),
    ] {
        let r = send(Method::POST, path, reason.clone(), elsewhere.clone()).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    let r = send(
        Method::POST,
        format!("/students/{kid}/payments"),
        json!({ "amount_paise": 100, "method": "cash" }),
        elsewhere.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        get("/fees/dues".into(), elsewhere.clone()).await.json["total"],
        0
    );

    // Monthly fees are switched on by owners and managers.
    let home = get("/summary".into(), owner.clone()).await.json;
    let month = format!("{}-01", &home["today"].as_str().unwrap()[..7]);
    let settings = json!({ "fees_from": month, "fee_due_day": 10 });
    let r = send(
        Method::PUT,
        "/fees/settings".into(),
        settings.clone(),
        desk.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let on = send(
        Method::PUT,
        "/fees/settings".into(),
        settings,
        owner.clone(),
    )
    .await;
    assert_eq!(on.status, StatusCode::OK, "{:?}", on.json);
    assert_eq!(on.json["fees_from"], month);
    assert_eq!(on.json["fee_due_day"], 10);

    // Selling a pack and taking the money in one go.
    let plan = send(
        Method::POST,
        "/plans".into(),
        json!({ "name": "Four classes", "kind": "pack", "price_paise": 120000, "classes": 4, "valid_days": 30 }),
        owner.clone(),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{:?}", plan.json);
    let plan_id = plan.json["id"].as_str().unwrap().to_string();
    let sold = send(
        Method::POST,
        format!("/students/{kid}/credits/sell"),
        json!({ "plan_id": plan_id, "paid": { "method": "cash" } }),
        desk.clone(),
    )
    .await;
    assert!(sold.status.is_success(), "{:?}", sold.json);
    let fees = get(format!("/students/{kid}/fees"), desk.clone())
        .await
        .json;
    // ₹1,200 came in for the pack and pays the pack; the costume is still
    // owed.
    assert_eq!(fees["due_paise"], 50000);
    let paid_of = |what: &str| {
        fees["charges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["description"] == what)
            .map(|c| c["paid_paise"].as_i64().unwrap())
    };
    assert_eq!(paid_of("Four classes"), Some(120000));
    assert_eq!(paid_of("Costume"), Some(0));
    assert_eq!(fees["payments"][0]["receipt_no"], 2);
    assert_eq!(fees["payments"][0]["amount_paise"], 120000);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_discount_is_for_owners_and_has_limits(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    hire(&pool, &alpha, DESK, StaffRole::FrontDesk).await;
    let beta = studio(&pool, "beta").await;
    hire(&pool, &beta, ELSEWHERE, StaffRole::Owner).await;
    let app = TestApp::new(pool.clone());
    let owner = app.sign_in(OWNER).await.1;
    let desk = app.sign_in(DESK).await.1;
    let elsewhere = app.sign_in(ELSEWHERE).await.1;
    let post = |path: String, body: Value, who: Option<String>| {
        let app = &app;
        async move {
            app.call(Method::POST, &path, Some(body), who.as_deref())
                .await
        }
    };

    let kid = post(
        "/students".into(),
        json!({ "name": "Bela" }),
        Some(desk.clone()),
    )
    .await
    .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let fees = post(
        format!("/students/{kid}/charges"),
        json!({ "description": "Term fee", "amount_paise": 250000 }),
        Some(desk.clone()),
    )
    .await;
    assert_eq!(fees.status, StatusCode::CREATED, "{:?}", fees.json);
    let charge = fees.json["charges"][0]["id"].as_str().unwrap().to_string();
    let path = format!("/charges/{charge}/discount");
    let sibling = json!({ "off_paise": 50000, "reason": "Sibling" });

    // Not the front desk, not signed out, not another studio's owner.
    assert_eq!(
        post(path.clone(), sibling.clone(), Some(desk.clone()))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(path.clone(), sibling.clone(), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(path.clone(), sibling.clone(), Some(elsewhere))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // No reason, all of it, and a body that isn't a discount.
    for bad in [
        json!({ "off_paise": 50000, "reason": " " }),
        json!({ "off_paise": 250000, "reason": "Sibling" }),
        json!({ "reason": "Sibling" }),
    ] {
        let r = post(path.clone(), bad, Some(owner.clone())).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }

    let done = post(path.clone(), sibling, Some(owner.clone())).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["due_paise"], 200000);
    assert_eq!(done.json["charges"][0]["amount_paise"], 200000);
    assert_eq!(done.json["charges"][0]["discount_paise"], 50000);
    assert_eq!(done.json["charges"][0]["discount_reason"], "Sibling");
}
