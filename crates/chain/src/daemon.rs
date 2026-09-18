//! Reading the SBO database and submitting signed wire batches, both through
//! an sbo-daemon's `/v1/*` (always with `repo=`): the daemon holds the repo's
//! TurboDA credentials and refreshes issuer DNSSEC evidence ahead of any write
//! that needs it. Both behind traits so tests can fake them.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::Value;

/// A confirmed-or-pending object as the daemon serves it: we keep the
/// attributed author (`creator`), the payload (`value`), confirmation, and the
/// DA block it landed in.
#[derive(Debug, Clone)]
pub struct ObjectView {
    pub path: String,
    pub id: String,
    pub creator: String,
    pub value: Value,
    pub confirmed: bool,
    /// The Avail block this object was included in, `None` while it is still
    /// pending. This is the only time in the system no party controls: heights
    /// are assigned by the chain, the daemon processes only FINALIZED blocks
    /// (`sbo-daemon` `lc.rs: subscribe_blocks`), and the number is the Avail
    /// block number — global across apps, since one block is read for several
    /// app_ids at once (`sync.rs: fetch_block_data_multi`).
    ///
    /// Prefer this over any timestamp in a payload: a payload time is the
    /// writer's claim about itself and nothing validates it
    /// (`browserid-pay-e1ou`).
    pub block: Option<u64>,
}

#[async_trait]
pub trait ChainReader: Send + Sync {
    async fn get_object(&self, path: &str, id: &str) -> Result<Option<ObjectView>>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectView>>;
    /// The chain's current head: the newest finalized block the daemon has
    /// processed. This is "now" for anything measured in block heights.
    ///
    /// Deliberately fallible and deliberately NOT defaulted: a caller that
    /// cannot read the head must fail closed rather than guess a deadline.
    async fn head_block(&self) -> Result<u64>;
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
            block: v["block"].as_u64(),
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

    async fn head_block(&self) -> Result<u64> {
        let v: Value = self
            .http
            .get(format!("{}/v1/state-root", self.daemon))
            .query(&[("repo", &self.repo)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        v["block"].as_u64().context("state-root: no block")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A confirmed object carries its block. This is the only time in the
    /// system no party controls, so it is what deadlines are measured against
    /// (`browserid-pay-e1ou`) — never a timestamp in the payload.
    #[test]
    fn view_reads_the_block() {
        let v = json!({ "path": "/agreements/a/", "id": "proposal", "creator": "a@x.test",
                        "value": { "v": 1 }, "confirmed": true, "block": 3_885_219u64 });
        let o = DaemonReader::view(&v).expect("view");
        assert_eq!(o.block, Some(3_885_219));
        assert!(o.confirmed);
    }

    /// A pending object has no block yet: it has not landed, so it has no
    /// place in chain order and cannot have met a deadline.
    #[test]
    fn pending_objects_have_no_block() {
        let v = json!({ "path": "/agreements/a/", "id": "delivery", "creator": "b@y.test",
                        "value": { "v": 1 }, "confirmed": false });
        let o = DaemonReader::view(&v).expect("view");
        assert_eq!(o.block, None);
        assert!(!o.confirmed);
    }

    /// A credential issued for one ledger must not be treated as usable on
    /// another. The two failure modes are very different: an unwarranted
    /// audience surfaces as "no matching grant" at write time, long after
    /// startup reported healthy, which is how a repoint turns into a silent
    /// outage (browserid-pay-4v59).
    #[tokio::test]
    async fn a_credential_knows_which_ledgers_it_is_warranted_for() {
        let id = crate::identity::Identity::new(std::path::PathBuf::from("/nonexistent/identity.json"));
        // Nothing loaded: no audience is warranted, so a caller must provision.
        assert!(!id.has_audience("sbo+raw://avail:turing:506/").await);
        id.state.lock().await.audiences = vec!["sbo+raw://avail:turing:530/".into()];
        assert!(id.has_audience("sbo+raw://avail:turing:530/").await);
        assert!(
            !id.has_audience("sbo+raw://avail:turing:506/").await,
            "a 530 credential must not pass as a 506 one"
        );
    }

    /// A daemon that returns a block we cannot parse must not silently become
    /// "pending" — but it also must not panic. `None` is the safe reading, and
    /// callers fail closed on it.
    #[test]
    fn a_malformed_block_reads_as_absent() {
        let v = json!({ "path": "/x/", "id": "y", "creator": "", "value": null,
                        "confirmed": true, "block": "not-a-number" });
        assert_eq!(DaemonReader::view(&v).expect("view").block, None);
    }
}
