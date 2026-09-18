//! browserid-pay indexer (index.dsp.fyi).
//!
//! An acceleration, never an authority (SBO Indexer spec): it polls the
//! daemon's confirmed reads, keeps a reverse index by identity, and answers
//! with results *plus the references* a client can re-fetch from the daemon
//! and verify itself.

mod chain;
mod index;
mod reputation;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chain::{Chain, Daemon, StateRoot};
use index::Index;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
struct App {
    index: Index,
    daemon: String,
    repo: String,
    origin: String,
    signer: Arc<reputation::Signer>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let port: u16 = env("INDEX_PORT", "3400").parse()?;
    let daemon = env("DAEMON", "https://da.sandmill.org");
    let repo = env("REPO", "sbo+raw://avail:turing:530/");
    let origin = env("INDEX_ORIGIN", &format!("http://localhost:{port}"));
    let interval: u64 = env("POLL_SECS", "30").parse()?;
    let db_path = env("DATABASE_PATH", "indexer.db");
    let index = Index::open(&db_path)?;
    // The indexer's signing key (scorecards are signed so a consumer can hold
    // this indexer to its published view). Lives next to the database.
    let data_dir = std::env::var("INDEX_DATA_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::Path::new(&db_path).parent().map(|p| p.to_path_buf()).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::PathBuf::from("."))
    });
    let signer = Arc::new(reputation::Signer::load_or_create(&data_dir.join("indexer-key.json"))?);
    tracing::info!(key = %signer.public_key_b64(), "scorecard signing key");

    let chain = Arc::new(Daemon::new(&daemon, &repo));
    tokio::spawn(poll_loop(chain, index.clone(), interval));

    let app = App { index, daemon: daemon.clone(), repo: repo.clone(), origin: origin.clone(), signer };
    let router = Router::new()
        .route("/", get(page))
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/agreements", get(agreements))
        .route("/v1/agreements/:id", get(agreement))
        .route("/v1/attestations", get(attestations))
        .route("/v1/providers/:kind", get(providers))
        .route("/v1/contribution", get(contribution))
        .route("/v1/reputation", get(reputation))
        .route("/v1/reputation/key", get(reputation_key))
        .with_state(app)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!(%origin, %daemon, %repo, %addr, "indexer listening");
    axum::serve(tokio::net::TcpListener::bind(&addr).await?, router).await?;
    Ok(())
}

/// One full rebuild from the daemon's confirmed reads. Returns the state
/// root the reads were taken against (read first, so results are never
/// newer than the root we advertise).
pub async fn poll_once(chain: &dyn Chain, index: &Index) -> anyhow::Result<StateRoot> {
    let root = chain.state_root().await?;
    let agreements = chain.list_prefix("/agreements/").await?;
    let providers = chain.list_prefix("/providers/").await?;
    let attestations = chain.list_schema("attestation.v1").await?;
    let sys = chain.list_prefix("/sys/").await?;
    let contribution = sys.iter().find(|o| o.path == "/sys/" && o.id == "contribution");
    index.rebuild(&agreements, &attestations, &providers, contribution, &root)?;
    tracing::info!(block = root.block, agreements = agreements.len(), attestations = attestations.len(), providers = providers.len(), "indexed");
    Ok(root)
}

async fn poll_loop(chain: Arc<Daemon>, index: Index, interval: u64) {
    loop {
        match poll_once(chain.as_ref(), &index).await {
            Ok(_) => {}
            Err(e) if chain::is_unknown_repo(&e) => tracing::warn!("daemon does not follow {} yet; retrying", chain.repo),
            Err(e) => tracing::warn!("poll failed: {e:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
    }
}

fn envelope(app: &App, results: Value, references: Value) -> Value {
    json!({
        "results": results,
        "references": references,
        "verify": { "daemon": app.daemon, "repo": app.repo, "object": format!("{}/v1/object?repo={}&path=<path>&id=<id>&proof=1", app.daemon, app.repo) },
        "state_root": app.index.meta("state_root").ok().flatten(),
        "indexed_at": app.index.meta("indexed_at").ok().flatten(),
        "indexer": app.origin,
    })
}

fn internal(e: anyhow::Error) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response()
}

#[derive(Deserialize)]
struct PartyQ {
    party: Option<String>,
}

async fn agreements(State(app): State<App>, Query(q): Query<PartyQ>) -> Response {
    let Some(party) = q.party.filter(|p| !p.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "party (an identity) is required" }))).into_response();
    };
    match app.index.agreements_by_party(&party) {
        Ok(v) => {
            let refs: Vec<_> = v.iter().flat_map(|a| a.references.clone()).collect();
            Json(envelope(&app, json!(v), json!(refs))).into_response()
        }
        Err(e) => internal(e),
    }
}

async fn agreement(State(app): State<App>, Path(id): Path<String>) -> Response {
    match app.index.agreement(&id) {
        Ok(Some(a)) => {
            let refs = json!(a.references);
            Json(envelope(&app, json!(a), refs)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct SubjectQ {
    subject: Option<String>,
    #[serde(rename = "type")]
    typ: Option<String>,
}

async fn attestations(State(app): State<App>, Query(q): Query<SubjectQ>) -> Response {
    let Some(subject) = q.subject.filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "subject (an identity) is required" }))).into_response();
    };
    match app.index.attestations_by_subject(&subject, q.typ.as_deref()) {
        Ok(v) => {
            let refs: Vec<_> = v.iter().map(|a| a.reference.clone()).collect();
            Json(envelope(&app, json!(v), json!(refs))).into_response()
        }
        Err(e) => internal(e),
    }
}

