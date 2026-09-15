//! demo-idp: the identity provider the local demo runs against.
//!
//! One in-memory Ed25519 issuer key. It speaks the one IdP endpoint the
//! agents need at runtime (`POST /access/mint`, the same contract as the
//! broker) and a handful of `/demo/*` endpoints that stand in for the human
//! consent ceremonies: issue a device credential, sign a warrant, verify a
//! presentation. Everything it signs is accepted by the custodian and the
//! judge only because they run in dev mode with this issuer pinned.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}, Json, Router};
use browserid_core::device::{
    AccessCert, AccessRequest, Binding, DeviceCert, Holder, HolderMatcher, Purpose, ScopeEntry, Warrant,
};
use browserid_core::{KeyPair, StatusRef};
use browserid_rp::Verifier;
use chrono::Duration;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct Idp {
    domain: String,
    origin: String,
    key: KeyPair,
    /// One config key per grantor, so a grantor's warrants share a config cert.
    config_keys: Mutex<HashMap<String, KeyPair>>,
    status_idx: Mutex<u64>,
    holders: Mutex<u64>,
}

type App = Arc<Idp>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let port: u16 = std::env::var("DEMO_IDP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3900);
    let domain = format!("127.0.0.1:{port}");
    let idp = Arc::new(Idp {
        origin: format!("http://{domain}"),
        domain,
        key: KeyPair::generate(),
        config_keys: Mutex::new(HashMap::new()),
        status_idx: Mutex::new(1),
        holders: Mutex::new(1),
    });
    let router = Router::new()
        .route("/demo/issuer", get(issuer))
        .route("/demo/credential", post(credential))
        .route("/demo/grant", post(grant))
        .route("/demo/verify", post(verify))
        .route("/access/mint", post(access_mint))
        .route("/status", get(|| async { "" }))
        .with_state(idp.clone());
    tracing::info!(domain = %idp.domain, key = %idp.key.public_key().to_base64(), "demo-idp listening");
    axum::serve(tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?, router).await?;
    Ok(())
}

fn bad(msg: impl Into<String>) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg.into() }))).into_response()
}

async fn issuer(State(idp): State<App>) -> Json<Value> {
    Json(json!({ "domain": idp.domain, "origin": idp.origin, "public_key": idp.key.public_key().to_base64() }))
}

#[derive(Deserialize)]
struct CredentialReq {
    identity: String,
}

/// A device credential for `identity` in the shape `@browserid-ng/agent` and
/// `browserid-agent` load: seed + authentication-purpose device cert + idp.
async fn credential(State(idp): State<App>, Json(r): Json<CredentialReq>) -> axum::response::Response {
    let device_kp = KeyPair::generate();
    let n = { let mut h = idp.holders.lock().unwrap(); *h += 1; *h };
    let holder = Holder::new(format!("ag.demo{n}")).unwrap();
    let cert = match DeviceCert::create(
        &idp.domain, &device_kp.public_key(), Purpose::Authentication, holder.clone(),
        vec![r.identity.clone()], Duration::days(90), &idp.key, None,
    ) {
        Ok(c) => c,
        Err(e) => return bad(e.to_string()),
    };
    Json(json!({
        "device_key": base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, device_kp.secret_bytes()),
        "agent_device_cert": cert.encoded(),
        "idp": idp.origin,
        "identity": r.identity,
        "holder": holder,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct GrantReq {
    grantor: String,
    grantee: String,
    audience: String,
    scopes: Vec<ScopeEntry>,
    /// Holder matcher the grant binds to (the grantee's device holder, or a
    /// namespace such as `ag.*`). Default: any holder.
    #[serde(default)]
    holder: Option<String>,
}

/// Stand-in for the consent card: sign a v2 warrant as `grantor` with a
/// config cert this IdP issued for them. Returns the `warrant~config_cert`
/// pair the agent SDKs store.
async fn grant(State(idp): State<App>, Json(r): Json<GrantReq>) -> axum::response::Response {
    let config_kp = {
        let mut m = idp.config_keys.lock().unwrap();
        m.entry(r.grantor.clone()).or_insert_with(KeyPair::generate).clone()
    };
    let config_cert = match DeviceCert::create(
        &idp.domain, &config_kp.public_key(), Purpose::Authorization, Holder::new("br.demo").unwrap(),
        vec![r.grantor.clone()], Duration::days(90), &idp.key, None,
    ) {
        Ok(c) => c,
        Err(e) => return bad(e.to_string()),
    };
    let idx = { let mut i = idp.status_idx.lock().unwrap(); *i += 1; *i };
    let matcher = HolderMatcher::new(r.holder.clone().unwrap_or_else(|| "*".into())).unwrap();
    let warrant = match Warrant::create_v2(
        &r.grantor, &r.grantee, Binding::Holder { matcher }, &r.audience, r.scopes, Duration::days(30), &config_kp,
        StatusRef { uri: format!("{}/status", idp.origin), idx },
    ) {
        Ok(w) => w,
        Err(e) => return bad(e.to_string()),
    };
    Json(json!({
        "warrant": warrant.encoded(),
        "config_cert": config_cert.encoded(),
        "pair": format!("{}~{}", warrant.encoded(), config_cert.encoded()),
        "audience": r.audience,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct MintReq {
    device_cert: String,
    access_request: String,
}

/// `POST /access/mint` — the broker's contract, minus the registry: verify
/// the device cert is ours and the access request is signed by its key, then
/// certify the fresh access key for the requested identity.
async fn access_mint(State(idp): State<App>, Json(r): Json<MintReq>) -> axum::response::Response {
    let device_cert = match DeviceCert::parse(&r.device_cert) { Ok(c) => c, Err(e) => return bad(e.to_string()) };
    if let Err(e) = device_cert.verify(&idp.key.public_key()) { return bad(format!("device cert: {e}")); }
    if device_cert.purpose() != Purpose::Authentication { return bad("not an authentication cert"); }
    let areq = match AccessRequest::parse(&r.access_request) { Ok(a) => a, Err(e) => return bad(e.to_string()) };
    if let Err(e) = areq.verify(device_cert.public_key()) { return bad(format!("access request: {e}")); }
    let c = areq.claims();
    if !device_cert.authorizes_identity(&c.identity) { return bad("device cert not authorized for this identity"); }
    let cert = match AccessCert::create(&idp.domain, &c.identity, c.holder.clone(), &c.access_key, Duration::hours(24), &idp.key, None) {
        Ok(c) => c,
        Err(e) => return bad(e.to_string()),
    };
    Json(json!({ "success": true, "access_cert": cert.encoded(), "email": c.identity })).into_response()
}

#[derive(Deserialize)]
struct VerifyReq {
    presentation: String,
    audience: String,
}

/// What the hosted verifier would answer, for bundles this IdP issued.
async fn verify(State(idp): State<App>, Json(r): Json<VerifyReq>) -> Json<Value> {
    let v = Verifier::new(r.audience).without_status_checks().trust_issuer(idp.domain.clone(), idp.key.public_key());
    match v.verify(&r.presentation) {
        Ok(id) => {
            // Scope entries with parameters live on the warrant itself.
            let scopes: Value = browserid_core::device::AccessPresentation::parse(&r.presentation)
                .map(|p| serde_json::to_value(&p.warrant.claims().scopes).unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            Json(json!({ "ok": true, "email": id.email, "grantee": id.grantee, "issuer": id.issuer, "scopes": scopes }))
        }
        Err(e) => Json(json!({ "ok": false, "reason": e.to_string() })),
    }
}
