//! Shared helpers for API tests: an app wired to a capture sender, a tiny
//! HTTP client over the router, and seed data.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ncrm_api::security::SecurityConfig;
use ncrm_api::{router, AppState};
use ncrm_core::auth::{AuthConfig, CodeSender, PendingCode};
use ncrm_core::branches::{self, NewBranch};
use ncrm_core::db::TenantTx;
use ncrm_core::orgs::{self, NewOrganization, Organization};
use ncrm_core::staff::{self, NewStaff, StaffRole};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

pub struct TestApp {
    pub pool: PgPool,
    pub router: Router,
    pub codes: Arc<Mutex<Vec<PendingCode>>>,
}

pub struct Reply {
    pub status: StatusCode,
    pub json: Value,
    pub set_cookie: Option<String>,
    pub headers: axum::http::HeaderMap,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn error_code(&self) -> &str {
        self.json["error"]["code"].as_str().unwrap_or("")
    }
}

/// The dashboard origin the test API trusts.
pub const DASHBOARD: &str = "https://app.ncrm.test";

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self::with_security(pool, test_security())
    }

    pub fn with_security(pool: PgPool, security: SecurityConfig) -> Self {
        let (sender, codes) = CodeSender::capture();
        let auth = AuthConfig::new(vec![9u8; 32]).unwrap();
        let state = AppState::with_security(pool.clone(), auth, sender, security);
        TestApp {
            pool,
            router: router(state),
            codes,
        }
    }

    /// A request as our dashboard would send it: same-site, with our Origin
    /// on anything that changes state.
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> Reply {
        let origin = (method != Method::GET).then_some(DASHBOARD);
        self.call_with(method, path, body, cookie, origin, &[])
            .await
    }

    /// A request with full control over Origin and extra headers.
    pub async fn call_with(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
        origin: Option<&str>,
        extra_headers: &[(&str, &str)],
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        if let Some(o) = origin {
            req = req.header(header::ORIGIN, o);
        }
        for (name, value) in extra_headers {
            req = req.header(*name, *value);
        }
        let req = match body {
            Some(json) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let set_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().to_string());
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Reply {
            status,
            json,
            set_cookie,
            headers,
        }
    }

    /// Wait for the background sender to deliver a code to `phone`.
    pub async fn code_for(&self, phone_as_typed: &str) -> String {
        let phone = ncrm_core::phone::normalize(phone_as_typed).unwrap();
        let phone = phone.as_str();
        for _ in 0..100 {
            if let Some(p) = self
                .codes
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|p| p.phone == phone)
            {
                return p.code.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no code was delivered to {phone}");
    }

    /// Ask for a code, type it in, and return the `Cookie` header value.
    pub async fn sign_in(&self, phone: &str) -> (Reply, String) {
        self.codes.lock().unwrap().clear();
        let r = self
            .call(
                Method::POST,
                "/auth/code",
                Some(serde_json::json!({ "phone": phone })),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::ACCEPTED);
        let code = self.code_for(phone).await;
        let r = self
            .call(
                Method::POST,
                "/auth/verify",
                Some(serde_json::json!({ "phone": phone, "code": code })),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
        let cookie = cookie_pair(r.set_cookie.as_deref().expect("session cookie set"));
        (r, cookie)
    }
}

/// Security settings for tests: our dashboard is trusted, and limits are
/// high enough not to get in the way of ordinary tests.
pub fn test_security() -> SecurityConfig {
    SecurityConfig {
        allowed_origins: vec![DASHBOARD.to_string()],
        code_requests_per_window: 1000,
        verify_requests_per_window: 1000,
        ..SecurityConfig::default()
    }
}

/// "__Host-ncrm_session=abc; HttpOnly; ..." → "__Host-ncrm_session=abc"
pub fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().trim().to_string()
}

pub async fn studio(pool: &PgPool, slug: &str) -> Organization {
    orgs::create(
        pool,
        NewOrganization {
            name: format!("Studio {slug}"),
            slug: slug.into(),
            timezone: None,
        },
    )
    .await
    .unwrap()
}

pub async fn hire(pool: &PgPool, org: &Organization, phone: &str, role: StaffRole) {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    staff::create(
        &mut tx,
        NewStaff {
            name: format!("{role:?}"),
            phone: phone.into(),
            role,
            branch_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

pub async fn branch(pool: &PgPool, org: &Organization, name: &str) {
    let mut tx = TenantTx::begin(pool, org.id).await.unwrap();
    branches::create(
        &mut tx,
        NewBranch {
            name: name.into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
