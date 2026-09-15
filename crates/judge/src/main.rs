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

    let router = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/setup", get(api_setup))
        .route("/api/dispute", post(api_dispute))
        .route("/api/cases/:id", get(api_case))
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
            "path:/*/attestations/**".into(),
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
/// then rule in the background.
async fn api_dispute(State(app): State<App>, Json(body): Json<DisputeBody>) -> Response {
    let who = match app.verify.presentation(&body.presentation).await {
        Ok(v) => v.email,
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
    let evidence: Vec<Value> = match app.reader.list_prefix(&format!("{path}evidence/")).await {
        Ok(v) => v
            .into_iter()
            .filter(|o| parties.contains(&o.creator))
            .map(|o| json!({ "id": o.id, "by": o.creator, "confirmed": o.confirmed, "object": o.value }))
            .collect(),
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("chain read: {e}")),
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
    };
    if let Err(e) = app.cases.insert(&case) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let bg = app.clone();
    let cid = id.clone();
    tokio::spawn(async move {
        if let Err(e) = rule_and_deliver(&bg, &cid).await {
            tracing::error!(contract = %cid, "ruling failed: {e:#}");
            let _ = bg.cases.update(&cid, "failed", None, None, Some(&e.to_string()));
        }
    });
    (StatusCode::ACCEPTED, Json(json!({ "contract_id": id, "state": "open" }))).into_response()
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

async fn rule_and_deliver(app: &App, id: &str) -> anyhow::Result<()> {
    let case = app.cases.get(id)?.ok_or_else(|| anyhow::anyhow!("case vanished"))?;
    let file = CaseFile {
        contract_id: id.to_string(),
        terms: case.terms.clone(),
        proposal: case.proposal.clone(),
        evidence: case.evidence.as_array().cloned().unwrap_or_default(),
        dispute_note: case.dispute_note.clone(),
        disputed_by: case.disputed_by.clone(),
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

    // 2. Attestations on both parties.
    if let Some(me) = app.identity.email().await {
        for p in &case.parties {
            let payload = json!({ "subject": p, "type": "agreement:ruled", "value": { "contract": id, "outcome": ruling.outcome }, "issued_at": cases::now(), "expires": null, "issuer": me });
            let w = app
                .identity
                .sign_write(
                    &app.cfg.chain_audience,
                    &WriteSpec { path: format!("/{me}/attestations/{p}/"), id: "agreement:ruled".into(), schema: "attestation.v1".into(), payload: serde_json::to_vec(&payload)? },
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
            "contract_id": c.contract_id, "state": c.state, "disputed_by": c.disputed_by,
            "custodian": c.custodian, "parties": c.parties, "ruling": c.ruling,
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
mod tests {
    use super::*;
    use async_trait::async_trait;
    use browserid_core::device::{AccessCert, Binding, DeviceCert, Holder, HolderMatcher, Purpose, ScopeEntry, Warrant};
    use browserid_core::{Assertion, KeyPair, StatusRef};
    use chrono::Duration;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const ORIGIN: &str = "https://judge.test";
    const ISSUER: &str = "idp.test";

    struct FakeChain {
        objects: Mutex<HashMap<(String, String), chain::ObjectView>>,
        submitted: Mutex<Vec<Vec<u8>>>,
    }
    #[async_trait]
    impl ChainReader for FakeChain {
        async fn get_object(&self, path: &str, id: &str) -> anyhow::Result<Option<chain::ObjectView>> {
            Ok(self.objects.lock().unwrap().get(&(path.into(), id.into())).cloned())
        }
        async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<chain::ObjectView>> {
            Ok(self.objects.lock().unwrap().values().filter(|o| o.path.starts_with(prefix)).cloned().collect())
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

    fn login_presentation(issuer_kp: &KeyPair, email: &str) -> String {
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

    async fn app(issuer_kp: &KeyPair, chain: Arc<FakeChain>, ruler: Value) -> (App, seal::SealKey) {
        let dir = std::env::temp_dir().join(format!("judge-test-{}-{}", std::process::id(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config {
            port: 0, origin: ORIGIN.into(), data_dir: dir.to_string_lossy().into(), broker: "https://idp.test".into(),
            handle: "judge".into(), chain_audience: "sbo+raw://avail:turing:530/".into(), daemon: "http://x".into(),
            custodians: vec!["https://pay.test".into()], accepted_fallbacks: vec![ISSUER.into()],
            price_amount: "5.00".into(), price_currency: "USD".into(), anthropic_api_key: String::new(),
            anthropic_model: "m".into(), dev: true, trust_issuers: vec![(ISSUER.into(), issuer_kp.public_key().to_base64())],
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

    fn terms() -> Value {
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
            chain::ObjectView { path: "/agreements/agr_test/".into(), id: "proposal".into(), creator: "a@x.test".into(), value: proposal, confirmed: true });
        chain.objects.lock().unwrap().insert(("/agreements/agr_test/".into(), "acceptance".into()),
            chain::ObjectView { path: "/agreements/agr_test/".into(), id: "acceptance".into(), creator: "b@y.test".into(), value: json!({ "v": 1 }), confirmed: true });
        let (app, seal) = app(&issuer_kp, chain.clone(), json!({ "outcome": "payer", "fee": "escrow", "reasoning": "nothing delivered" })).await;
        let sealed = seal::seal_to(&seal.public_b64(), &serde_json::to_vec(&t).unwrap()).unwrap();

        // A stranger is refused.
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "z@z.test"), contract_id: "agr_test".into(), sealed_terms: sealed.clone(), note: None })).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        // Wrong terms are refused.
        let bad = seal::seal_to(&seal.public_b64(), br#"{"id":"agr_test","price":{"amount":"1.00","currency":"USD"}}"#).unwrap();
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "a@x.test"), contract_id: "agr_test".into(), sealed_terms: bad, note: None })).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // A party with the right terms is accepted; the ruling is recorded
        // (delivery fails: no identity to present, so the case ends "ruled"
        // with an error, never "delivered").
        let r = api_dispute(State(app.clone()), Json(DisputeBody { presentation: login_presentation(&issuer_kp, "a@x.test"), contract_id: "agr_test".into(), sealed_terms: sealed.clone(), note: Some("late".into()) })).await;
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if app.cases.get("agr_test").unwrap().unwrap().ruling.is_some() { break; }
        }
        let c = app.cases.get("agr_test").unwrap().unwrap();
        assert_eq!(c.ruling.as_ref().unwrap()["outcome"], "payer");
        assert_ne!(c.state, "delivered");
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
