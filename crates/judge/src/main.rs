//! browserid-pay adjudicator (judge.dsp.fyi).
//!
//! An AI judge with its own browserid identity. Parties file a dispute with
//! the sealed terms; the judge reads the on-chain record, rules with Claude,
//! posts the ruling to the chain, delivers it to the custodian named in the
//! proposal, and attests the outcome on both parties.

mod cases;
mod config;
mod llm;
mod seal;
mod verify;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use cases::{Case, Cases};
use chain::{ChainReader, ChainWriter, Identity, ProvisionSpec, WriteSpec};
use config::Config;
use llm::{CaseFile, Ruler};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct App {
    cfg: Arc<Config>,
    cases: Cases,
    verify: Arc<verify::Verify>,
    seal: Arc<seal::SealKey>,
    identity: Arc<Identity>,
    reader: Arc<dyn ChainReader>,
    writer: Arc<dyn ChainWriter>,
    ruler: Arc<dyn Ruler>,
    http: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();
    let cfg = Config::from_env();
    if cfg.dev {
        tracing::warn!("JUDGE_DEV=1: pinned issuers, no status checks — never in production");
    }
    let data = PathBuf::from(&cfg.data_dir);
    std::fs::create_dir_all(&data)?;
    let cases = Cases::open(data.join("cases.db").to_str().unwrap())?;
    let seal = seal::SealKey::load_or_create(&data.join("judge-seal-key.json"))?;
    let identity = Arc::new(Identity::new(data.join("judge-credential.json")));
    // Writes go through the daemon (it holds the repo's TurboDA credentials and
    // refreshes issuer DNSSEC evidence ahead of the write). `JUDGE_SUBMIT_URL`
    // points a dev build at a stub daemon's submit route.
    let writer: Arc<dyn ChainWriter> = match std::env::var("JUDGE_SUBMIT_URL") {
        Ok(url) if cfg.dev => Arc::new(chain::DaemonWriter::at_url(&url)),
        _ => Arc::new(chain::DaemonWriter::new(&cfg.daemon, &cfg.chain_audience)),
    };
    let ruler: Arc<dyn Ruler> = match std::env::var("JUDGE_FAKE_RULING") {
        Ok(j) if cfg.dev => {
            tracing::warn!("JUDGE_FAKE_RULING set: rulings are canned (dev only)");
            Arc::new(llm::StaticRuler(serde_json::from_str(&j).expect("JUDGE_FAKE_RULING must be JSON")))
        }
        _ => Arc::new(llm::ClaudeRuler::new(cfg.anthropic_api_key.clone(), cfg.anthropic_model.clone())),
    };
    let app = App {
        cfg: Arc::new(cfg.clone()),
        cases,
        verify: Arc::new(verify::Verify::new(&cfg)?),
        seal: Arc::new(seal),
        identity: identity.clone(),
        reader: Arc::new(chain::DaemonReader::new(&cfg.daemon, &cfg.chain_audience)),
        writer,
        ruler,
        http: reqwest::Client::builder().no_proxy().build()?,
    };

    // Identity: load, or provision in the background (prints the approval URL).
    let boot = app.clone();
    tokio::spawn(async move {
        match boot.identity.load().await {
            Ok(true) => {}
            Ok(false) => {
                if let Err(e) = boot.identity.provision(&boot.cfg.broker, &provision_spec(&boot.cfg)).await {
                    tracing::error!("provisioning failed: {e:#}");
                    boot.identity.state.lock().await.error = Some(e.to_string());
                    return;
                }
            }
            Err(e) => {
                tracing::error!("loading identity: {e:#}");
                boot.identity.state.lock().await.error = Some(e.to_string());
                return;
            }
        }
        if let Err(e) = ensure_listing(&boot).await {
            tracing::error!("provider listing: {e:#}");
        }
    });

    // Response-window sweep: rule on cases whose window passed with no response.
    let sweep = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            rule_due_cases(&sweep).await;
        }
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/setup", get(api_setup))
        .route("/api/dispute", post(api_dispute))
        .route("/api/cases", get(api_cases_list))
        .route("/api/cases/:id", get(api_case))
        .route("/api/cases/:id/respond", post(api_respond))
        .route("/api/cases/:id/deliver", post(api_deliver))
        .with_state(app)
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!(origin = %cfg.origin, %addr, "judge listening");
    axum::serve(tokio::net::TcpListener::bind(&addr).await?, router).await?;
    Ok(())
}

/// What the judge asks the approver for: its own delegated identity (handle
/// `judge`), a chain warrant for its listing, rulings and attestations, and a
/// `ruling` warrant per custodian it delivers to.
fn provision_spec(cfg: &Config) -> ProvisionSpec {
    let mut grants = vec![browserid_agent::GrantRequest {
        audience: cfg.chain_audience.clone(),
        scopes: vec![
            "action:post".into(),
            "path:/agreements/**".into(),
            "path:/providers/adjudicators/**".into(),
            "path:/u/*/attestations/**".into(),
        ],
    }];
    for c in &cfg.custodians {
        grants.push(browserid_agent::GrantRequest { audience: c.clone(), scopes: vec!["ruling".into()] });
    }
    ProvisionSpec {
        handle: cfg.handle.clone(),
        label: "browserid-pay adjudicator".into(),
        message: "The AI adjudicator for browserid-pay agreements: it posts rulings to the dsp.fyi database and delivers them to the escrow custodian. Name it (the handle it asks for is `judge`) and pick which of your identities delegates; it acts as its own identity, attributed to you.".into(),
        grants,
    }
}

