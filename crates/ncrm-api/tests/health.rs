use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ncrm_api::{router, AppState};
use ncrm_core::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test(migrator = "MIGRATOR")]
async fn healthz_reports_ok_when_the_database_answers(pool: PgPool) {
    let app = router(AppState { pool });
    let response = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn healthz_reports_unavailable_when_the_database_is_gone(pool: PgPool) {
    let app = router(AppState { pool: pool.clone() });
    pool.close().await;
    let response = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
