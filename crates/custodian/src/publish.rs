//! The custodian on the chain: its own delegated identity (provisioned once,
//! like the judge), its provider listing, and the escrow `Publisher` that
//! posts lock objects and settlement attestations through the daemon.

use crate::config::Config;
use crate::escrow::Publisher;
use chain::{ChainWriter, Identity, ProvisionSpec, WriteSpec};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct ChainPublisher {
    pub identity: Arc<Identity>,
    pub writer: Arc<dyn ChainWriter>,
    pub chain_audience: String,
}

impl ChainPublisher {
    async fn post(&self, spec: WriteSpec, what: &str) {
        let wire = match self.identity.sign_write(&self.chain_audience, &spec).await {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("{what}: not posted (identity not provisioned?): {e:#}");
                return;
            }
        };
        match self.writer.submit(wire).await {
            Ok(sub) => tracing::info!(%sub, "{what} posted"),
            Err(e) => tracing::error!("{what}: submit failed: {e:#}"),
        }
    }

    async fn attest(&self, subject: &str, kind: &str, value: Value) {
        let Some(me) = self.identity.email().await else { return };
        let payload = json!({
            "subject": subject, "type": kind, "value": value,
            "issued_at": chrono::Utc::now().timestamp(), "expires": null, "issuer": me,
        });
        self.post(
            WriteSpec {
                path: format!("/{me}/attestations/{subject}/"),
                id: kind.into(),
                schema: "attestation.v1".into(),
                payload: serde_json::to_vec(&payload).expect("attestation json"),
            },
            &format!("attestation {kind} on {subject}"),
        )
        .await;
    }
}

#[async_trait::async_trait]
impl Publisher for ChainPublisher {
    async fn lock(&self, contract_id: &str, locked_at: i64) {
        let payload = json!({ "v": 1, "id": contract_id, "locked_at": locked_at });
        self.post(
            WriteSpec {
                path: format!("/agreements/{contract_id}/"),
                id: "lock".into(),
                schema: "agreement.lock.v1".into(),
                payload: serde_json::to_vec(&payload).expect("lock json"),
            },
            &format!("escrow lock for {contract_id}"),
        )
        .await;
    }

    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str) {
        // A ruling-driven settlement is a dispute in the reputation record.
        let kind = if outcome.starts_with("ruled:") { "agreement:disputed" } else { "agreement:settled" };
        for p in [parties.0, parties.1] {
            self.attest(p, kind, json!({ "contract": contract_id, "outcome": outcome })).await;
        }
    }
}

/// What the custodian asks the approver for: its own delegated identity
/// (handle `escrow`) and a chain warrant for lock objects, its listing and
/// settlement attestations.
pub fn provision_spec(cfg: &Config) -> ProvisionSpec {
    ProvisionSpec {
        handle: cfg.handle.clone(),
        label: "browserid-pay custodian".into(),
        message: "The escrow custodian for browserid-pay agreements: it posts escrow locks and settlement attestations to the dsp.fyi database. Name it (the handle it asks for is `escrow`) and pick which of your identities delegates; it acts as its own identity, attributed to you.".into(),
        grants: vec![browserid_agent::GrantRequest {
            audience: cfg.chain_audience.clone(),
            scopes: vec![
                "action:post".into(),
                "path:/agreements/**".into(),
                "path:/providers/custodians/**".into(),
                "path:/*/attestations/**".into(),
            ],
        }],
    }
}

pub fn listing(cfg: &Config, me: &str) -> Value {
    json!({
        "v": 1,
        "identity": me,
        "origin": cfg.origin,
        "price": { "amount": cfg.fee, "currency": cfg.currency },
        "conformance": "browserid-pay-custodian-v1",
    })
}