/// Ensure `/providers/adjudicators/<identity>/listing` (provider.v1) is on chain and current.
async fn ensure_listing(app: &App) -> anyhow::Result<()> {
    let Some(me) = app.identity.email().await else { return Ok(()) };
    chain::ensure_listing(&app.identity, app.reader.as_ref(), app.writer.as_ref(), &app.cfg.chain_audience, "adjudicators", &listing(app, &me)).await
}

fn listing(app: &App, me: &str) -> Value {
    json!({
        "v": 1,
        "identity": me,
        "origin": app.cfg.origin,
        "price": { "amount": app.cfg.price_amount, "currency": app.cfg.price_currency },
        "sealing_key": app.seal.public_b64(),
        "sealing_scheme": "sealed-x25519-v1",
        "conformance": "browserid-pay-adjudicator-v1",
    })
}

async fn index(State(app): State<App>) -> Html<String> {
    let me = app.identity.email().await.unwrap_or_else(|| "(not yet provisioned)".into());
    let (total, ruled) = app.cases.count().unwrap_or((0, 0));
    Html(
        include_str!("../static/index.html")
            .replace("{{IDENTITY}}", &me)
            .replace("{{PRICE}}", &format!("{} {}", app.cfg.price_amount, app.cfg.price_currency))
            .replace("{{SEAL}}", &app.seal.public_b64())
            .replace("{{CASES}}", &total.to_string())
            .replace("{{RULED}}", &ruled.to_string()),
    )
}

async fn api_setup(State(app): State<App>) -> Json<chain::SetupState> {
    Json(app.identity.state.lock().await.clone())
}

#[derive(Deserialize)]
struct DisputeBody {
    presentation: String,
    contract_id: String,
    sealed_terms: String,
    #[serde(default)]
    note: Option<String>,
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "error": msg.into() }))).into_response()
}

/// Either party files: verify the caller, unseal the terms, read the on-chain
/// record, check the caller is a party and we are the named adjudicator,
/// then open the case with a response window for the other party. The ruling
/// comes when they respond, or when the window passes (sweep).
async fn api_dispute(State(app): State<App>, Json(body): Json<DisputeBody>) -> Response {
    let who = match app.verify.presentation(&body.presentation).await {
        // Parties are acting identities (the agents), not their grantors.
        Ok(v) => v.grantee,
        Err(e) => return err(StatusCode::UNAUTHORIZED, format!("verification failed: {e}")),
    };
    let id = body.contract_id.trim().to_string();
    if !id.starts_with("agr_") || id.len() > 64 {
        return err(StatusCode::BAD_REQUEST, "contract_id");
    }
    if let Ok(Some(c)) = app.cases.get(&id) {
        return (StatusCode::CONFLICT, Json(json!({ "error": "already_filed", "state": c.state }))).into_response();
    }
    let terms: Value = match app.seal.unseal(&body.sealed_terms).and_then(|b| Ok(serde_json::from_slice(&b)?)) {
        Ok(t) => t,
        Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string()),
    };
    if terms["id"].as_str() != Some(id.as_str()) {
        return err(StatusCode::BAD_REQUEST, "terms.id does not match contract_id");
    }
    let path = format!("/agreements/{id}/");
    let proposal = match app.reader.get_object(&path, "proposal").await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "no proposal on chain"),
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain read: {e}")),
    };
    let acceptance = match app.reader.get_object(&path, "acceptance").await {
        Ok(Some(a)) => a,
        Ok(None) => return err(StatusCode::NOT_FOUND, "no acceptance on chain"),
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain read: {e}")),
    };
    let me = app.identity.email().await.unwrap_or_default();
    if proposal.value["adjudicator"].as_str() != Some(me.as_str()) {
        return err(StatusCode::FORBIDDEN, format!("this contract names a different adjudicator (I am {me})"));
    }
    let parties: Vec<String> = ["proposer", "acceptor"]
        .iter()
        .filter_map(|k| proposal.value["parties"][k].as_str().map(String::from))
        .collect();
    if !parties.contains(&who) {
        return err(StatusCode::FORBIDDEN, "caller is not a party to this contract");
    }
    if !parties.contains(&proposal.creator) || !parties.contains(&acceptance.creator) {
        return err(StatusCode::CONFLICT, "proposal/acceptance authors do not match the named parties");
    }
    // Commitment check: the sealed terms must be the ones the proposal committed to.
    if let Some(c) = proposal.value["commitment"].as_str() {
        let got = commitment(&terms);
        if got != c {
            return err(StatusCode::BAD_REQUEST, format!("terms do not match the proposal commitment ({got} != {c})"));
        }
    }
    // Evidence in CHAIN ORDER, which is what the adjudicator is told to read it
    // in — and until now was never given (browserid-pay-e1ou). `block` is the
    // only time in this record no party controls; anything time-like inside
    // `object` is the writer's own claim and nothing validates it. Unconfirmed
    // writes have no block: they have not landed, so they have no place in the
    // order and go last, flagged, rather than being silently interleaved.
    let evidence: Vec<Value> = match app.reader.list_prefix(&format!("{path}evidence/")).await {
        Ok(v) => {
            let mut v: Vec<_> = v.into_iter().filter(|o| parties.contains(&o.creator)).collect();
            v.sort_by_key(|o| (o.block.is_none(), o.block, o.id.clone()));
            v.into_iter()
                .map(|o| json!({ "id": o.id, "by": o.creator, "confirmed": o.confirmed,
                                 "block": o.block, "object": o.value }))
                .collect()
        }
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain read: {e}")),
    };
    // The response window is measured in BLOCKS, not seconds: it decides whether
    // a party forfeited their chance to answer, so it must not rest on anyone's
    // clock. Fail closed — a judge that cannot read the head must not invent a
    // deadline.
    let head = match app.reader.head_block().await {
        Ok(h) => h,
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain head: {e}")),
    };
    let custodian = proposal.value["custodian"].as_str().unwrap_or("").to_string();
    let case = Case {
        contract_id: id.clone(),
        state: "open".into(),
        disputed_by: who.clone(),
        dispute_note: body.note.map(|n| n.chars().take(2000).collect()),
        custodian,
        parties,
        terms,
        proposal: proposal.value.clone(),
        evidence: Value::Array(evidence),
        ruling: None,
        submission: None,
        error: None,
        opened_at: cases::now(),
        updated_at: cases::now(),
        respond_by_block: head as i64 + app.cfg.response_window_blocks,
        response: None,
    };
    let respond_by_block = case.respond_by_block;
    if let Err(e) = app.cases.insert(&case) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    (StatusCode::ACCEPTED, Json(json!({ "contract_id": id, "state": "open", "respond_by_block": respond_by_block }))).into_response()
}

