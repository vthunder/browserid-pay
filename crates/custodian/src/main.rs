//! browserid-pay custodian.
//!
//! Holds the human's funds; enforces per-warrant spend caps; settles onward.
//! The agent never holds funds; the payee never learns about browserid.

mod config;
mod db;
mod pay;
mod session;
mod verify;

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use config::Config;
use db::Db;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
struct App {
    cfg: Arc<Config>,
    db: Db,
    verify: Arc<verify::Verify>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();
    let cfg = Config::from_env();
    if cfg.dev {
        tracing::warn!("PAY_DEV=1: pinned issuers, no status checks — never in production");
    }
    let db = Db::open(&cfg.database_path)?;
    let verify = verify::Verify::new(&cfg)?;
    let app = App { cfg: Arc::new(cfg.clone()), db, verify: Arc::new(verify) };

    let router = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/login", post(api_login))
        .route("/api/logout", post(api_logout))
        .route("/api/me", get(api_me))
        .route("/api/pay", post(api_pay))
        .route("/admin/credit", post(admin_credit))
        .with_state(app)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!(origin = %cfg.origin, %addr, "custodian listening");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

async fn index(State(app): State<App>) -> Html<String> {
    Html(
        include_str!("../static/index.html")
            .replace("{{BROKER}}", &app.cfg.broker)
            .replace("{{ORIGIN}}", &app.cfg.origin)
            .replace("{{CURRENCY}}", &app.cfg.currency),
    )
}

#[derive(Deserialize)]
struct LoginBody {
    presentation: String,
}

async fn api_login(State(app): State<App>, Json(body): Json<LoginBody>) -> Response {
    match app.verify.presentation(&body.presentation).await {
        Ok(v) if v.identity.grantee == v.identity.email => {
            let token = session::issue(&app.cfg.session_secret, &v.identity.email);
            let secure = if app.cfg.origin.starts_with("https://") { "; Secure" } else { "" };
            (
                [(
                    header::SET_COOKIE,
                    format!("{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=43200{secure}", session::COOKIE),
                )],
                Json(json!({ "email": v.identity.email })),
            )
                .into_response()
        }
        Ok(_) => (StatusCode::FORBIDDEN, Json(json!({ "reason": "the ledger is for the account holder, not an agent" })))
            .into_response(),
        Err(e) => (StatusCode::UNAUTHORIZED, Json(json!({ "reason": e }))).into_response(),
    }
}

async fn api_logout() -> Response {
    (
        [(header::SET_COOKIE, format!("{}=; Path=/; HttpOnly; Max-Age=0", session::COOKIE))],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

fn session_identity(app: &App, headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    let token = session::from_header(cookie)?;
    session::verify(&app.cfg.session_secret, token)
}

async fn api_me(State(app): State<App>, headers: HeaderMap) -> Response {
    let Some(identity) = session_identity(&app, &headers) else {
        return Json(json!({ "email": null })).into_response();
    };
    let account = app.db.account(&identity).ok().flatten();
    let now = db::now();
    let warrants: Vec<_> = app
        .db
        .warrants_for(&identity)
        .unwrap_or_default()
        .into_iter()
        .map(|w| {
            let since = db::window_start(w.iat, w.window_secs, now);
            let spent = app
                .db
                .with(|c| Db::spent_since(c, &w.key, since))
                .unwrap_or(0);
            json!({
                "key": w.key, "grantee": w.grantee, "scope": w.scope,
                "cap": pay::fmt_minor(w.cap_minor), "currency": w.currency,
                "window_secs": w.window_secs,
                "spent": pay::fmt_minor(spent),
                "remaining": pay::fmt_minor((w.cap_minor - spent).max(0)),
                "window_resets_at": w.window_secs.map(|s| since + s),
                "exp": w.exp,
            })
        })
        .collect();
    let receipts: Vec<_> = app
        .db
        .receipts_for(&identity, 100)
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id, "kind": r.kind, "grantor": r.grantor, "grantee": r.grantee,
                "amount": pay::fmt_minor(r.amount_minor), "currency": r.currency,
                "payee": r.payee, "memo": r.memo, "at": r.at,
            })
        })
        .collect();
    let ledger: Vec<_> = app
        .db
        .ledger_for(&identity, 100)
        .unwrap_or_default()
        .into_iter()
        .map(|l| json!({ "delta": pay::fmt_minor(l.delta_minor), "kind": l.kind, "reference": l.reference, "at": l.at }))
        .collect();
    Json(json!({
        "email": identity,
        "currency": app.cfg.currency,
        "balance": pay::fmt_minor(account.map(|a| a.balance_minor).unwrap_or(0)),
        "warrants": warrants,
        "receipts": receipts,
        "ledger": ledger,
    }))
    .into_response()
}

async fn api_pay(State(app): State<App>, Json(req): Json<pay::PayRequest>) -> Response {
    match pay::pay(&app.verify, &app.db, &app.cfg.currency, req).await {
        Ok(r) => Json(json!({
            "receipt": {
                "id": r.id, "kind": r.kind, "grantor": r.grantor, "grantee": r.grantee,
                "amount": pay::fmt_minor(r.amount_minor), "currency": r.currency,
                "payee": r.payee, "memo": r.memo, "at": r.at,
            }
        }))
        .into_response(),
        Err(e) => {
            let code = match e.error {
                "verification_failed" | "revoked" => StatusCode::UNAUTHORIZED,
                "no_grant" | "cap_exceeded" => StatusCode::FORBIDDEN,
                "insufficient_funds" => StatusCode::PAYMENT_REQUIRED,
                "bad_request" | "unsupported_currency" => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (code, Json(e)).into_response()
        }
    }
}

#[derive(Deserialize)]
struct CreditBody {
    identity: String,
    amount: String,
    #[serde(default)]
    memo: Option<String>,
}

/// Admin test credit (v1: balances are play money until a real rail lands).
async fn admin_credit(State(app): State<App>, headers: HeaderMap, Json(body): Json<CreditBody>) -> Response {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
    if app.cfg.admin_token.is_empty() || auth != format!("Bearer {}", app.cfg.admin_token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "admin token" }))).into_response();
    }
    let Some(minor) = browserid_core::device::parse_money_minor(&body.amount) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "amount" }))).into_response();
    };
    let identity = body.identity.trim().to_string();
    let currency = app.cfg.currency.clone();
    let memo = body.memo.clone();
    let res = app.db.with(|c| {
        let id = format!("dep_{}", db::now());
        let bal = Db::adjust(c, &identity, &currency, minor as i64, "credit", Some(&id))?;
        Db::insert_receipt(
            c,
            &db::Receipt {
                id,
                kind: "credit".into(),
                grantor: identity.clone(),
                grantee: identity.clone(),
                warrant_key: None,
                amount_minor: minor as i64,
                currency: currency.clone(),
                payee: None,
                memo,
                at: db::now(),
            },
        )?;
        Ok(bal)
    });
    match res {
        Ok(bal) => Json(json!({ "balance": pay::fmt_minor(bal) })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}
