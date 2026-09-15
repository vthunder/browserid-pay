//! browserid-pay custodian.
//!
//! Holds the human's funds; enforces per-warrant spend caps; settles onward.
//! The agent never holds funds; the payee never learns about browserid.

mod config;
mod db;
mod escrow;
mod pay;
mod publish;
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
    publisher: Arc<dyn escrow::Publisher>,
    identity: Arc<chain::Identity>,
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
    db.with(|c| escrow::schema(c))?;
    let verify = verify::Verify::new(&cfg)?;

    // The custodian's own chain identity: provisioned once (a human approves
    // at the broker; see /api/setup), then it posts its listing, escrow locks
    // and settlement attestations through the daemon. `PAY_SUBMIT_URL` points
    // a dev build at a stub daemon's submit route.
    let data = std::path::PathBuf::from(&cfg.data_dir);
    std::fs::create_dir_all(&data)?;
    let identity = Arc::new(chain::Identity::new(data.join("custodian-credential.json")));
    let reader: Arc<dyn chain::ChainReader> = Arc::new(chain::DaemonReader::new(&cfg.daemon, &cfg.repo));
    let writer: Arc<dyn chain::ChainWriter> = match std::env::var("PAY_SUBMIT_URL") {
        Ok(url) if cfg.dev => Arc::new(chain::DaemonWriter::at_url(&url)),
        _ => Arc::new(chain::DaemonWriter::new(&cfg.daemon, &cfg.repo)),
    };
    let publisher = Arc::new(publish::ChainPublisher {
        identity: identity.clone(),
        writer: writer.clone(),
        chain_audience: cfg.chain_audience.clone(),
    });
    let app = App { cfg: Arc::new(cfg.clone()), db, verify: Arc::new(verify), publisher: publisher.clone(), identity: identity.clone() };

    // Load the identity, or provision it in the background (prints the approval
    // URL). In dev mode without a credential nothing is provisioned unless
    // PAY_PROVISION=1 — the demo runs without a chain identity.
    let boot_cfg = cfg.clone();
    tokio::spawn(async move {
        match identity.load().await {
            Ok(true) => {}
            Ok(false) if boot_cfg.dev && std::env::var("PAY_PROVISION").is_err() => {
                tracing::warn!("PAY_DEV without a credential: chain writes are dropped (set PAY_PROVISION=1 to provision)");
                return;
            }
            Ok(false) => {
                if let Err(e) = identity.provision(&boot_cfg.broker, &publish::provision_spec(&boot_cfg)).await {
                    tracing::error!("provisioning failed: {e:#}");
                    identity.state.lock().await.error = Some(e.to_string());
                    return;
                }
            }
            Err(e) => {
                tracing::error!("loading identity: {e:#}");
                identity.state.lock().await.error = Some(e.to_string());
                return;
            }
        }
        if let Some(me) = identity.email().await {
            let want = publish::listing(&boot_cfg, &me);
            if let Err(e) = chain::ensure_listing(&identity, reader.as_ref(), writer.as_ref(), &boot_cfg.chain_audience, "custodians", &want).await {
                tracing::error!("provider listing: {e:#}");
            }
        }
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/setup", get(api_setup))
        .route("/api/login", post(api_login))
        .route("/api/logout", post(api_logout))
        .route("/api/me", get(api_me))
        .route("/api/pay", post(api_pay))
        .route("/admin/credit", post(admin_credit))
        .route("/api/escrow/lock", post(escrow_lock))
        .route("/api/escrow/:id", get(escrow_get))
        .route("/api/escrow/:id/acknowledge", post(escrow_acknowledge))
        .route("/api/escrow/:id/dispute", post(escrow_dispute))
        .route("/api/escrow/:id/ruling", post(escrow_ruling))
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

/// Provisioning status: the custodian's identity, or the pending approval.
async fn api_setup(State(app): State<App>) -> Json<chain::SetupState> {
    Json(app.identity.state.lock().await.clone())
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
    let escrows: Vec<_> = app
        .db
        .with(|c| escrow::for_party(c, &identity))
        .unwrap_or_default()
        .iter()
        .map(escrow::view)
        .collect();
    Json(json!({
        "email": identity,
        "escrows": escrows,
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
        let id = format!("dep_{}", pay::random_id());
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

fn escrow_status(e: &escrow::EscrowError) -> StatusCode {
    match e.error {
        "verification_failed" | "revoked" => StatusCode::UNAUTHORIZED,
        "no_grant" | "cap_exceeded" | "not_payer" | "not_party" | "not_adjudicator" | "payee_bundle_invalid" | "counterparty_not_allowed" | "duration_exceeded" => StatusCode::FORBIDDEN,
        "insufficient_funds" => StatusCode::PAYMENT_REQUIRED,
        "not_found" => StatusCode::NOT_FOUND,
        "already_locked" | "bad_state" => StatusCode::CONFLICT,
        "bad_request" | "unsupported_currency" | "wrong_custodian" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn escrow_lock(State(app): State<App>, Json(req): Json<escrow::LockRequest>) -> Response {
    match escrow::lock(&app.verify, &app.db, app.publisher.as_ref(), &app.cfg.origin, &app.cfg.currency, req).await {
        Ok(e) => Json(json!({ "lock": escrow::view(&e) })).into_response(),
        Err(e) => (escrow_status(&e), Json(e)).into_response(),
    }
}

async fn escrow_get(State(app): State<App>, axum::extract::Path(id): axum::extract::Path<String>) -> Response {
    match app.db.with(|c| escrow::get(c, &id)) {
        Ok(Some(e)) => Json(escrow::view(&e)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
struct PresentationBody {
    presentation: String,
}

async fn escrow_acknowledge(State(app): State<App>, axum::extract::Path(id): axum::extract::Path<String>, Json(b): Json<PresentationBody>) -> Response {
    match escrow::acknowledge(&app.verify, &app.db, app.publisher.as_ref(), &id, &b.presentation).await {
        Ok(e) => Json(escrow::view(&e)).into_response(),
        Err(e) => (escrow_status(&e), Json(e)).into_response(),
    }
}

async fn escrow_dispute(State(app): State<App>, axum::extract::Path(id): axum::extract::Path<String>, Json(b): Json<PresentationBody>) -> Response {
    match escrow::dispute(&app.verify, &app.db, &id, &b.presentation).await {
        Ok((e, sealed)) => Json(json!({ "escrow": escrow::view(&e), "sealed_terms": sealed })).into_response(),
        Err(e) => (escrow_status(&e), Json(e)).into_response(),
    }
}

async fn escrow_ruling(State(app): State<App>, axum::extract::Path(id): axum::extract::Path<String>, Json(req): Json<escrow::RulingRequest>) -> Response {
    match escrow::ruling(&app.verify, &app.db, app.publisher.as_ref(), &app.cfg.origin, &id, req).await {
        Ok(e) => Json(escrow::view(&e)).into_response(),
        Err(e) => (escrow_status(&e), Json(e)).into_response(),
    }
}
