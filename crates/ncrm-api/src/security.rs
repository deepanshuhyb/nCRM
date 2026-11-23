//! Protections that sit in front of every handler.
//!
//! - `origin_guard`: state-changing requests must come from our own dashboard.
//! - `RateLimiter` + `limit_*`: per-IP limits on the sign-in endpoints.
//! - `client_ip`: which address a request comes from, without trusting
//!   headers anyone could fake.
//!
//! Response headers (no-store, nosniff, …), the body size limit and the
//! timeout are layers set up in `lib.rs`.
//!
//! Background and trade-offs: docs/NOTES.md ("Browser-facing protections").

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderName, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::AppState;

/// Settings for the protections in this module.
#[derive(Clone, Debug)]
pub struct SecurityConfig {
    /// Exact origins (scheme + host + port) allowed to make state-changing
    /// requests, e.g. `https://app.example.com`. Empty means no browser page
    /// may; non-browser clients that send no Origin are still allowed.
    pub allowed_origins: Vec<String>,
    /// Header holding the real client address, set by our own reverse proxy
    /// (e.g. `x-real-ip`). `None` means use the TCP peer address. Only set
    /// this when every request passes through a proxy that overwrites it,
    /// or anyone can pick their own rate-limit bucket.
    pub client_ip_header: Option<HeaderName>,
    /// Requests per IP per window for `POST /auth/code`.
    pub code_requests_per_window: u32,
    /// Requests per IP per window for `POST /auth/verify`.
    pub verify_requests_per_window: u32,
    pub window: Duration,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            client_ip_header: None,
            code_requests_per_window: 10,
            verify_requests_per_window: 30,
            window: Duration::from_secs(10 * 60),
        }
    }
}

/// Refuse POST/PUT/PATCH/DELETE from pages that aren't ours.
///
/// Browsers attach an `Origin` header to cross-site requests and to
/// same-site non-GET fetches. If it is present it must be on the allow
/// list. If it is absent, `Sec-Fetch-Site: cross-site` is refused too.
/// Requests with neither (curl, server-to-server calls) pass; they can't
/// carry a victim's cookies anyway.
pub async fn origin_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    let headers = req.headers();
    if let Some(origin) = headers.get(header::ORIGIN) {
        let allowed = origin.to_str().is_ok_and(|o| {
            state
                .security
                .allowed_origins
                .iter()
                .any(|a| a.eq_ignore_ascii_case(o))
        });
        if !allowed {
            return ApiError::BadOrigin.into_response();
        }
    } else if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("cross-site"))
    {
        return ApiError::BadOrigin.into_response();
    }
    next.run(req).await
}

/// The address a request comes from.
pub fn client_ip(state: &AppState, req: &Request) -> IpAddr {
    if let Some(name) = &state.security.client_ip_header {
        let from_header = req
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse::<IpAddr>().ok());
        if let Some(ip) = from_header {
            return ip;
        }
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

/// (client address, endpoint bucket) → (window start, requests so far).
type Counts = HashMap<(IpAddr, &'static str), (Instant, u32)>;

/// Fixed-window request counter per (IP, bucket), kept in memory.
///
/// In memory is fine while there is one API process. With several, move
/// this to Postgres or Redis (see docs/NOTES.md).
#[derive(Debug)]
pub struct RateLimiter {
    window: Duration,
    counts: Mutex<Counts>,
}

impl RateLimiter {
    /// Above this many tracked keys, expired ones are swept on the next hit.
    const SWEEP_AT: usize = 50_000;

    pub fn new(window: Duration) -> Self {
        Self {
            window,
            counts: Mutex::new(HashMap::new()),
        }
    }

    /// Count one request. Returns false when the limit is exceeded.
    pub fn hit(&self, ip: IpAddr, bucket: &'static str, max: u32) -> bool {
        let now = Instant::now();
        let mut counts = self.counts.lock().expect("rate limiter poisoned");
        if counts.len() > Self::SWEEP_AT {
            let window = self.window;
            counts.retain(|_, (start, _)| now.duration_since(*start) < window);
        }
        let entry = counts.entry((ip, bucket)).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        entry.1 = entry.1.saturating_add(1);
        entry.1 <= max
    }
}

pub async fn limit_code_requests(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&state, &req);
    if !state
        .limiter
        .hit(ip, "auth_code", state.security.code_requests_per_window)
    {
        return ApiError::RateLimited.into_response();
    }
    next.run(req).await
}

pub async fn limit_verify_requests(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&state, &req);
    if !state
        .limiter
        .hit(ip, "auth_verify", state.security.verify_requests_per_window)
    {
        return ApiError::RateLimited.into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_applies_per_ip_and_per_bucket() {
        let limiter = RateLimiter::new(Duration::from_secs(60));
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        for _ in 0..3 {
            assert!(limiter.hit(a, "x", 3));
        }
        assert!(!limiter.hit(a, "x", 3), "fourth hit is over the limit");
        assert!(limiter.hit(b, "x", 3), "another IP has its own budget");
        assert!(limiter.hit(a, "y", 3), "another bucket has its own budget");
    }

    #[test]
    fn the_window_resets() {
        let limiter = RateLimiter::new(Duration::from_millis(30));
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(limiter.hit(ip, "x", 1));
        assert!(!limiter.hit(ip, "x", 1));
        std::thread::sleep(Duration::from_millis(40));
        assert!(limiter.hit(ip, "x", 1));
    }
}
