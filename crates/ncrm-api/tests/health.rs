mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use ncrm_core::db::MIGRATOR;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR")]
async fn healthz_reports_ok_when_the_database_answers(pool: PgPool) {
    let app = TestApp::new(pool);
    let r = app.call(Method::GET, "/healthz", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["status"], "ok");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn healthz_reports_unavailable_when_the_database_is_gone(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    pool.close().await;
    let r = app.call(Method::GET, "/healthz", None, None).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
}
