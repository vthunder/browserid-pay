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
    async fn lock(&self, contract_id: &str, locked_at: i64, accounts: (&str, &str)) {
        // The controlling accounts behind the two agent identities. The
        // custodian verified both — the payer's from a live presentation, the
        // payee's from the grant they registered — so this is an attested
        // statement, not a self-declaration. It replaces the presentation that
        // used to be embedded in the proposal and acceptance, which could not
        // survive in a permanent object (browserid-pay-3a8p), and it is what
        // lets an indexer group reputation by human and spot two agents run by
        // the same account.
        let payload = json!({ "v": 1, "id": contract_id, "locked_at": locked_at,
            "payer_account": accounts.0, "payee_account": accounts.1 });
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

    async fn adjudicator_replaced(&self, contract_id: &str, from: &str, to: &str) {
        let payload = json!({ "v": 1, "id": contract_id, "from": from, "to": to, "at": chrono::Utc::now().timestamp() });
        self.post(
            WriteSpec {
                path: format!("/agreements/{contract_id}/"),
                id: "adjudicator".into(),
                schema: "agreement.adjudicator.v1".into(),
                payload: serde_json::to_vec(&payload).expect("adjudicator json"),
            },
            &format!("adjudicator replacement for {contract_id}"),
        )
        .await;
    }

    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str, price_minor: i64, currency: &str) {
        // A ruling-driven settlement is a dispute in the reputation record.
        let kind = if outcome.starts_with("ruled:") { "agreement:disputed" } else { "agreement:settled" };
        let band = value_band(price_minor);
        for (p, role) in [(parties.0, "payer"), (parties.1, "payee")] {
            self.attest(
                p,
                kind,
                json!({ "contract": contract_id, "outcome": outcome, "role": role, "value_band": band, "currency": currency }),
            )
            .await;
        }
    }
}

/// The price as a band, never the amount (reputation draft: "value bands
/// only"). Bands are in major units of the deployment currency.
pub fn value_band(price_minor: i64) -> &'static str {
    let major = price_minor / 100;
    match major {
        m if m < 10 => "lt10",
        m if m < 100 => "10-100",
        m if m < 1_000 => "100-1k",
        m if m < 10_000 => "1k-10k",
        _ => "gte10k",
    }
}

#[cfg(test)]
mod tests {
    use super::value_band;
    #[test]
    fn value_bands() {
        assert_eq!(value_band(999), "lt10");
        assert_eq!(value_band(1_000), "10-100");
        assert_eq!(value_band(5_600), "10-100");
        assert_eq!(value_band(10_000), "100-1k");
        assert_eq!(value_band(99_999), "100-1k");
        assert_eq!(value_band(100_000), "1k-10k");
        assert_eq!(value_band(1_000_000), "gte10k");
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