#[derive(Deserialize)]
struct Reveal {
    evidence_id: String,
    content: Value,
}

#[derive(Deserialize)]
struct RespondBody {
    presentation: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    reveals: Vec<Reveal>,
}

/// `sha256:` + hex over a revealed content: a string is hashed as-is, JSON
/// as its canonical form — exactly how the SDK commits at delivery.
fn content_commitment(content: &Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = match content {
        Value::String(s) => s.clone(),
        other => canonical(other),
    };
    format!("sha256:{}", hex::encode(Sha256::digest(bytes.as_bytes())))
}

/// The counterparty answers the dispute: a note and/or reveals of its own
/// delivery commitments. Each reveal is checked against the commitment on
/// the named evidence object; the case then goes to ruling at once.
async fn api_respond(State(app): State<App>, Path(id): Path<String>, Json(body): Json<RespondBody>) -> Response {
    let who = match app.verify.presentation(&body.presentation).await {
        Ok(v) => v.grantee,
        Err(e) => return err(StatusCode::UNAUTHORIZED, format!("verification failed: {e}")),
    };
    let case = match app.cases.get(&id) {
        Ok(Some(c)) => c,
        Ok(None) => return err(StatusCode::NOT_FOUND, "no such case"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if !case.parties.contains(&who) || who == case.disputed_by {
        return err(StatusCode::FORBIDDEN, "only the party the dispute is against may respond");
    }
    if case.state != "open" {
        return (StatusCode::CONFLICT, Json(json!({ "error": "not_open", "state": case.state }))).into_response();
    }
    let path = format!("/agreements/{id}/evidence/");
    let mut reveals = Vec::new();
    for r in body.reveals.iter().take(16) {
        let ev = match app.reader.get_object(&path, &r.evidence_id).await {
            Ok(Some(o)) => o,
            Ok(None) => return err(StatusCode::BAD_REQUEST, format!("no evidence {} on chain", r.evidence_id)),
            Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain read: {e}")),
        };
        if ev.creator != who || ev.value["kind"].as_str() != Some("delivery") {
            return err(StatusCode::BAD_REQUEST, format!("{} is not a delivery of yours", r.evidence_id));
        }
        let committed = ev.value["commitment"].as_str().unwrap_or("").to_string();
        let got = content_commitment(&r.content);
        let matched = !committed.is_empty() && committed == got;
        let mut entry = json!({ "evidence_id": r.evidence_id, "commitment": committed, "matched": matched });
        if matched {
            // Keep the opened content for the ruler (capped so a reveal cannot
            // bloat the case file).
            let text = serde_json::to_string(&r.content).unwrap_or_default();
            entry["content"] = if text.len() > 32_000 { Value::String(text[..32_000].to_string()) } else { r.content.clone() };
        }
        reveals.push(entry);
    }
    let response = json!({
        "by": who,
        "note": body.note.map(|n| n.chars().take(2000).collect::<String>()),
        "reveals": reveals,
        "at": cases::now(),
    });
    if let Err(e) = app.cases.respond(&id, &response) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    spawn_ruling(&app, &id);
    (StatusCode::ACCEPTED, Json(json!({ "contract_id": id, "state": "responded", "reveals": response["reveals"] }))).into_response()
}

fn spawn_ruling(app: &App, id: &str) {
    let bg = app.clone();
    let cid = id.to_string();
    tokio::spawn(async move {
        if let Err(e) = rule_and_deliver(&bg, &cid).await {
            tracing::error!(contract = %cid, "ruling failed: {e:#}");
            let _ = bg.cases.update(&cid, "failed", None, None, Some(&e.to_string()));
        }
    });
}

/// Rule on every open case whose response window has passed.
pub async fn rule_due_cases(app: &App) {
    // The sweep still WAKES on a wall clock — that is only when we look. What
    // is actually due is decided by the chain head.
    let head = match app.reader.head_block().await {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("sweep: cannot read chain head, skipping: {e}");
            return;
        }
    };
    let due = match app.cases.due(head) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("sweep: {e}");
            return;
        }
    };
    for c in due {
        tracing::info!(contract = %c.contract_id, "response window passed with no response; ruling");
        // Mark first so a slow ruling is not picked up twice.
        let _ = app.cases.update(&c.contract_id, "responded", None, None, None);
        if let Err(e) = rule_and_deliver(app, &c.contract_id).await {
            tracing::error!(contract = %c.contract_id, "ruling failed: {e:#}");
            let _ = app.cases.update(&c.contract_id, "failed", None, None, Some(&e.to_string()));
        }
    }
}

