//! Reads from an SBO daemon (`/v1/list`, `/v1/object`, `/v1/state-root`).
//! The indexer is an acceleration, never an authority: everything it serves
//! is a reference back to these reads.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One confirmed object as the daemon serves it (the subset we keep).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obj {
    pub path: String,
    pub id: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub content_schema: Option<String>,
    pub block: u64,
    pub object_hash: String,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub payload_text: String,
    #[serde(default = "yes")]
    pub confirmed: bool,
    /// The object's `Related` header (Wire Format Spec §Related Objects), if it
    /// declared one. This is what makes the reverse index possible without
    /// knowing any application schema: a post referencing an agreement looks
    /// the same here as anything else referencing anything else.
    #[serde(default)]
    pub related: Vec<Related>,
}

/// One `{rel, ref}` from an object's `Related` header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Related {
    pub rel: String,
    #[serde(rename = "ref")]
    pub reference: String,
}
fn yes() -> bool {
    true
}

impl Obj {
    /// The JSON payload: the daemon's parsed `value` when present, else the
    /// payload text parsed as JSON, else `null`.
    pub fn json(&self) -> Value {
        if let Some(v) = &self.value {
            return v.clone();
        }
        serde_json::from_str(&self.payload_text).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateRoot {
    pub block: u64,
    pub state_root: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiError {
    error: String,
}

/// What the poller needs from a chain. `Daemon` is the real one; tests use
/// an in-memory fake.
#[async_trait]
pub trait Chain: Send + Sync {
    /// Confirmed objects under a path prefix.
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<Obj>>;
    /// Confirmed objects with a content schema.
    async fn list_schema(&self, schema: &str) -> Result<Vec<Obj>>;
    async fn state_root(&self) -> Result<StateRoot>;
}

pub struct Daemon {
    pub base: String,
    pub repo: String,
    http: reqwest::Client,
}

impl Daemon {
    pub fn new(base: &str, repo: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            repo: repo.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("http client"),
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str, params: &[(&str, &str)]) -> Result<T> {
        let mut q: Vec<(&str, &str)> = vec![("repo", &self.repo)];
        q.extend_from_slice(params);
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(&q)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            let msg = serde_json::from_str::<ApiError>(&body).map(|e| e.error).unwrap_or(body);
            anyhow::bail!("{path}: {status}: {msg}");
        }
        serde_json::from_str(&body).with_context(|| format!("{path}: decode"))
    }
}

#[async_trait]
impl Chain for Daemon {
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<Obj>> {
        self.get("/v1/list", &[("prefix", prefix)]).await
    }
    async fn list_schema(&self, schema: &str) -> Result<Vec<Obj>> {
        self.get("/v1/list", &[("schema", schema)]).await
    }
    async fn state_root(&self) -> Result<StateRoot> {
        self.get("/v1/state-root", &[]).await
    }
}

/// Is this the daemon telling us it does not (yet) follow our repo? The
/// poller treats it as "not ready", not as a failure.
pub fn is_unknown_repo(e: &anyhow::Error) -> bool {
    e.to_string().contains("no followed repo")
}
