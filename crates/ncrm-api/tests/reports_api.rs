//! The report endpoints over HTTP: who may call them, bad input, and ids
//! from another studio.

mod common;

use axum::http::{Method, StatusCode};
use common::{branch, hire, studio, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::{json, Value};
use sqlx::PgPool;

const OWNER: &str = "+919800000001";
const DESK: &str = "+919800000003";
const TEACHER: &str = "+919800000004";
const ELSEWHERE: &str = "+919800000009";

#[sqlx::test(migrator = "MIGRATOR")]
async fn reports_follow_the_role(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    for (phone, role) in [
        (OWNER, StaffRole::Owner),
        (DESK, StaffRole::FrontDesk),
        (TEACHER, StaffRole::Instructor),
    ] {
        hire(&pool, &alpha, phone, role).await;
    }
    branch(&pool, &alpha, "North").await;
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

    // Not signed in: nothing.
    let anon = app
        .call(Method::GET, "/reports/attendance", None, None)
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    // Attendance and students: everyone on the team, counts only.
    for who in [&owner, &desk, &teacher] {
        let a = get("/reports/attendance".into(), who.clone()).await;
        assert_eq!(a.status, StatusCode::OK, "{:?}", a.json);
        assert_eq!(a.json.as_array().unwrap().len(), 28);
        assert_eq!(a.json[27]["came"], 0);
        let s = get("/reports/students".into(), who.clone()).await;
        assert_eq!(s.status, StatusCode::OK);
        assert_eq!(s.json.as_array().unwrap().len(), 6);
        assert!(s.json[5]["month"].as_str().unwrap().ends_with("-01"));
    }
    for bad in ["days=3", "days=500", "days=lots"] {
        let r = get(format!("/reports/attendance?{bad}"), owner.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(
        get("/reports/attendance?days=90".into(), owner.clone())
            .await
            .json
            .as_array()
            .unwrap()
            .len(),
        90
    );

    // Fees: owed for the front desk, collected only for owners; teachers no.
    let for_owner = get("/reports/fees".into(), owner.clone()).await;
    assert_eq!(for_owner.status, StatusCode::OK, "{:?}", for_owner.json);
    assert_eq!(for_owner.json["ageing"].as_array().unwrap().len(), 4);
    assert_eq!(for_owner.json["collection"]["collected_paise"], 0);
    let for_desk = get("/reports/fees".into(), desk.clone()).await;
    assert_eq!(for_desk.status, StatusCode::OK);
    assert_eq!(for_desk.json["collection"], Value::Null);
    assert_eq!(
        get("/reports/fees".into(), teacher.clone()).await.status,
        StatusCode::FORBIDDEN
    );

    // A class's history: this studio's classes only.
    let north = get("/branches".into(), owner.clone()).await.json[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let class = app
        .call(
            Method::POST,
            "/batches",
            Some(json!({ "name": "Hip-hop", "branch_id": north,
                         "slots": [{ "weekday": 1, "starts_at": "18:00", "ends_at": "19:00" }] })),
            Some(&owner),
        )
        .await;
    assert_eq!(class.status, StatusCode::CREATED, "{:?}", class.json);
    let id = class.json["id"].as_str().unwrap().to_string();
    let mine = get(format!("/batches/{id}/history"), teacher.clone()).await;
    assert_eq!(mine.status, StatusCode::OK);
    assert_eq!(mine.json, json!([]));
    for (path, who) in [
        (format!("/batches/{id}/history"), elsewhere.clone()),
        ("/batches/not-an-id/history".to_string(), owner.clone()),
    ] {
        assert_eq!(get(path, who).await.status, StatusCode::NOT_FOUND);
    }
}
