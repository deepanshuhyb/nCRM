//! Browser-facing protections: origin check, per-IP limits, response
//! headers, body size. Each test tries to get around one of them.

mod common;

use axum::http::{HeaderName, Method, StatusCode};
use common::{branch, hire, studio, test_security, TestApp, DASHBOARD};
use ncrm_api::security::SecurityConfig;
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::json;
use sqlx::PgPool;

const OWNER: &str = "+919876543210";

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_site_cannot_make_changes_with_the_owners_cookie(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    branch(&pool, &alpha, "Alpha North").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool.clone());
    let (_, cookie) = app.sign_in(OWNER).await;

    for origin in [
        "https://evil.example",
        "null",
        "https://app.ncrm.test.evil.example",
    ] {
        let r = app
            .call_with(
                Method::POST,
                "/branches",
                Some(json!({ "name": "Planted" })),
                Some(&cookie),
                Some(origin),
                &[],
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "origin {origin}");
        assert_eq!(r.error_code(), "bad_origin");
    }

    // A cross-site request that hides its Origin is caught by Fetch Metadata.
    let r = app
        .call_with(
            Method::POST,
            "/branches",
            Some(json!({ "name": "Planted" })),
            Some(&cookie),
            None,
            &[("sec-fetch-site", "cross-site")],
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let planted: i64 = sqlx::query_scalar("SELECT count(*) FROM branches WHERE name = 'Planted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(planted, 0);

    // Our own dashboard still works.
    let ok = app
        .call_with(
            Method::POST,
            "/branches",
            Some(json!({ "name": "Alpha South" })),
            Some(&cookie),
            Some(DASHBOARD),
            &[("sec-fetch-site", "same-origin")],
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sign_out_from_another_site_is_refused(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, OWNER, StaffRole::Owner).await;
    let app = TestApp::new(pool);
    let (_, cookie) = app.sign_in(OWNER).await;

    let r = app
        .call_with(
            Method::POST,
            "/auth/sign-out",
            None,
            Some(&cookie),
            Some("https://evil.example"),
            &[],
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let me = app.call(Method::GET, "/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.status, StatusCode::OK, "still signed in");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn non_browser_clients_without_an_origin_still_work(pool: PgPool) {
    let app = TestApp::new(pool);
    let r = app
        .call_with(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": OWNER })),
            None,
            None,
            &[],
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn no_cross_origin_reads_are_offered(pool: PgPool) {
    let app = TestApp::new(pool);
    let r = app
        .call_with(
            Method::GET,
            "/healthz",
            None,
            None,
            Some("https://evil.example"),
            &[],
        )
        .await;
    assert!(r.header("access-control-allow-origin").is_none());
    assert!(r.header("access-control-allow-credentials").is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_response_carries_the_protective_headers(pool: PgPool) {
    let app = TestApp::new(pool);
    let ok = app.call(Method::GET, "/healthz", None, None).await;
    let err = app.call(Method::GET, "/auth/me", None, None).await;
    let refused = app
        .call_with(
            Method::POST,
            "/auth/code",
            Some(json!({})),
            None,
            Some("https://evil.example"),
            &[],
        )
        .await;
    for r in [&ok, &err, &refused] {
        assert_eq!(r.header("cache-control"), Some("no-store"));
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(r.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(
            r.header("content-security-policy"),
            Some("default-src 'none'; frame-ancestors 'none'")
        );
        assert_eq!(
            r.header("cross-origin-resource-policy"),
            Some("same-origin")
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn oversized_bodies_are_refused(pool: PgPool) {
    let app = TestApp::new(pool);
    let huge = "9".repeat(ncrm_api::MAX_BODY_BYTES + 1);
    let r = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": huge })),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn one_ip_cannot_spray_code_requests_across_many_numbers(pool: PgPool) {
    let app = TestApp::with_security(
        pool,
        SecurityConfig {
            code_requests_per_window: 3,
            ..test_security()
        },
    );
    for i in 0..3 {
        let r = app
            .call(
                Method::POST,
                "/auth/code",
                Some(json!({ "phone": format!("+9198765000{i:02}") })),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::ACCEPTED);
    }
    let r = app
        .call(
            Method::POST,
            "/auth/code",
            Some(json!({ "phone": "+919876500099" })),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.error_code(), "too_many_requests");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn one_ip_cannot_spray_guesses_across_many_numbers(pool: PgPool) {
    let app = TestApp::with_security(
        pool,
        SecurityConfig {
            verify_requests_per_window: 3,
            ..test_security()
        },
    );
    for i in 0..3 {
        let r = app
            .call(
                Method::POST,
                "/auth/verify",
                Some(json!({ "phone": format!("+9198765000{i:02}"), "code": "123456" })),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    let r = app
        .call(
            Method::POST,
            "/auth/verify",
            Some(json!({ "phone": "+919876500099", "code": "123456" })),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_spoofed_ip_header_is_ignored_unless_our_proxy_sets_it(pool: PgPool) {
    // Not configured: the header is attacker-controlled, so it must not
    // give each request a fresh budget.
    let app = TestApp::with_security(
        pool.clone(),
        SecurityConfig {
            code_requests_per_window: 2,
            ..test_security()
        },
    );
    let mut statuses = Vec::new();
    for i in 0..3 {
        let fake_ip = format!("203.0.113.{i}");
        let r = app
            .call_with(
                Method::POST,
                "/auth/code",
                Some(json!({ "phone": format!("+9198765000{i:02}") })),
                None,
                Some(DASHBOARD),
                &[("x-real-ip", fake_ip.as_str())],
            )
            .await;
        statuses.push(r.status);
    }
    assert_eq!(statuses[2], StatusCode::TOO_MANY_REQUESTS, "{statuses:?}");

    // Configured (our proxy overwrites it): different clients get their own
    // budgets.
    let app = TestApp::with_security(
        pool,
        SecurityConfig {
            code_requests_per_window: 2,
            client_ip_header: Some(HeaderName::from_static("x-real-ip")),
            ..test_security()
        },
    );
    for i in 0..3 {
        let ip = format!("203.0.113.{i}");
        let r = app
            .call_with(
                Method::POST,
                "/auth/code",
                Some(json!({ "phone": format!("+9198765001{i:02}") })),
                None,
                Some(DASHBOARD),
                &[("x-real-ip", ip.as_str())],
            )
            .await;
        assert_eq!(r.status, StatusCode::ACCEPTED, "client {i}");
    }
}
