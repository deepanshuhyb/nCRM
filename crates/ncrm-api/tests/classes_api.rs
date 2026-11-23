//! Classes, plans, enrollments and credits through the HTTP API: each role,
//! bad input, and another studio.

mod common;

use axum::http::{Method, StatusCode};
use common::{branch, hire, studio, Reply, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use serde_json::{json, Value};
use sqlx::PgPool;

const OWNER: &str = "+919800000001";
const MANAGER: &str = "+919800000002";
const DESK: &str = "+919800000003";
const TEACHER: &str = "+919800000004";
const OTHER_TEACHER: &str = "+919800000005";
const ELSEWHERE: &str = "+919800000009";

struct Setup {
    app: TestApp,
    owner: String,
    manager: String,
    desk: String,
    teacher: String,
    other_teacher: String,
    elsewhere: String,
}

async fn setup(pool: &PgPool) -> Setup {
    let alpha = studio(pool, "alpha").await;
    hire(pool, &alpha, OWNER, StaffRole::Owner).await;
    hire(pool, &alpha, MANAGER, StaffRole::Manager).await;
    hire(pool, &alpha, DESK, StaffRole::FrontDesk).await;
    hire(pool, &alpha, TEACHER, StaffRole::Instructor).await;
    hire(pool, &alpha, OTHER_TEACHER, StaffRole::Instructor).await;
    branch(pool, &alpha, "North").await;
    let beta = studio(pool, "beta").await;
    hire(pool, &beta, ELSEWHERE, StaffRole::Owner).await;
    branch(pool, &beta, "Theirs").await;

    let app = TestApp::new(pool.clone());
    let owner = app.sign_in(OWNER).await.1;
    let manager = app.sign_in(MANAGER).await.1;
    let desk = app.sign_in(DESK).await.1;
    let teacher = app.sign_in(TEACHER).await.1;
    let other_teacher = app.sign_in(OTHER_TEACHER).await.1;
    let elsewhere = app.sign_in(ELSEWHERE).await.1;
    Setup {
        app,
        owner,
        manager,
        desk,
        teacher,
        other_teacher,
        elsewhere,
    }
}

impl Setup {
    async fn get(&self, path: &str, who: &str) -> Reply {
        self.app.call(Method::GET, path, None, Some(who)).await
    }
    async fn post(&self, path: &str, body: Value, who: &str) -> Reply {
        self.app
            .call(Method::POST, path, Some(body), Some(who))
            .await
    }
    async fn put(&self, path: &str, body: Value, who: &str) -> Reply {
        self.app
            .call(Method::PUT, path, Some(body), Some(who))
            .await
    }
    async fn id_of(&self, path: &str, who: &str, pick: &str) -> String {
        let r = self.get(path, who).await;
        r.json
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == pick)
            .unwrap_or_else(|| panic!("{pick} not in {path}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
    async fn staff_id(&self, phone: &str) -> String {
        let r = self.get("/staff", &self.owner).await;
        r.json
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["phone"] == phone)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_studio_sets_up_classes_and_front_desk_fills_them(pool: PgPool) {
    let s = setup(&pool).await;
    let north = s.id_of("/branches", &s.owner, "North").await;
    let teacher = s.staff_id(TEACHER).await;

    // Rooms: managers add them, front desk can't.
    let room = s
        .post(
            &format!("/branches/{north}/rooms"),
            json!({ "name": "Studio A", "capacity": 2 }),
            &s.manager,
        )
        .await;
    assert_eq!(room.status, StatusCode::CREATED, "{:?}", room.json);
    let room_id = room.json["id"].as_str().unwrap().to_string();
    let r = s
        .post(
            &format!("/branches/{north}/rooms"),
            json!({ "name": "Studio B" }),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // A class, with times as "HH:MM".
    let class = json!({
        "name": "Hip-hop Beginners",
        "style": "Hip-hop",
        "branch_id": north,
        "room_id": room_id,
        "instructor_id": teacher,
        "slots": [
            { "weekday": 1, "starts_at": "18:00", "ends_at": "19:00" },
            { "weekday": 3, "starts_at": "18:00", "ends_at": "19:00" }
        ]
    });
    let created = s.post("/batches", class.clone(), &s.manager).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    assert_eq!(created.json["capacity"], 2, "took the room's capacity");
    assert_eq!(created.json["slots"][1]["starts_at"], "18:00");
    let batch = created.json["id"].as_str().unwrap().to_string();

    // The same room at an overlapping time is a clear 409.
    let mut clash = class.clone();
    clash["name"] = json!("Salsa");
    clash["instructor_id"] = Value::Null;
    clash["slots"] = json!([{ "weekday": 3, "starts_at": "18:30", "ends_at": "20:00" }]);
    let r = s.post("/batches", clash, &s.owner).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(
        r.json["error"]["message"],
        "Studio A is booked for Hip-hop Beginners on Wednesday 18:00–19:00."
    );

    // Front desk can't create classes or set prices.
    assert_eq!(
        s.post("/batches", class.clone(), &s.desk).await.status,
        StatusCode::FORBIDDEN
    );
    let pricing = json!({ "name": "Monthly", "kind": "monthly", "price_paise": 250000 });
    assert_eq!(
        s.post("/plans", pricing.clone(), &s.desk).await.status,
        StatusCode::FORBIDDEN
    );
    let monthly = s.post("/plans", pricing, &s.owner).await;
    assert_eq!(monthly.status, StatusCode::CREATED, "{:?}", monthly.json);
    let monthly = monthly.json["id"].as_str().unwrap().to_string();
    let pack = s
        .post(
            "/plans",
            json!({ "name": "8 classes", "kind": "pack", "price_paise": 320000, "classes": 8, "valid_days": 45 }),
            &s.owner,
        )
        .await;
    assert_eq!(pack.status, StatusCode::CREATED, "{:?}", pack.json);
    let pack = pack.json["id"].as_str().unwrap().to_string();
    // Front desk reads prices to sell them.
    assert_eq!(
        s.get("/plans", &s.desk)
            .await
            .json
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Front desk enrolls two students; the third doesn't fit.
    let mut kids = Vec::new();
    for name in ["Aarav", "Bela", "Chirag"] {
        let r = s.post("/students", json!({ "name": name }), &s.desk).await;
        kids.push(r.json["id"].as_str().unwrap().to_string());
    }
    let join = |plan: Option<&str>| json!({ "batch_id": batch, "plan_id": plan });
    let r = s
        .post(
            &format!("/students/{}/enrollments", kids[0]),
            join(Some(&monthly)),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json[0]["batch_name"], "Hip-hop Beginners");
    assert_eq!(r.json[0]["plan_price_paise"], 250000);
    let r = s
        .post(
            &format!("/students/{}/enrollments", kids[1]),
            join(None),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let r = s
        .post(
            &format!("/students/{}/enrollments", kids[2]),
            join(None),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(
        r.json["error"]["message"],
        "Hip-hop Beginners is full (2 of 2)."
    );

    // Packs: front desk sells, only managers give or take back.
    let r = s
        .post(
            &format!("/students/{}/credits/sell", kids[1]),
            json!({ "plan_id": pack }),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json["available"], 8);
    let gift = json!({ "classes": 1, "note": "Missed a class" });
    let r = s
        .post(
            &format!("/students/{}/credits/give", kids[1]),
            gift.clone(),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = s
        .post(
            &format!("/students/{}/credits/give", kids[1]),
            gift,
            &s.manager,
        )
        .await;
    assert_eq!(r.json["available"], 9);
    let r = s
        .post(
            &format!("/students/{}/credits/take-back", kids[1]),
            json!({ "classes": 20, "note": "Refund" }),
            &s.manager,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["error"]["message"], "They have 9 usable classes.");

    // The class's own teacher sees who's in it; another teacher sees the
    // count only; nobody else's students are exposed.
    let own = s.get(&format!("/batches/{batch}"), &s.teacher).await;
    assert_eq!(own.status, StatusCode::OK);
    assert_eq!(own.json["roster"].as_array().unwrap().len(), 2);
    let other = s.get(&format!("/batches/{batch}"), &s.other_teacher).await;
    assert_eq!(other.json["roster"], Value::Null);
    assert_eq!(other.json["enrolled"], 2);
    // Teachers see the timetable but not students' classes or credits.
    assert_eq!(
        s.get("/batches", &s.teacher)
            .await
            .json
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let r = s
        .get(&format!("/students/{}/credits", kids[1]), &s.teacher)
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Leaving a class frees the place.
    let theirs = s
        .get(&format!("/students/{}/enrollments", kids[1]), &s.desk)
        .await;
    let enrollment = theirs.json[0]["id"].as_str().unwrap().to_string();
    let r = s
        .post(
            &format!("/enrollments/{enrollment}/end"),
            json!({}),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert!(r.json[0]["ended_on"].is_string());
    let r = s
        .post(
            &format!("/students/{}/enrollments", kids[2]),
            join(None),
            &s.desk,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);

    // The home counts include classes and plans.
    let summary = s.get("/summary", &s.teacher).await;
    assert_eq!(summary.json["classes"], 1);
    assert_eq!(summary.json["plans"], 2);
    // The studio's date, as YYYY-MM-DD.
    assert_eq!(summary.json["today"].as_str().unwrap().len(), 10);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn bad_class_input_gets_plain_answers(pool: PgPool) {
    let s = setup(&pool).await;
    let north = s.id_of("/branches", &s.owner, "North").await;
    let base = json!({
        "name": "Salsa",
        "branch_id": north,
        "slots": [{ "weekday": 2, "starts_at": "19:00", "ends_at": "20:00" }]
    });
    let cases = [
        ("name", json!(""), "Class names must be 1 to 80 characters."),
        (
            "slots",
            json!([]),
            "Add at least one day and time the class meets.",
        ),
        (
            "slots",
            json!([{ "weekday": 2, "starts_at": "20:00", "ends_at": "19:00" }]),
            "On Tuesday, the class has to end after it starts.",
        ),
        (
            "capacity",
            json!(0),
            "Class capacity must be between 1 and 500.",
        ),
        (
            "branch_id",
            json!("00000000-0000-4000-8000-000000000000"),
            "Choose one of your branches.",
        ),
    ];
    for (field, value, message) in cases {
        let mut body = base.clone();
        body[field] = value;
        let r = s.post("/batches", body, &s.owner).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{field}: {:?}", r.json);
        assert_eq!(r.json["error"]["message"], message, "{field}");
    }
    // A time that isn't a time is refused by the parser.
    let mut body = base.clone();
    body["slots"] = json!([{ "weekday": 2, "starts_at": "7pm", "ends_at": "20:00" }]);
    let r = s.post("/batches", body, &s.owner).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error_code(), "invalid_input");
    assert_eq!(
        r.json["error"]["message"],
        "slots[0].starts_at isn't in the right format."
    );
    let r = s
        .post("/batches", json!({ "branch_id": north }), &s.owner)
        .await;
    assert_eq!(r.json["error"]["message"], "Something's missing: name.");

    let r = s
        .post(
            "/plans",
            json!({ "name": "Pack", "kind": "pack", "price_paise": 100 }),
            &s.owner,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        r.json["error"]["message"],
        "Say how many classes the pack has."
    );
    let r = s
        .post(
            "/plans",
            json!({ "name": "Free", "kind": "monthly", "price_paise": -1 }),
            &s.owner,
        )
        .await;
    assert_eq!(
        r.json["error"]["message"],
        "Prices must be between ₹0 and ₹10,00,000."
    );
    let r = s.get("/batches/not-an-id", &s.owner).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn another_studio_sees_none_of_it(pool: PgPool) {
    let s = setup(&pool).await;
    let north = s.id_of("/branches", &s.owner, "North").await;
    let batch = s
        .post(
            "/batches",
            json!({
                "name": "Hip-hop",
                "branch_id": north,
                "slots": [{ "weekday": 1, "starts_at": "18:00", "ends_at": "19:00" }]
            }),
            &s.owner,
        )
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plan = s
        .post(
            "/plans",
            json!({ "name": "Drop-in", "kind": "drop_in", "price_paise": 50000, "valid_days": 7 }),
            &s.owner,
        )
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let kid = s
        .post("/students", json!({ "name": "Aarav" }), &s.desk)
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();

    let e = &s.elsewhere;
    assert_eq!(s.get("/batches", e).await.json, json!([]));
    assert_eq!(s.get("/plans", e).await.json, json!([]));
    assert_eq!(s.get("/rooms", e).await.json, json!([]));
    for path in [
        format!("/batches/{batch}"),
        format!("/students/{kid}/credits"),
        format!("/students/{kid}/enrollments"),
    ] {
        let r = s.get(&path, e).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}: {:?}", r.json);
    }
    let r = s
        .put(
            &format!("/plans/{plan}"),
            json!({ "name": "Mine", "price_paise": 1, "active": true }),
            e,
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = s
        .post(
            &format!("/branches/{north}/rooms"),
            json!({ "name": "Planted" }),
            e,
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Their own student, our class: refused as not theirs.
    let theirs = s.post("/students", json!({ "name": "Zoya" }), e).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = s
        .post(
            &format!("/students/{theirs}/enrollments"),
            json!({ "batch_id": batch }),
            e,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["error"]["message"], "Choose one of your classes.");
    let r = s
        .post(
            &format!("/students/{theirs}/credits/sell"),
            json!({ "plan_id": plan }),
            e,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}
