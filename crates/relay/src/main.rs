//! browserid-pay relay: a mailbox for agents.
//!
//! `POST /api/send    { presentation, to, body }` — deliver `body` to the
//! identity `to`, from the caller's ACTING identity (the warrant grantee).
//! `POST /api/receive { presentation, since? }` — the caller's messages
//! (addressed to its acting identity), oldest first, after `since` (a message
//! id). Messages are plain text, size-capped, and expire after a few days.
//! Chat is not evidence: anything that must count later goes on the chain.
//!
//! The relay is a relying party for its own origin: a `message` scope on a
//! warrant for the relay's origin is all it needs. It stores nothing but
//! the messages themselves; there is no directory and no history beyond TTL.

mod config;
mod iplimit;
mod verify;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use config::Config;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};

const MAX_BODY: usize = 16 * 1024;
const TTL_SECS: i64 = 3 * 24 * 3600;
const MAX_PER_RECIPIENT: i64 = 500;
/// Abuse bounds: one sender may not flood the relay or crowd out a mailbox.
const MAX_PER_SENDER_PER_HOUR: i64 = 60;
const MAX_PER_PAIR: i64 = 50;

#[derive(Clone)]
struct App {
    cfg: Arc<Config>,
    db: Arc<Mutex<Connection>>,
    verify: Arc<verify::Verify>,
}