/// Cases where `party` is a party (`?party=<identity>`): id, state, window,
/// who disputed. The record itself is public on chain; this is just the
/// judge's bookkeeping, so no authentication.
async fn api_cases_list(State(app): State<App>, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Response {
    let Some(party) = q.get("party") else { return err(StatusCode::BAD_REQUEST, "party required") };
    match app.cases.for_party(party) {
        Ok(v) => Json(json!({ "cases": v.iter().map(|c| json!({
            "contract_id": c.contract_id, "state": c.state, "disputed_by": c.disputed_by,
            "respond_by_block": c.respond_by_block, "responded": c.response.is_some(), "opened_at": c.opened_at,
        })).collect::<Vec<_>>() })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// `sha256:` + hex over the canonical (JCS) terms. JCS for the subset we
/// produce = serde_json with sorted keys and no whitespace.
pub fn commitment(terms: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canon = canonical(terms);
    format!("sha256:{}", hex::encode(Sha256::digest(canon.as_bytes())))
}

fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical(&m[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        other => serde_json::to_string(other).unwrap(),
    }
}

/// `payer` | `payee` | `party` (a proposal that names no roles) for an identity.
fn party_role<'a>(proposal: &'a serde_json::Value, who: &str) -> &'a str {
    if proposal["roles"]["payer"].as_str() == Some(who) {
        "payer"
    } else if proposal["roles"]["payee"].as_str() == Some(who) {
        "payee"
    } else {
        "party"
    }
}

/// The fraction of the price the ruling awards to `role` (0..1).
fn share_for(ruling: &llm::Ruling, role: &str) -> f64 {
    let payee = match ruling.outcome.as_str() {
        "payee" => 1.0,
        "payer" => 0.0,
        _ => {
            let (a, b) = ruling
                .split
                .as_ref()
                .map(|s| {
                    (
                        browserid_core::device::parse_money_minor(&s.payee).unwrap_or(0) as f64,
                        browserid_core::device::parse_money_minor(&s.payer).unwrap_or(0) as f64,
                    )
                })
                .unwrap_or((0.0, 0.0));
            if a + b > 0.0 { a / (a + b) } else { 0.5 }
        }
    };
    match role {
        "payee" => payee,
        "payer" => 1.0 - payee,
        _ => 0.5,
    }
}

async fn rule_and_deliver(app: &App, id: &str) -> anyhow::Result<()> {
    let case = app.cases.get(id)?.ok_or_else(|| anyhow::anyhow!("case vanished"))?;
    let file = CaseFile {
        contract_id: id.to_string(),
        terms: case.terms.clone(),
        proposal: case.proposal.clone(),
        evidence: case.evidence.as_array().cloned().unwrap_or_default(),
        dispute_note: case.dispute_note.clone(),
        disputed_by: case.disputed_by.clone(),
        response: case.response.clone(),
        window_expired: case.response.is_none(),
    };
    let raw = app.ruler.rule(&file).await?;
    let ruling = llm::validate(&raw, id, &case.terms)?;
    let ruling_v = serde_json::to_value(&ruling)?;
    app.cases.update(id, "ruled", Some(&ruling_v), None, None)?;

    // 1. The ruling object on chain.
    let wire = app
        .identity
        .sign_write(
            &app.cfg.chain_audience,
            &WriteSpec { path: format!("/agreements/{id}/"), id: "ruling".into(), schema: "agreement.ruling.v1".into(), payload: serde_json::to_vec(&ruling_v)? },
        )
        .await?;
    let sub = app.writer.submit(wire).await?;
    app.cases.update(id, "ruled", None, Some(&sub), None)?;

    // 2. Attestations on both parties, with the reputation-bearing facts the
    //    ruling established (draft 2026-09-16): each party's share of the
    //    price, whether it was late, whether performance was partial, and
    //    whether the disputed party answered with matching reveals.
    if let Some(me) = app.identity.email().await {
        let responded = case.response.is_some();
        let reveals_matched = case
            .response
            .as_ref()
            .and_then(|r| r["reveals"].as_array())
            .map(|rs| !rs.is_empty() && rs.iter().all(|r| r["matched"].as_bool() == Some(true)))
            .unwrap_or(false);
        for p in &case.parties {
            let role = party_role(&case.proposal, p);
            let value = json!({
                "contract": id, "outcome": ruling.outcome, "role": role,
                "share": format!("{:.4}", share_for(&ruling, role)),
                "late": ruling.late_party == role, "partial": ruling.partial,
                "disputed_by": case.disputed_by,
                "responded": if p != &case.disputed_by { responded } else { true },
                "reveals_matched": if p != &case.disputed_by { reveals_matched } else { true },
            });
            let payload = json!({ "subject": p, "type": "agreement:ruled", "value": value, "issued_at": cases::now(), "expires": null, "issuer": me });
            let w = app
                .identity
                .sign_write(
                    &app.cfg.chain_audience,
                    &WriteSpec { path: format!("/u/{me}/attestations/{p}/"), id: "agreement:ruled".into(), schema: "attestation.v1".into(), payload: serde_json::to_vec(&payload)? },
                )
                .await?;
            if let Err(e) = app.writer.submit(w).await {
                tracing::warn!("attestation for {p} not submitted: {e}");
            }
        }
    }

    // 3. Deliver to the custodian.
    deliver(app, id).await
}

async fn deliver(app: &App, id: &str) -> anyhow::Result<()> {
    let case = app.cases.get(id)?.ok_or_else(|| anyhow::anyhow!("case vanished"))?;
    let ruling = case.ruling.clone().ok_or_else(|| anyhow::anyhow!("no ruling yet"))?;
    let custodian = case.custodian.trim_end_matches('/').to_string();
    anyhow::ensure!(!custodian.is_empty(), "proposal names no custodian");
    let presentation = app.identity.presentation_for(&custodian).await?;
    let resp = app
        .http
        .post(format!("{custodian}/api/escrow/{id}/ruling"))
        .json(&json!({ "presentation": presentation, "ruling": ruling }))
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        app.cases.update(id, "ruled", None, None, Some(&format!("custodian {status}: {body}")))?;
        anyhow::bail!("custodian {status}: {body}");
    }
    app.cases.update(id, "delivered", None, None, None)?;
    Ok(())
}

async fn api_case(State(app): State<App>, Path(id): Path<String>) -> Response {
    match app.cases.get(&id) {
        Ok(Some(c)) => Json(json!({
            "contract_id": c.contract_id, "state": c.state, "disputed_by": c.disputed_by, "dispute_note": c.dispute_note,
            "custodian": c.custodian, "parties": c.parties, "ruling": c.ruling,
            "respond_by_block": c.respond_by_block, "response": c.response.as_ref().map(|r| json!({
                "by": r["by"], "note": r["note"], "at": r["at"],
                "reveals": r["reveals"].as_array().map(|a| a.iter().map(|x| json!({ "evidence_id": x["evidence_id"], "matched": x["matched"] })).collect::<Vec<_>>()),
            })),
            "evidence": c.evidence,
            "submission": c.submission, "error": c.error, "opened_at": c.opened_at, "updated_at": c.updated_at,
        }))
        .into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "no such case"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Retry delivery of a ruling the custodian did not accept.
async fn api_deliver(State(app): State<App>, Path(id): Path<String>) -> Response {
    match deliver(&app, &id).await {
        Ok(()) => Json(json!({ "state": "delivered" })).into_response(),
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

#[cfg(test)]
pub(crate) mod tests {

    #[test]
    fn shares_and_roles_from_a_ruling() {
        let split = llm::Ruling { v: 1, id: "a".into(), outcome: "split".into(), split: Some(llm::Split { payee: "30.00".into(), payer: "20.00".into() }), fee: "escrow".into(), reasoning: "x".into(), late_party: "payee".into(), partial: true };
        assert!((share_for(&split, "payee") - 0.6).abs() < 1e-9);
        assert!((share_for(&split, "payer") - 0.4).abs() < 1e-9);
        let payee = llm::Ruling { outcome: "payee".into(), split: None, late_party: "none".into(), partial: false, ..split.clone() };
        assert_eq!(share_for(&payee, "payee"), 1.0);
        assert_eq!(share_for(&payee, "payer"), 0.0);
        let p = serde_json::json!({ "roles": { "payer": "a@x", "payee": "b@y" } });
        assert_eq!(party_role(&p, "a@x"), "payer");
        assert_eq!(party_role(&p, "b@y"), "payee");
        assert_eq!(party_role(&p, "c@z"), "party");
    }
    use super::*;
    use async_trait::async_trait;
    use browserid_core::device::{AccessCert, Binding, DeviceCert, Holder, HolderMatcher, Purpose, ScopeEntry, Warrant};
    use browserid_core::{Assertion, KeyPair, StatusRef};
    use chrono::Duration;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const ORIGIN: &str = "https://judge.test";
    const ISSUER: &str = "idp.test";

    pub struct FakeChain {
        pub objects: Mutex<HashMap<(String, String), chain::ObjectView>>,
        pub submitted: Mutex<Vec<Vec<u8>>>,
    }
    #[async_trait]
    impl ChainReader for FakeChain {
        async fn get_object(&self, path: &str, id: &str) -> anyhow::Result<Option<chain::ObjectView>> {
            Ok(self.objects.lock().unwrap().get(&(path.into(), id.into())).cloned())
        }
        async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<chain::ObjectView>> {
            Ok(self.objects.lock().unwrap().values().filter(|o| o.path.starts_with(prefix)).cloned().collect())
        }
        /// One past the newest object: "now" is just after the last write.
        async fn head_block(&self) -> anyhow::Result<u64> {
            Ok(self.objects.lock().unwrap().values().filter_map(|o| o.block).max().unwrap_or(0) + 1)
        }
    }
    #[async_trait]
    impl ChainWriter for FakeChain {
        async fn submit(&self, wire: Vec<u8>) -> anyhow::Result<String> {
            self.submitted.lock().unwrap().push(wire);
            Ok("fake".into())
        }
    }
    struct FakeRuler(Value);
    #[async_trait]
    impl Ruler for FakeRuler {
        async fn rule(&self, _case: &CaseFile) -> anyhow::Result<Value> {
            Ok(self.0.clone())
        }
    }

    pub fn login_presentation(issuer_kp: &KeyPair, email: &str) -> String {
        let access_kp = KeyPair::generate();
        let config_kp = KeyPair::generate();
        let access_cert = AccessCert::create(ISSUER, email, Holder::new("br.main").unwrap(), &access_kp.public_key(), Duration::hours(1), issuer_kp, None).unwrap();
        let config_cert = DeviceCert::create(ISSUER, &config_kp.public_key(), Purpose::Authorization, Holder::new("br.main").unwrap(), vec![email.into()], Duration::days(90), issuer_kp, None).unwrap();
        let warrant = Warrant::create_v2(
            email, email, Binding::Holder { matcher: HolderMatcher::new("br.*").unwrap() }, ORIGIN,
            vec![ScopeEntry::Bare("login".into())], Duration::days(1), &config_kp,
            StatusRef { uri: "https://idp.test/status".into(), idx: 1 },
        )
        .unwrap();
        let assertion = Assertion::create(ORIGIN, Duration::minutes(5), &access_kp).unwrap();
        format!("{}~{}~{}~{}", access_cert.encoded(), assertion.encoded(), warrant.encoded(), config_cert.encoded())
    }

    pub async fn app(issuer_kp: &KeyPair, chain: Arc<FakeChain>, ruler: Value) -> (App, seal::SealKey) {
        let dir = std::env::temp_dir().join(format!("judge-test-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config {
            port: 0, origin: ORIGIN.into(), data_dir: dir.to_string_lossy().into(), broker: "https://idp.test".into(),
            handle: "judge".into(), chain_audience: "sbo+raw://avail:turing:530/".into(), daemon: "http://x".into(),
            custodians: vec!["https://pay.test".into()], accepted_fallbacks: vec![ISSUER.into()],
            price_amount: "5.00".into(), price_currency: "USD".into(), anthropic_api_key: String::new(),
            anthropic_model: "m".into(), dev: true, trust_issuers: vec![(ISSUER.into(), issuer_kp.public_key().to_base64())],
            response_window_blocks: 180,
        };
        let seal = seal::SealKey::from_secret([3u8; 32]);
        let identity = Arc::new(Identity::new(dir.join("cred.json")));
        // Pretend provisioned as judge@dsp.fyi for the party/adjudicator checks.
        identity.state.lock().await.identity = Some("judge@dsp.fyi".into());
        let app = App {
            cfg: Arc::new(cfg.clone()), cases: Cases::open(":memory:").unwrap(), verify: Arc::new(verify::Verify::new(&cfg).unwrap()),
            seal: Arc::new(seal::SealKey::from_secret([3u8; 32])), identity, reader: chain.clone(), writer: chain, ruler: Arc::new(FakeRuler(ruler)),
            http: reqwest::Client::new(),
        };
        (app, seal)
    }

    pub fn terms() -> Value {
        json!({ "v": 1, "id": "agr_test", "salt": "s", "price": { "amount": "50.00", "currency": "USD" }, "obligations": [], "fees": { "custodian": "0.50", "adjudicator": "5.00" } })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dispute_is_accepted_only_from_a_party_with_matching_terms() {
        let issuer_kp = KeyPair::generate();
        let chain = Arc::new(FakeChain { objects: Mutex::new(HashMap::new()), submitted: Mutex::new(vec![]) });
        let t = terms();
        let proposal = json!({ "v": 1, "id": "agr_test", "mode": "commitment", "commitment": commitment(&t),
            "parties": { "proposer": "a@x.test", "acceptor": "b@y.test" }, "roles": { "payer": "a@x.test", "payee": "b@y.test" },
            "adjudicator": "judge@dsp.fyi", "custodian": "https://pay.test", "expires": 0 });
        chain.objects.lock().unwrap().insert(("/agreements/agr_test/".into(), "proposal".into()),
            chain::ObjectView { path: "/agreements/agr_test/".into(), id: "proposal".into(), creator: "a@x.test".into(), value: proposal, confirmed: true, block: Some(100) });
        chain.objects.lock().unwrap().insert(("/agreements/agr_test/".into(), "acceptance".into()),
            chain::ObjectView { path: "/agreements/agr_test/".into(), id: "acceptance".into(), creator: "b@y.test".into(), value: json!({ "v": 1 }), confirmed: true, block: Some(101) });
        let (app, seal) = app(&issuer_kp, chain.clone(), json!({ "outcome": "payer", "fee": "escrow", "reasoning": "nothing delivered" })).await;
        let sealed = seal::seal_to(&seal.public_b64(), &serde_json::to_vec(&t).unwrap()).unwrap();

        // A stranger is refused.
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "z@z.test"), contract_id: "agr_test".into(), sealed_terms: sealed.clone(), note: None })).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        // Wrong terms are refused.
        let bad = seal::seal_to(&seal.public_b64(), br#"{"id":"agr_test","price":{"amount":"1.00","currency":"USD"}}"#).unwrap();
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "a@x.test"), contract_id: "agr_test".into(), sealed_terms: bad, note: None })).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // A party with the right terms is accepted; the case opens with a
        // response window and NO ruling yet.
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "a@x.test"), contract_id: "agr_test".into(), sealed_terms: sealed.clone(), note: Some("late".into()) })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let c = app.cases.get("agr_test").unwrap().unwrap();
        assert_eq!(c.state, "open");
        assert!(c.ruling.is_none(), "no ruling before the response window");
        // The window is a height ahead of the chain head, not a wall-clock time.
        let head = app.reader.head_block().await.unwrap() as i64;
        assert!(c.respond_by_block > head, "window must be ahead of the head");
        assert_eq!(c.respond_by_block, head + app.cfg.response_window_blocks);
        // Filing twice conflicts.
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "b@y.test"), contract_id: "agr_test".into(), sealed_terms: sealed, note: None })).await;
        assert_eq!(r.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn commitment_is_key_order_independent() {
        let a = json!({ "b": 1, "a": [1, { "y": 2, "x": 1 }] });
        let b = json!({ "a": [1, { "x": 1, "y": 2 }], "b": 1 });
        assert_eq!(commitment(&a), commitment(&b));
        assert!(commitment(&a).starts_with("sha256:"));
    }
}

