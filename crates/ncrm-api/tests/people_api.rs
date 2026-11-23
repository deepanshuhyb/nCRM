//! Team and students through the HTTP API: roles, bad input, other studios.

mod common;

use axum::http::{Method, StatusCode};
use common::{hire, studio, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::{json, Value};
use sqlx::PgPool;

const OWNER: &str = "+919800000001";
const MANAGER: &str = "+919800000002";
const DESK: &str = "+919800000003";
const TEACHER: &str = "+919800000004";

async fn setup(pool: &PgPool) -> TestApp {
    let alpha = studio(pool, "alpha").await;
    hire(pool, &alpha, OWNER, StaffRole::Owner).await;
    hire(pool, &alpha, MANAGER, StaffRole::Manager).await;
    hire(pool, &alpha, DESK, StaffRole::FrontDesk).await;
    hire(pool, &alpha, TEACHER, StaffRole::Instructor).await;
    TestApp::new(pool.clone())
}

fn new_student() -> Value {
    json!({
        "name": "Aarav Sharma",
        "date_of_birth": "2015-03-04",
        "notes": "Shy at first",
        "contacts": [
            { "name": "Priya Sharma", "phone": "98111 22233", "relation": "mother" },
            { "name": "Rohit Sharma", "phone": "98111 22234", "relation": "father" }
        ]
    })
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn front_desk_runs_students_end_to_end(pool: PgPool) {
    let app = setup(&pool).await;
    let (_, desk) = app.sign_in(DESK).await;

    let created = app
        .call(Method::POST, "/students", Some(new_student()), Some(&desk))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    assert_eq!(created.json["date_of_birth"], "2015-03-04");
    assert_eq!(created.json["status"], "active");
    let contacts = created.json["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 2);
    assert_eq!(contacts[0]["phone"], "+919811122233");
    assert_eq!(contacts[0]["is_primary"], true);
    let id = created.json["id"].as_str().unwrap().to_string();

    let list = app
        .call(Method::GET, "/students?q=aarav", None, Some(&desk))
        .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.json["total"], 1);
    assert_eq!(
        list.json["items"][0]["primary_contact_name"],
        "Priya Sharma"
    );

    let edited = app
        .call(
            Method::PUT,
            &format!("/students/{id}"),
            Some(json!({ "name": "Aarav S.", "date_of_birth": null, "branch_id": null, "status": "paused", "notes": null })),
            Some(&desk),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
    assert_eq!(edited.json["status"], "paused");
    assert!(edited.json["date_of_birth"].is_null());

    let father = contacts[1]["contact_id"].as_str().unwrap();
    let removed = app
        .call(
            Method::DELETE,
            &format!("/students/{id}/contacts/{father}"),
            None,
            Some(&desk),
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK);
    assert_eq!(removed.json["contacts"].as_array().unwrap().len(), 1);

    let added = app
        .call(
            Method::POST,
            &format!("/students/{id}/contacts"),
            Some(json!({ "name": "Nani", "phone": "98111 22235", "relation": "guardian", "is_primary": true })),
            Some(&desk),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED);
    assert_eq!(added.json["contacts"][0]["name"], "Nani");

    // Front desk can't see or change the team.
    let team = app.call(Method::GET, "/staff", None, Some(&desk)).await;
    assert_eq!(team.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn instructors_see_the_summary_but_not_student_records(pool: PgPool) {
    let app = setup(&pool).await;
    let (_, desk) = app.sign_in(DESK).await;
    app.call(Method::POST, "/students", Some(new_student()), Some(&desk))
        .await;

    let (_, teacher) = app.sign_in(TEACHER).await;
    for (method, path, body) in [
        (Method::GET, "/students", None),
        (Method::POST, "/students", Some(new_student())),
    ] {
        let r = app.call(method, path, body, Some(&teacher)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}");
    }
    let summary = app
        .call(Method::GET, "/summary", None, Some(&teacher))
        .await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(summary.json["active_students"], 1);
    assert_eq!(summary.json["active_team"], 4);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn managers_build_the_team_but_cannot_make_owners(pool: PgPool) {
    let app = setup(&pool).await;
    let (me, manager) = app.sign_in(MANAGER).await;
    let my_id = me.json["studio"]["staff_id"].as_str().unwrap().to_string();

    let owner = app
        .call(
            Method::POST,
            "/staff",
            Some(json!({ "name": "Sneaky", "phone": "9800000099", "role": "owner" })),
            Some(&manager),
        )
        .await;
    assert_eq!(owner.status, StatusCode::FORBIDDEN);
    assert_eq!(owner.error_code(), "not_allowed");

    let teacher = app
        .call(
            Method::POST,
            "/staff",
            Some(json!({ "name": "Ravi", "phone": "9800000098", "role": "instructor" })),
            Some(&manager),
        )
        .await;
    assert_eq!(teacher.status, StatusCode::CREATED, "{:?}", teacher.json);
    assert_eq!(teacher.json["phone"], "+919800000098");

    let dup = app
        .call(
            Method::POST,
            "/staff",
            Some(json!({ "name": "Ravi again", "phone": "+91 98000 00098", "role": "instructor" })),
            Some(&manager),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let promote_self = app
        .call(
            Method::PUT,
            &format!("/staff/{my_id}"),
            Some(json!({ "name": "Manager", "phone": MANAGER, "role": "owner", "branch_id": null, "active": true })),
            Some(&manager),
        )
        .await;
    assert_eq!(promote_self.status, StatusCode::FORBIDDEN);

    let list = app.call(Method::GET, "/staff", None, Some(&manager)).await;
    assert_eq!(list.json.as_array().unwrap().len(), 5);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn bad_input_gets_clear_errors(pool: PgPool) {
    let app = setup(&pool).await;
    let (_, desk) = app.sign_in(DESK).await;

    let cases = [
        json!({ "name": "", "contacts": [] }),
        json!({ "name": "X", "date_of_birth": "2999-01-01" }),
        json!({ "name": "X", "date_of_birth": "not a date" }),
        json!({ "name": "X", "contacts": [{ "name": "P", "phone": "12", "relation": "mother" }] }),
        json!({ "name": "X", "contacts": [{ "name": "P", "phone": "9811122233", "relation": "neighbour" }] }),
        json!({ "name": "X".repeat(121) }),
    ];
    for body in cases {
        let r = app
            .call(Method::POST, "/students", Some(body.clone()), Some(&desk))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(r.error_code(), "invalid_input", "{body}");
    }

    for path in [
        "/students?status=gone",
        "/students?limit=lots",
        "/students?branch_id=nope",
    ] {
        let r = app.call(Method::GET, path, None, Some(&desk)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{path}");
    }

    for path in [
        "/students/not-a-uuid",
        "/students/00000000-0000-0000-0000-000000000000",
    ] {
        let r = app.call(Method::GET, path, None, Some(&desk)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_studios_students_are_invisible(pool: PgPool) {
    let app = setup(&pool).await;
    let beta = studio(&pool, "beta").await;
    let beta_desk = "+919800000020";
    hire(&pool, &beta, beta_desk, StaffRole::FrontDesk).await;

    let (_, desk) = app.sign_in(DESK).await;
    let created = app
        .call(Method::POST, "/students", Some(new_student()), Some(&desk))
        .await;
    let id = created.json["id"].as_str().unwrap().to_string();
    let contact = created.json["contacts"][0]["contact_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (_, other) = app.sign_in(beta_desk).await;
    let get = app
        .call(Method::GET, &format!("/students/{id}"), None, Some(&other))
        .await;
    assert_eq!(get.status, StatusCode::NOT_FOUND);
    let put = app
        .call(
            Method::PUT,
            &format!("/students/{id}"),
            Some(json!({ "name": "Taken", "date_of_birth": null, "branch_id": null, "status": "left", "notes": null })),
            Some(&other),
        )
        .await;
    assert_eq!(put.status, StatusCode::NOT_FOUND);
    let del = app
        .call(
            Method::DELETE,
            &format!("/students/{id}/contacts/{contact}"),
            None,
            Some(&other),
        )
        .await;
    assert_eq!(del.status, StatusCode::NOT_FOUND);
    let list = app.call(Method::GET, "/students", None, Some(&other)).await;
    assert_eq!(list.json["total"], 0);

    // And the original is untouched.
    let mine = app
        .call(Method::GET, &format!("/students/{id}"), None, Some(&desk))
        .await;
    assert_eq!(mine.json["name"], "Aarav Sharma");
    assert_eq!(mine.json["contacts"].as_array().unwrap().len(), 2);
}
