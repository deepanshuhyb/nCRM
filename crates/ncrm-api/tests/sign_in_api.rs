//! The sign-in flow and studio-scoped endpoints, through the real router.

mod common;

use axum::http::{Method, StatusCode};
use common::{branch, hire, studio, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::json;
use sqlx::PgPool;

const OWNER: &str = "+919876543210";
const TEACHER: &str = "+919876500001";
const TWO_STUDIOS: &str = "+919876500002";

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_owner_signs_in_and_manages_branches(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    let beta = studio(&pool, "beta").await;
    branch(&pool, &alpha, "Alpha North").await;
    branch(&pool, &beta, "Beta Central").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool.clone());

    let (verified, cookie) = app.sign_in("98765 43210").await;
    assert_eq!(verified.json["studio"]["org_name"], "Studio alpha");
    assert_eq!(verified.json["studio"]["role"], "owner");

    // The cookie is locked down.
    let set_cookie = verified.set_cookie.unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
        assert!(set_cookie.contains(flag), "missing {flag} in {set_cookie}");
    }

    let me = app.call(Method::GET, "/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["phone"], OWNER);
    let caps = me.json["studio"]["capabilities"].as_array().unwrap();
    assert!(caps.contains(&json!("manage_branches")));
    assert!(caps.contains(&json!("manage_billing")));

    // Only this studio's branches.
    let list = app
        .call(Method::GET, "/branches", None, Some(&cookie))
        .await;
    assert_eq!(list.status, StatusCode::OK);
    let names: Vec<&str> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Alpha North"]);

    let created = app
        .call(
            Method::POST,
            "/branches",
            Some(json!({ "name": "Alpha South", "address": "Sector 12" })),
            Some(&cookie),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    assert_eq!(created.json["org_id"], alpha.id.to_string());

    // The change is in the audit log, attributed to the owner.
    let (action, actor_name): (String, String) = sqlx::query_as(
        "SELECT a.action, s.name FROM audit_log a JOIN staff s ON s.id = a.actor_staff_id
          WHERE a.org_id = $1",
    )
    .bind(alpha.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action, "branch.created");
    assert_eq!(actor_name, "Owner");

    // A duplicate name is a clear conflict, not a crash.
    let dup = app
        .call(
            Method::POST,
            "/branches",
            Some(json!({ "name": "Alpha South" })),
            Some(&cookie),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    assert_eq!(dup.error_code(), "conflict");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_instructor_can_look_but_not_change(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    branch(&pool, &alpha, "Alpha North").await;
    hire(&pool, &alpha, TEACHER, StaffRole::Instructor).await;
    let app = TestApp::new(pool);

    let (_, cookie) = app.sign_in(TEACHER).await;
    let list = app
        .call(Method::GET, "/branches", None, Some(&cookie))
        .await;
    assert_eq!(list.status, StatusCode::OK);

    let create = app
        .call(
            Method::POST,
            "/branches",
            Some(json!({ "name": "Rogue" })),
            Some(&cookie),
        )
        .await;
    assert_eq!(create.status, StatusCode::FORBIDDEN);
    assert_eq!(create.error_code(), "not_allowed");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn someone_at_two_studios_must_pick_one_and_only_sees_it(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    let beta = studio(&pool, "beta").await;
    let gamma = studio(&pool, "gamma").await;
    branch(&pool, &alpha, "Alpha North").await;
    branch(&pool, &beta, "Beta Central").await;
    hire(&pool, &alpha, TWO_STUDIOS, StaffRole::Instructor).await;
    hire(&pool, &beta, TWO_STUDIOS, StaffRole::Manager).await;
    let app = TestApp::new(pool);

    let (verified, cookie) = app.sign_in(TWO_STUDIOS).await;
    assert!(verified.json["studio"].is_null());
    assert_eq!(verified.json["memberships"].as_array().unwrap().len(), 2);

    let blocked = app
        .call(Method::GET, "/branches", None, Some(&cookie))
        .await;
    assert_eq!(blocked.status, StatusCode::CONFLICT);
    assert_eq!(blocked.error_code(), "choose_studio");

    let picked = app
        .call(
            Method::POST,
            "/auth/studio",
            Some(json!({ "org_id": beta.id })),
            Some(&cookie),
        )
        .await;
    assert_eq!(picked.status, StatusCode::OK);
    assert_eq!(picked.json["studio"]["role"], "manager");

    let list = app
        .call(Method::GET, "/branches", None, Some(&cookie))
        .await;
    let names: Vec<&str> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Beta Central"]);

    // Switch back: now alpha, as an instructor.
    let switched = app
        .call(
            Method::POST,
            "/auth/studio",
            Some(json!({ "org_id": alpha.id })),
            Some(&cookie),
        )
        .await;
    assert_eq!(switched.json["studio"]["role"], "instructor");

    // A studio they don't work at looks like it doesn't exist.
    let refused = app
        .call(
            Method::POST,
            "/auth/studio",
            Some(json!({ "org_id": gamma.id })),
            Some(&cookie),
        )
        .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn signing_out_ends_the_session(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool);
    let (_, cookie) = app.sign_in(OWNER).await;

    let out = app
        .call(Method::POST, "/auth/sign-out", None, Some(&cookie))
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let cleared = out.set_cookie.expect("cookie cleared");
    assert!(
        cleared.starts_with("ncrm_session=;") || cleared.contains("Max-Age=0"),
        "{cleared}"
    );

    let me = app.call(Method::GET, "/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.error_code(), "not_signed_in");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn requests_without_a_session_are_refused(pool: PgPool) {
    let app = TestApp::new(pool);
    for path in ["/auth/me", "/branches"] {
        let r = app.call(Method::GET, path, None, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(r.error_code(), "not_signed_in");
        let forged = app
            .call(Method::GET, path, None, Some("ncrm_session=made-up-token"))
            .await;
        assert_eq!(forged.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn known_and_unknown_numbers_get_the_same_answer(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool);

    let known = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": OWNER })),
            None,
        )
        .await;
    let unknown = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": "+919111111111" })),
            None,
        )
        .await;
    assert_eq!(known.status, unknown.status);
    assert_eq!(known.json, unknown.json);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn wrong_codes_and_bad_input_get_clear_errors(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool);

    app.call(
        Method::POST,
        "/auth/code",
        Some(json!({ "phone": OWNER })),
        None,
    )
    .await;
    let code = app.code_for(OWNER).await;
    let wrong = format!("{:06}", (code.parse::<u32>().unwrap() + 1) % 1_000_000);
    let r = app
        .call(
            Method::POST,
            "/auth/verify",
            Some(json!({ "phone": OWNER, "code": wrong })),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.error_code(), "invalid_code");
    assert!(r.set_cookie.is_none());

    let bad_phone = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": "not a phone" })),
            None,
        )
        .await;
    assert_eq!(bad_phone.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad_phone.error_code(), "invalid_input");

    let missing_field = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "mobile": OWNER })),
            None,
        )
        .await;
    assert_eq!(missing_field.status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_field.error_code(), "invalid_input");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn hammering_the_code_endpoint_is_rate_limited(pool: PgPool) {
    let app = TestApp::new(pool);
    let mut last = None;
    for _ in 0..6 {
        last = Some(
            app.call(
                Method::POST,
                "/auth/code",
                Some(json!({ "phone": OWNER })),
                None,
            )
            .await,
        );
    }
    let last = last.unwrap();
    assert_eq!(last.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(last.error_code(), "too_many_requests");
}
