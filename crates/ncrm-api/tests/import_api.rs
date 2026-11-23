//! Importing a student list through the HTTP API: the two steps, file
//! types, size, roles, and the origin check.

mod common;

use axum::http::{Method, StatusCode};
use common::{branch, hire, studio, TestApp};
use ncrm_core::db::MIGRATOR;
use ncrm_core::staff::StaffRole;
use rust_xlsxwriter::Workbook;
use sqlx::PgPool;

const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const DESK: &str = "+919800000003";
const TEACHER: &str = "+919800000004";

fn sheet() -> Vec<u8> {
    let mut book = Workbook::new();
    let ws = book.add_worksheet();
    for (c, t) in ["Name", "Phone", "Relation", "Joined on"]
        .iter()
        .enumerate()
    {
        ws.write_string(0, c as u16, *t).unwrap();
    }
    ws.write_string(1, 0, "Aarav Sharma").unwrap();
    ws.write_number(1, 1, 9876543210.0).unwrap();
    ws.write_string(1, 2, "Mother").unwrap();
    ws.write_string(1, 3, "03/04/2024").unwrap();
    ws.write_string(2, 0, "Bela Gupta").unwrap();
    ws.write_number(2, 1, 9876543211.0).unwrap();
    book.save_to_buffer().unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn front_desk_previews_then_imports(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, DESK, StaffRole::FrontDesk).await;
    hire(&pool, &alpha, TEACHER, StaffRole::Instructor).await;
    branch(&pool, &alpha, "North").await;
    let app = TestApp::new(pool.clone());
    let (_, desk) = app.sign_in(DESK).await;
    let (_, teacher) = app.sign_in(TEACHER).await;

    let preview = app
        .call_raw("/students/import/preview", sheet(), XLSX, Some(&desk))
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    assert_eq!(preview.json["ready"], 2);
    assert_eq!(preview.json["rows"][0]["joined_on"], "2024-04-03");
    assert_eq!(preview.json["rows"][0]["contacts"][0]["relation"], "mother");
    let list = app.call(Method::GET, "/students", None, Some(&desk)).await;
    assert_eq!(list.json["total"], 0, "a preview writes nothing");

    let done = app
        .call_raw("/students/import", sheet(), XLSX, Some(&desk))
        .await;
    assert_eq!(done.status, StatusCode::CREATED, "{:?}", done.json);
    assert_eq!(done.json["added"], 2);
    let again = app
        .call_raw("/students/import", sheet(), XLSX, Some(&desk))
        .await;
    assert_eq!(again.json["added"], 0);
    assert_eq!(again.json["skipped"], 2);

    // CSV works too, with the type Windows browsers give it.
    let csv = b"Name,Phone\nChirag Jain,9876543212\n".to_vec();
    let r = app
        .call_raw(
            "/students/import",
            csv,
            "application/vnd.ms-excel",
            Some(&desk),
        )
        .await;
    assert_eq!(r.json["added"], 1, "{:?}", r.json);

    // Instructors can't.
    let r = app
        .call_raw("/students/import/preview", sheet(), XLSX, Some(&teacher))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_wrong_kind_of_upload_is_refused(pool: PgPool) {
    let alpha = studio(&pool, "alpha").await;
    hire(&pool, &alpha, DESK, StaffRole::FrontDesk).await;
    let app = TestApp::new(pool.clone());
    let (_, desk) = app.sign_in(DESK).await;

    // The types a cross-site HTML form can send are refused outright.
    for form_type in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data; boundary=x",
    ] {
        let r = app
            .call_raw(
                "/students/import",
                b"Name\nX\n".to_vec(),
                form_type,
                Some(&desk),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{form_type}");
        assert_eq!(r.json["error"]["message"], "Upload a .xlsx or .csv file.");
    }

    // Over 2 MB: 413, before anything is read.
    let big = vec![b'a'; 3 * 1024 * 1024];
    let r = app
        .call_raw("/students/import", big, "text/csv", Some(&desk))
        .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);

    // Just under the cap gets through the body limit (and is then judged on
    // its contents): proof the route really allows more than 64 KB.
    let mut csv = String::from("Name,Phone\n");
    while csv.len() < 200 * 1024 {
        csv.push_str("Somebody With A Fairly Long Name,\n");
    }
    let r = app
        .call_raw(
            "/students/import/preview",
            csv.into_bytes(),
            "text/csv",
            Some(&desk),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(r.json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("more than 2,000"));

    // JSON routes keep their 64 KB limit.
    let r = app
        .call(
            Method::POST,
            "/students",
            Some(serde_json::json!({ "name": "x", "notes": "n".repeat(70 * 1024) })),
            Some(&desk),
        )
        .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);

    // A page that isn't the dashboard can't upload.
    let r = app
        .call_with(
            Method::POST,
            "/students/import",
            None,
            Some(&desk),
            Some("https://evil.example"),
            &[("content-type", "text/csv")],
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}
