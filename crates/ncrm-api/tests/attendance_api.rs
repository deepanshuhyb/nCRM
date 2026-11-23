//! Attendance through the HTTP API: the day, marking, roles, another studio.

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
const OTHER_TEACHER: &str = "+919800000005";
const ELSEWHERE: &str = "+919800000009";

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_day_of_attendance(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    for (phone, role) in [
        (OWNER, StaffRole::Owner),
        (DESK, StaffRole::FrontDesk),
        (TEACHER, StaffRole::Instructor),
        (OTHER_TEACHER, StaffRole::Instructor),
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
    let other_teacher = app.sign_in(OTHER_TEACHER).await.1;
    let elsewhere = app.sign_in(ELSEWHERE).await.1;
    let get = |path: String, who: String| {
        let app = &app;
        async move { app.call(Method::GET, &path, None, Some(&who)).await }
    };
    let send = |method: Method, path: String, body: Value, who: String| {
        let app = &app;
        async move { app.call(method, &path, Some(body), Some(&who)).await }
    };

    let north = get("/branches".into(), owner.clone()).await.json[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let staff = get("/staff".into(), owner.clone()).await.json;
    let teacher_id = staff
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["phone"] == TEACHER)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // A class every day just after midnight, so today's is already over.
    let slots: Vec<Value> = (1..=7)
        .map(|d| json!({ "weekday": d, "starts_at": "00:00", "ends_at": "00:01" }))
        .collect();
    let class = send(
        Method::POST,
        "/batches".into(),
        json!({ "name": "Hip-hop", "branch_id": north, "instructor_id": teacher_id, "slots": slots }),
        owner.clone(),
    )
    .await;
    assert_eq!(class.status, StatusCode::CREATED, "{:?}", class.json);
    let batch = class.json["id"].as_str().unwrap().to_string();
    let mut kids = Vec::new();
    for name in ["Aarav", "Bela"] {
        let id = send(
            Method::POST,
            "/students".into(),
            json!({ "name": name }),
            desk.clone(),
        )
        .await
        .json["id"]
            .as_str()
            .unwrap()
            .to_string();
        send(
            Method::POST,
            format!("/students/{id}/enrollments"),
            json!({ "batch_id": batch }),
            desk.clone(),
        )
        .await;
        kids.push(id);
    }

    // The day: one class, two expected, nobody marked, already over.
    let day = get("/sessions".into(), teacher.clone()).await;
    assert_eq!(day.status, StatusCode::OK, "{:?}", day.json);
    let s = &day.json[0];
    assert_eq!(
        (
            s["expected"].as_i64(),
            s["unmarked"].as_i64(),
            s["ended"].as_bool()
        ),
        (Some(2), Some(2), Some(true))
    );
    assert_eq!(s["starts_at"], "00:00");
    let id = s["id"].as_str().unwrap().to_string();
    let summary = get("/summary".into(), desk.clone()).await.json;
    assert_eq!(
        (
            summary["classes_today"].as_i64(),
            summary["unmarked"].as_i64()
        ),
        (Some(1), Some(1))
    );
    assert_eq!(
        get("/sessions?date=2099-01-01".into(), desk.clone())
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // The teacher marks; both pay with credits and have none, so they owe.
    let marks = json!({ "marks": [
        { "student_id": kids[0], "status": "present" },
        { "student_id": kids[1], "status": "absent" }
    ]});
    let other = send(
        Method::PUT,
        format!("/sessions/{id}/attendance"),
        marks.clone(),
        other_teacher.clone(),
    )
    .await;
    assert_eq!(other.status, StatusCode::FORBIDDEN);
    assert_eq!(
        get(format!("/sessions/{id}"), other_teacher.clone())
            .await
            .json["roster"],
        Value::Null
    );
    let marked = send(
        Method::PUT,
        format!("/sessions/{id}/attendance"),
        marks,
        teacher.clone(),
    )
    .await;
    assert_eq!(marked.status, StatusCode::OK, "{:?}", marked.json);
    assert_eq!(
        (
            marked.json["came"].as_i64(),
            marked.json["away"].as_i64(),
            marked.json["unmarked"].as_i64()
        ),
        (Some(1), Some(1), Some(0))
    );
    assert_eq!(marked.json["roster"][0]["owes_credit"], true);
    let bad = send(
        Method::PUT,
        format!("/sessions/{id}/attendance"),
        json!({ "marks": [{ "student_id": kids[0], "status": "vanished" }] }),
        teacher.clone(),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Teacher checks in; only the front desk cancels.
    let checked = send(
        Method::PUT,
        format!("/sessions/{id}/teacher"),
        json!({ "present": true }),
        teacher.clone(),
    )
    .await;
    assert_eq!(checked.json["teacher_present"], true);
    let r = send(
        Method::POST,
        format!("/sessions/{id}/cancel"),
        json!({ "reason": "Rain" }),
        teacher.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = send(
        Method::POST,
        format!("/sessions/{id}/cancel"),
        json!({ "reason": "Rain" }),
        desk.clone(),
    )
    .await;
    assert_eq!(r.json["status"], "cancelled");
    let r = send(
        Method::POST,
        format!("/sessions/{id}/restore"),
        json!({}),
        desk.clone(),
    )
    .await;
    assert_eq!(r.json["status"], "scheduled");

    // Alerts and a student's history.
    let alerts = get("/attendance/alerts".into(), desk.clone()).await;
    assert_eq!(alerts.status, StatusCode::OK);
    assert!(
        alerts.json["unmarked"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == id.as_str()),
        "restored and unmarked again"
    );
    assert_eq!(
        get(format!("/students/{}/attendance", kids[0]), teacher.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(format!("/students/{}/attendance", kids[0]), desk.clone())
            .await
            .status,
        StatusCode::OK
    );

    // Another studio.
    assert_eq!(
        get("/sessions".into(), elsewhere.clone()).await.json,
        json!([])
    );
    assert_eq!(
        get(format!("/sessions/{id}"), elsewhere.clone())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = send(
        Method::PUT,
        format!("/sessions/{id}/attendance"),
        json!({ "marks": [] }),
        elsewhere.clone(),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
