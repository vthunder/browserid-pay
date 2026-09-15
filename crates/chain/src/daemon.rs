//! Reading the SBO database and submitting signed wire batches, both through
//! an sbo-daemon's `/v1/*` (always with `repo=`): the daemon holds the repo's
//! TurboDA credentials and refreshes issuer DNSSEC evidence ahead of any write
//! that needs it. Both behind traits so tests can fake them.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::Value;

/// A confirmed-or-pending object as the daemon serves it: we keep the
/// attributed author (`creator`), the payload (`value`) and confirmation.
#[derive(Debug, Clone)]
pub struct ObjectView {
    pub path: String,
    pub id: String,
    pub creator: String,
    pub value: Value,
    pub confirmed: bool,
}

#[async_trait]
pub trait ChainReader: Send + Sync {
    async fn get_object(&self, path: &str, id: &str) -> Result<Option<ObjectView>>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectView>>;
}

#[async_trait]
pub trait ChainWriter: Send + Sync {
    /// Submit one wire batch; returns a submission id.
    async fn submit(&self, wire: Vec<u8>) -> Result<String>;
}

pub struct DaemonReader {
    pub daemon: String,
    pub repo: String,
    http: reqwest::Client,
}

impl DaemonReader {
    pub fn new(daemon: &str, repo: &str) -> Self {
        Self {
            daemon: daemon.trim_end_matches('/').to_string(),
            repo: repo.to_string(),
            http: reqwest::Client::builder().no_proxy().build().expect("http client"),
        }
    }
    fn view(v: &Value) -> Option<ObjectView> {
        Some(ObjectView {
            path: v["path"].as_str()?.to_string(),
            id: v["id"].as_str()?.to_string(),
            creator: v["creator"].as_str().unwrap_or("").to_string(),
            value: v.get("value").cloned().unwrap_or(Value::Null),
            confirmed: v["confirmed"].as_bool().unwrap_or(false),
        })
    }
}

#[async_trait]
impl ChainReader for DaemonReader {
    async fn get_object(&self, path: &str, id: &str) -> Result<Option<ObjectView>> {
        let resp = self
            .http
            .get(format!("{}/v1/object", self.daemon))
            .query(&[("path", path), ("id", id), ("repo", &self.repo)])
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let v: Value = resp.error_for_status()?.json().await?;
        Ok(Self::view(&v))
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectView>> {
        let v: Value = self
            .http
            .get(format!("{}/v1/list", self.daemon))
            .query(&[("prefix", prefix), ("repo", &self.repo)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let arr = v.as_array().or_else(|| v["objects"].as_array()).cloned().unwrap_or_default();
        Ok(arr.iter().filter_map(Self::view).collect())
    }
}

/// Submit through a daemon: `POST <daemon>/v1/submit?repo=<repo>` with the
/// raw wire. Returns the daemon's `submission_id`.
pub struct DaemonWriter {
    url: String,
    http: reqwest::Client,
}

impl DaemonWriter {
    pub fn new(daemon: &str, repo: &str) -> Self {
        Self::at_url(&format!("{}/v1/submit?repo={}", daemon.trim_end_matches('/'), crate::encode_repo(repo)))
    }
    /// An explicit submit URL (dev/demo stub daemons).
    pub fn at_url(url: &str) -> Self {
        Self { url: url.to_string(), http: reqwest::Client::builder().no_proxy().build().expect("http client") }
    }
}

#[async_trait]
impl ChainWriter for DaemonWriter {
    async fn submit(&self, wire: Vec<u8>) -> Result<String> {
        let resp = self.http.post(&self.url).header("Content-Type", "application/octet-stream").body(wire).send().await?;
        if !resp.status().is_success() {
            bail!("submit to {} failed ({}): {}", self.url, resp.status(), resp.text().await.unwrap_or_default());
        }
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if let Some(refreshed) = v["evidence_refreshed"].as_array().filter(|a| !a.is_empty()) {
            tracing::info!(?refreshed, "daemon refreshed issuer evidence ahead of the write");
        }
        v["submission_id"].as_str().map(String::from).or_else(|| v["hash"].as_str().map(String::from)).context("daemon submit: no submission_id")
    }
}

/// Dev/test writer: logs and drops.
pub struct LogWriter;
#[async_trait]
impl ChainWriter for LogWriter {
    async fn submit(&self, wire: Vec<u8>) -> Result<String> {
        tracing::warn!(bytes = wire.len(), "no chain writer configured: write dropped");
        Ok("dropped".into())
    }
}