async fn providers(State(app): State<App>, Path(kind): Path<String>) -> Response {
    if kind != "custodians" && kind != "adjudicators" {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "kind must be custodians or adjudicators" }))).into_response();
    }
    match app.index.providers(&kind) {
        Ok(v) => {
            let refs: Vec<_> = v.iter().map(|p| p["reference"].clone()).collect();
            Json(envelope(&app, json!(v), json!(refs))).into_response()
        }
        Err(e) => internal(e),
    }
}

async fn contribution(State(app): State<App>) -> Response {
    match app.index.meta("contribution") {
        Ok(Some(c)) if !c.is_null() => Json(envelope(&app, c["value"].clone(), json!([c["reference"]]))).into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not_indexed_yet" }))).into_response(),
        Err(e) => internal(e),
    }
}

/// The scorecard (`browserid-pay-reputation-v1`): one indexer's signed view
/// of a subject — an agent identity, or a controlling account (its agents'
/// agreements pooled). See reputation.rs and the README.
async fn reputation(State(app): State<App>, Query(q): Query<SubjectQ>) -> Response {
    let Some(subject) = q.subject.filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "subject (an identity) is required" }))).into_response();
    };
    let root = app.index.meta("state_root").ok().flatten();
    let block = root.as_ref().and_then(|r| r["block"].as_u64()).unwrap_or(0);
    let mut card = match reputation::scorecard(&app.index, &subject, block) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    if let Ok(counts) = app.index.counts(&subject) {
        card["counts"]["attestations"] = counts["attestations"].clone();
    }
    card["indexed_at"] = app.index.meta("indexed_at").ok().flatten().unwrap_or(Value::Null);
    card["state_root"] = root.unwrap_or(Value::Null);
    card["indexer"] = json!(app.origin);
    card["key"] = json!({ "alg": "Ed25519", "public_key": app.signer.public_key_b64(), "url": format!("{}/v1/reputation/key", app.origin) });
    let sig = app.signer.sign(&card);
    card["signature"] = json!(sig);
    Json(card).into_response()
}

async fn reputation_key(State(app): State<App>) -> Response {
    Json(json!({ "alg": "Ed25519", "public_key": app.signer.public_key_b64(), "formula": reputation::FORMULA, "indexer": app.origin })).into_response()
}

async fn page(State(app): State<App>) -> Html<String> {
    Html(
        include_str!("../static/index.html")
            .replace("{{DAEMON}}", &app.daemon)
            .replace("{{REPO}}", &app.repo)
            .replace("{{ORIGIN}}", &app.origin),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chain::Obj;

    struct Fake(Vec<Obj>);
    #[async_trait::async_trait]
    impl Chain for Fake {
        async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<Obj>> {
            Ok(self.0.iter().filter(|o| o.path.starts_with(prefix)).cloned().collect())
        }
        async fn list_schema(&self, schema: &str) -> anyhow::Result<Vec<Obj>> {
            Ok(self.0.iter().filter(|o| o.content_schema.as_deref() == Some(schema)).cloned().collect())
        }
        async fn state_root(&self) -> anyhow::Result<StateRoot> {
            Ok(StateRoot { block: 42, state_root: "root".into() })
        }
    }

    fn obj(path: &str, id: &str, block: u64, schema: Option<&str>, v: Value) -> Obj {
        Obj { path: path.into(), id: id.into(), creator: "e".into(), content_schema: schema.map(String::from), block, object_hash: format!("h{block}"), value: Some(v), payload_text: String::new(), confirmed: true }
    }

    #[tokio::test]
    async fn poll_once_builds_the_index_from_the_daemon_reads() {
        let chain = Fake(vec![
            obj("/sys/", "contribution", 1, Some("contribution.v1"), json!({ "recipient": "fund@dsp.fyi", "suggested_percentage": 1.0 })),
            obj("/providers/custodians/escrow@dsp.fyi/", "listing", 2, Some("provider.v1"), json!({ "identity": "escrow@dsp.fyi" })),
            obj("/agreements/agr_1/", "proposal", 3, Some("agreement.proposal.v1"), json!({ "id": "agr_1", "parties": { "proposer": "a@x", "acceptor": "b@y" }, "roles": { "payer": "a@x", "payee": "b@y" }, "adjudicator": "judge@dsp.fyi", "custodian": "https://pay.dsp.fyi" })),
            obj("/u/b@y/attestations/a@x/", "agreement:completed", 4, Some("attestation.v1"), json!({ "subject": "a@x", "type": "agreement:completed", "value": { "contract": "agr_1" } })),
        ]);
        let index = Index::open(":memory:").unwrap();
        let root = poll_once(&chain, &index).await.unwrap();
        assert_eq!(root.block, 42);
        assert_eq!(index.agreements_by_party("a@x").unwrap()[0].state, "proposed");
        assert_eq!(index.attestations_by_subject("a@x", None).unwrap().len(), 1);
        assert_eq!(index.providers("custodians").unwrap()[0]["name"], "escrow@dsp.fyi");
        assert_eq!(index.meta("state_root").unwrap().unwrap()["block"], 42);
    }
}