#[cfg(test)]
mod response_tests {
    use super::tests_support::*;
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn counterparty_reveals_then_the_judge_rules() {
        let (issuer_kp, chain, app, seal) = open_case(json!({ "outcome": "payee", "fee": "escrow", "reasoning": "opened content matches the terms" })).await;
        // Bob's delivery commitment on chain (content hashed like the SDK does).
        let content = json!({ "summaries": ["one", "two"] });
        chain.put("/agreements/agr_test/evidence/", "ev_1", "b@y.test", json!({ "v": 1, "id": "agr_test", "kind": "delivery", "by": "b@y.test", "commitment": content_commitment(&content) }));
        let _ = seal;
        // The disputer cannot respond; a stranger cannot respond.
        for who in ["a@x.test", "z@z.test"] {
            let r = api_respond(State(app.clone()), Path("agr_test".into()), Json(RespondBody { presentation: login_presentation(&issuer_kp, who), note: None, reveals: vec![] })).await;
            assert_eq!(r.status(), StatusCode::FORBIDDEN, "{who}");
        }
        // Bob reveals: matched, then the ruling follows.
        let r = api_respond(State(app.clone()), Path("agr_test".into()), Json(RespondBody { presentation: login_presentation(&issuer_kp, "b@y.test"), note: Some("all ten".into()), reveals: vec![Reveal { evidence_id: "ev_1".into(), content: content.clone() }] })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if app.cases.get("agr_test").unwrap().unwrap().ruling.is_some() { break; }
        }
        let c = app.cases.get("agr_test").unwrap().unwrap();
        assert_eq!(c.response.as_ref().unwrap()["reveals"][0]["matched"], true);
        assert_eq!(c.response.as_ref().unwrap()["reveals"][0]["content"], content);
        assert_eq!(c.ruling.as_ref().unwrap()["outcome"], "payee");
        // A second response is refused: the case is no longer open.
        let r = api_respond(State(app.clone()), Path("agr_test".into()), Json(RespondBody { presentation: login_presentation(&issuer_kp, "b@y.test"), note: None, reveals: vec![] })).await;
        assert_eq!(r.status(), StatusCode::CONFLICT);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_mismatching_reveal_is_recorded_as_such() {
        let (issuer_kp, chain, app, _seal) = open_case(json!({ "outcome": "payer", "fee": "escrow", "reasoning": "reveal did not match" })).await;
        chain.put("/agreements/agr_test/evidence/", "ev_1", "b@y.test", json!({ "kind": "delivery", "commitment": "sha256:deadbeef" }));
        let r = api_respond(State(app.clone()), Path("agr_test".into()), Json(RespondBody { presentation: login_presentation(&issuer_kp, "b@y.test"), note: None, reveals: vec![Reveal { evidence_id: "ev_1".into(), content: json!("other") }] })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        let c = app.cases.get("agr_test").unwrap().unwrap();
        assert_eq!(c.response.as_ref().unwrap()["reveals"][0]["matched"], false);
        assert!(c.response.as_ref().unwrap()["reveals"][0].get("content").is_none());
        // Someone else's evidence cannot be revealed.
        let (issuer_kp2, chain2, app2, _s2) = open_case(json!({ "outcome": "payer", "fee": "escrow", "reasoning": "x" })).await;
        chain2.put("/agreements/agr_test/evidence/", "ev_a", "a@x.test", json!({ "kind": "delivery", "commitment": "sha256:00" }));
        let r = api_respond(State(app2.clone()), Path("agr_test".into()), Json(RespondBody { presentation: login_presentation(&issuer_kp2, "b@y.test"), note: None, reveals: vec![Reveal { evidence_id: "ev_a".into(), content: json!("x") }] })).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_expired_window_is_ruled_by_the_sweep() {
        let (_issuer_kp, _chain, app, _seal) = open_case(json!({ "outcome": "payer", "fee": "escrow", "reasoning": "no response" })).await;
        // Nothing due yet.
        rule_due_cases(&app).await;
        assert!(app.cases.get("agr_test").unwrap().unwrap().ruling.is_none());
        // Put the window behind the chain head, sweep, ruled.
        app.cases.with_conn(|c| c.execute("UPDATE cases SET respond_by_block=?1 WHERE contract_id='agr_test'", rusqlite::params![0i64]).map(|_| ()));
        rule_due_cases(&app).await;
        let c = app.cases.get("agr_test").unwrap().unwrap();
        assert_eq!(c.ruling.as_ref().unwrap()["outcome"], "payer");
        assert!(c.response.is_none());
        // The party listing shows it.
        let r = api_cases_list(State(app.clone()), axum::extract::Query([("party".to_string(), "b@y.test".to_string())].into_iter().collect())).await;
        assert_eq!(r.status(), StatusCode::OK);
    }

    /// Evidence reaches the adjudicator in CHAIN order, carrying the block it
    /// landed in — the prompt has always told the model to read it that way and
    /// until now it was handed an unsorted list with no chain time at all
    /// (browserid-pay-e1ou). A pending write has no block: it has not landed, so
    /// it sorts last rather than being interleaved on a number it does not have.
    #[tokio::test]
    async fn evidence_reaches_the_judge_in_chain_order_with_blocks() {
        let (issuer_kp, chain, app, seal) = tests_support::open_case(json!({ "outcome": "payer", "fee": "escrow", "reasoning": "x" })).await;
        // Deliberately not in id order, so passing cannot be an accident of sorting by id.
        chain.put("/agreements/agr_test/evidence/", "ev_c", "b@y.test", json!({ "kind": "delivery", "at": 1 }));
        chain.put("/agreements/agr_test/evidence/", "ev_a", "a@x.test", json!({ "kind": "acknowledgement", "at": 2 }));
        chain.put("/agreements/agr_test/evidence/", "ev_b", "b@y.test", json!({ "kind": "delivery", "at": 3 }));
        // One still in the mempool.
        chain.objects.lock().unwrap().insert(
            ("/agreements/agr_test/evidence/".into(), "ev_pending".into()),
            chain::ObjectView { path: "/agreements/agr_test/evidence/".into(), id: "ev_pending".into(),
                creator: "b@y.test".into(), value: json!({ "kind": "delivery" }), confirmed: false, block: None });

        // Re-open so the case picks up the new evidence.
        app.cases.with_conn(|c| c.execute("DELETE FROM cases", []).map(|_| ()));
        let t = tests::terms();
        let sealed = seal::seal_to(&seal.public_b64(), &serde_json::to_vec(&t).unwrap()).unwrap();
        let r = api_dispute(State(app.clone()), Json(DisputeBody {
            presentation: tests_support::login_presentation(&issuer_kp, "a@x.test"),
            contract_id: "agr_test".into(), sealed_terms: sealed, note: None })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);

        let c = app.cases.get("agr_test").unwrap().unwrap();
        let ev = c.evidence.as_array().unwrap();
        let ids: Vec<&str> = ev.iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["ev_c", "ev_a", "ev_b", "ev_pending"], "chain order, pending last");

        let blocks: Vec<Option<u64>> = ev.iter().map(|e| e["block"].as_u64()).collect();
        assert!(blocks[..3].windows(2).all(|w| w[0] < w[1]), "confirmed blocks ascend: {blocks:?}");
        assert_eq!(blocks[3], None, "a pending write carries no block");
    }
}

/// Shared fixtures for the response tests (built on the same fakes as `tests`).
#[cfg(test)]
mod tests_support {
    use super::*;
    pub use super::tests::{login_presentation, FakeChain};
    use browserid_core::KeyPair;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    impl FakeChain {
        /// Each `put` lands in a later block than the one before it, so tests
        /// exercise real chain ordering rather than HashMap iteration order.
        pub fn put(&self, path: &str, id: &str, creator: &str, value: Value) {
            let mut objs = self.objects.lock().unwrap();
            let block = objs.values().filter_map(|o| o.block).max().unwrap_or(99) + 1;
            objs.insert((path.into(), id.into()), chain::ObjectView { path: path.into(), id: id.into(), creator: creator.into(), value, confirmed: true, block: Some(block) });
        }
    }

    /// A case for agr_test between a@x.test (payer, disputer) and b@y.test,
    /// opened through `api_dispute`, with the given canned ruling.
    pub async fn open_case(ruling: Value) -> (KeyPair, Arc<FakeChain>, App, seal::SealKey) {
        let issuer_kp = KeyPair::generate();
        let chain = Arc::new(FakeChain { objects: Mutex::new(HashMap::new()), submitted: Mutex::new(vec![]) });
        let t = super::tests::terms();
        let proposal = json!({ "v": 1, "id": "agr_test", "mode": "commitment", "commitment": commitment(&t),
            "parties": { "proposer": "a@x.test", "acceptor": "b@y.test" }, "roles": { "payer": "a@x.test", "payee": "b@y.test" },
            "adjudicator": "judge@dsp.fyi", "custodian": "https://pay.test", "expires": 0 });
        chain.put("/agreements/agr_test/", "proposal", "a@x.test", proposal);
        chain.put("/agreements/agr_test/", "acceptance", "b@y.test", json!({ "v": 1 }));
        let (app, seal) = super::tests::app(&issuer_kp, chain.clone(), ruling).await;
        let sealed = seal::seal_to(&seal.public_b64(), &serde_json::to_vec(&t).unwrap()).unwrap();
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "a@x.test"), contract_id: "agr_test".into(), sealed_terms: sealed, note: Some("half".into()) })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        (issuer_kp, chain, app, seal)
    }
}