pub fn schema(c: &Connection) -> anyhow::Result<()> {
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS messages (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           sender TEXT NOT NULL,
           recipient TEXT NOT NULL,
           body TEXT NOT NULL,
           at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sends (sender TEXT NOT NULL, at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS sends_sender ON sends(sender, at);
         CREATE INDEX IF NOT EXISTS messages_recipient ON messages(recipient, id);",
    )?;
    Ok(())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Identity comparison per the protocol spec: domain lowercased, local part
/// byte-exact.
pub fn norm(id: &str) -> String {
    match id.rsplit_once('@') {
        Some((l, d)) => format!("{l}@{}", d.to_ascii_lowercase()),
        None => id.to_string(),
    }
}

#[derive(Debug)]
pub enum SendError {
    RateLimited { retry_after_secs: i64 },
    Db(anyhow::Error),
}
impl From<rusqlite::Error> for SendError {
    fn from(e: rusqlite::Error) -> Self { SendError::Db(e.into()) }
}

pub fn send(c: &Connection, sender: &str, to: &str, body: &str) -> Result<i64, SendError> {
    let t = now();
    c.execute("DELETE FROM messages WHERE at < ?1", params![t - TTL_SECS])?;
    let sender = norm(sender);
    let recipient = norm(to);
    // Per-sender rate limit (sliding hour), so a single agent cannot flood.
    // Accounted separately from retention: evicted messages still count.
    c.execute("DELETE FROM sends WHERE at < ?1", params![t - 3600])?;
    let (sent_last_hour, oldest): (i64, Option<i64>) = c.query_row(
        "SELECT COUNT(*), MIN(at) FROM sends WHERE sender=?1",
        params![sender],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if sent_last_hour >= MAX_PER_SENDER_PER_HOUR {
        return Err(SendError::RateLimited { retry_after_secs: (oldest.unwrap_or(t) + 3600 - t).max(1) });
    }
    c.execute("INSERT INTO sends(sender, at) VALUES(?1, ?2)", params![sender, t])?;
    c.execute(
        "INSERT INTO messages(sender, recipient, body, at) VALUES(?1,?2,?3,?4)",
        params![sender, recipient, body, t],
    )?;
    let id = c.last_insert_rowid();
    // Per-pair cap FIRST: a sender evicts only its own older messages to this
    // mailbox, never someone else's.
    c.execute(
        "DELETE FROM messages WHERE sender=?1 AND recipient=?2 AND id NOT IN (SELECT id FROM messages WHERE sender=?1 AND recipient=?2 ORDER BY id DESC LIMIT ?3)",
        params![sender, recipient, MAX_PER_PAIR],
    )?;
    // Then the mailbox-wide bound.
    c.execute(
        "DELETE FROM messages WHERE recipient=?1 AND id NOT IN (SELECT id FROM messages WHERE recipient=?1 ORDER BY id DESC LIMIT ?2)",
        params![recipient, MAX_PER_RECIPIENT],
    )?;
    Ok(id)
}

pub fn receive(c: &Connection, me: &str, since: i64) -> anyhow::Result<Vec<serde_json::Value>> {
    let t = now();
    let mut st = c.prepare(
        "SELECT id, sender, body, at FROM messages WHERE recipient=?1 AND id>?2 AND at>=?3 ORDER BY id ASC LIMIT 100",
    )?;
    let rows = st
        .query_map(params![norm(me), since, t - TTL_SECS], |r| {
            Ok(json!({ "id": r.get::<_, i64>(0)?, "from": r.get::<_, String>(1)?, "body": r.get::<_, String>(2)?, "at": r.get::<_, i64>(3)? }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();
    let cfg = Config::from_env();
    if cfg.dev {
        tracing::warn!("RELAY_DEV=1: pinned issuers, no status checks — never in production");
    }
    let conn = Connection::open(&cfg.database_path)?;
    schema(&conn)?;
    let app = App { cfg: Arc::new(cfg.clone()), db: Arc::new(Mutex::new(conn)), verify: Arc::new(verify::Verify::new(&cfg)?) };
    let router = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/send", post(api_send))
        .route("/api/receive", post(api_receive))
        .with_state(app)
        // Ahead of verification: per-IP request cap (iplimit.rs).
        .layer(axum::middleware::from_fn_with_state(iplimit::IpLimiter::default(), iplimit::middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!(origin = %cfg.origin, %addr, "relay listening");
    axum::serve(tokio::net::TcpListener::bind(&addr).await?, router).await?;
    Ok(())
}

async fn index(State(app): State<App>) -> Html<String> {
    Html(format!(
        "<!doctype html><meta charset=utf-8><title>relay.dsp.fyi</title>\
         <style>body{{font:15px/1.5 system-ui;max-width:40rem;margin:3rem auto;padding:0 1rem;color:#222}}code{{background:#eee;padding:.1em .3em;border-radius:4px}}</style>\
         <h1>relay.dsp.fyi</h1><p>A mailbox for agents. Messages are addressed by identity and authenticated with browserid \
         presentations for <code>{}</code> (scope <code>message</code>). Plain text, {} KB max, kept {} days. \
         Chat is not evidence — anything that must count later goes on the ledger.</p>\
         <p><code>POST /api/send {{ presentation, to, body }}</code> · <code>POST /api/receive {{ presentation, since? }}</code></p>",
        app.cfg.origin, MAX_BODY / 1024, TTL_SECS / 86400
    ))
}

#[derive(Deserialize)]
struct SendBody {
    presentation: String,
    to: String,
    body: String,
}

#[derive(Deserialize)]
struct ReceiveBody {
    presentation: String,
    #[serde(default)]
    since: Option<i64>,
}

fn unauthorized(e: String) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": "verification_failed", "detail": e }))).into_response()
}

async fn api_send(State(app): State<App>, Json(b): Json<SendBody>) -> Response {
    let v = match app.verify.presentation(&b.presentation).await {
        Ok(v) => v,
        Err(e) => return unauthorized(e),
    };
    if !v.scopes.iter().any(|s| s == "message") {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "no_grant", "detail": "warrant lacks the message scope" }))).into_response();
    }
    if b.body.len() > MAX_BODY || b.body.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "bad_request", "detail": format!("body must be 1..{MAX_BODY} bytes") }))).into_response();
    }
    if !b.to.contains('@') || b.to.len() > 254 || b.to.chars().any(char::is_whitespace) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "bad_request", "detail": "to must be an identity" }))).into_response();
    }
    let res = { let c = app.db.lock().expect("db"); send(&c, &v.grantee, &b.to, &b.body) };
    match res {
        Ok(id) => Json(json!({ "id": id, "from": v.grantee, "to": norm(&b.to), "at": now() })).into_response(),
        Err(SendError::RateLimited { retry_after_secs }) => (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
            Json(json!({ "error": "rate_limited", "detail": format!("at most {MAX_PER_SENDER_PER_HOUR} messages per hour"), "retry_after_secs": retry_after_secs })),
        ).into_response(),
        Err(SendError::Db(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn api_receive(State(app): State<App>, Json(b): Json<ReceiveBody>) -> Response {
    let v = match app.verify.presentation(&b.presentation).await {
        Ok(v) => v,
        Err(e) => return unauthorized(e),
    };
    if !v.scopes.iter().any(|s| s == "message") {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "no_grant", "detail": "warrant lacks the message scope" }))).into_response();
    }
    let res = { let c = app.db.lock().expect("db"); receive(&c, &v.grantee, b.since.unwrap_or(0)) };
    match res {
        Ok(msgs) => Json(json!({ "me": v.grantee, "messages": msgs })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_receive_since_and_normalization() {
        let c = Connection::open_in_memory().unwrap();
        schema(&c).unwrap();
        let a = send(&c, "alice+agent@X.test", "bob+agent@Y.TEST", "hi bob").unwrap();
        let b = send(&c, "alice+agent@x.test", "bob+agent@y.test", "second").unwrap();
        let got = receive(&c, "bob+agent@y.test", 0).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0]["from"], "alice+agent@x.test");
        assert_eq!(got[0]["body"], "hi bob");
        let after = receive(&c, "bob+agent@y.test", a).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0]["id"], b);
        assert!(receive(&c, "alice+agent@x.test", 0).unwrap().is_empty(), "not addressed to alice");
    }

    #[test]
    fn mailbox_is_bounded() {
        let c = Connection::open_in_memory().unwrap();
        schema(&c).unwrap();
        for i in 0..(MAX_PER_RECIPIENT + 20) {
            send(&c, &format!("s{i}@x"), "b@y", &format!("m{i}")).unwrap();
        }
        let n: i64 = c.query_row("SELECT COUNT(*) FROM messages WHERE recipient='b@y'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, MAX_PER_RECIPIENT);
        let oldest = receive(&c, "b@y", 0).unwrap();
        assert_eq!(oldest[0]["body"], "m20", "the oldest were dropped");
    }

    #[test]
    fn a_sender_cannot_crowd_out_others_and_is_rate_limited() {
        let c = Connection::open_in_memory().unwrap();
        schema(&c).unwrap();
        send(&c, "friend@x", "victim@y", "hello").unwrap();
        // The flooder hits the hourly limit long before it could evict anyone.
        let mut sent = 0;
        for i in 0..(MAX_PER_SENDER_PER_HOUR + 5) {
            match send(&c, "flood@z", "victim@y", &format!("spam {i}")) {
                Ok(_) => sent += 1,
                Err(SendError::RateLimited { retry_after_secs }) => { assert!(retry_after_secs >= 1); break; }
                Err(e) => panic!("{e:?}"),
            }
        }
        assert_eq!(sent, MAX_PER_SENDER_PER_HOUR);
        let got = receive(&c, "victim@y", 0).unwrap();
        assert!(got.iter().any(|m| m["from"] == "friend@x"), "the friend's message survived");
        // Per-pair cap: even within the rate limit, one pair keeps at most MAX_PER_PAIR.
        let pair: i64 = c.query_row("SELECT COUNT(*) FROM messages WHERE sender='flood@z' AND recipient='victim@y'", [], |r| r.get(0)).unwrap();
        assert!(pair <= MAX_PER_PAIR);
    }
}
