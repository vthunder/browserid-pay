//! Per-client-IP request limiter ahead of verification. Verifying a
//! presentation costs DNS-over-TLS lookups and status-list fetches, so an
//! unauthenticated caller must not be able to make the relay do that
//! unboundedly. Fixed window per IP; the client IP is the first
//! `X-Forwarded-For` entry (dokku's nginx sets it), else the socket address.

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_PER_MINUTE: u32 = 120;

#[derive(Clone, Default)]
pub struct IpLimiter(Arc<Mutex<HashMap<String, (Instant, u32)>>>);

impl IpLimiter {
    /// `true` if the request may proceed.
    pub fn allow(&self, ip: &str) -> bool {
        let mut m = self.0.lock().expect("iplimit");
        let now = Instant::now();
        if m.len() > 50_000 {
            m.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        }
        let e = m.entry(ip.to_string()).or_insert((now, 0));
        if now.duration_since(e.0) >= Duration::from_secs(60) {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= MAX_PER_MINUTE
    }
}

pub async fn middleware(
    State(limiter): State<IpLimiter>,
    conn: Option<ConnectInfo<SocketAddr>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| conn.map(|c| c.0.ip().to_string()))
        .unwrap_or_else(|| "unknown".into());
    if !limiter.allow(&ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "60")],
            Json(json!({ "error": "rate_limited", "detail": format!("at most {MAX_PER_MINUTE} requests per minute per client") })),
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_caps_requests_per_ip() {
        let l = IpLimiter::default();
        for _ in 0..MAX_PER_MINUTE { assert!(l.allow("1.2.3.4")); }
        assert!(!l.allow("1.2.3.4"));
        assert!(l.allow("5.6.7.8"), "other clients unaffected");
    }
}
