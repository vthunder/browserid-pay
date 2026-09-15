//! The judge's provisioning request to the broker, spelled out here because
//! `browserid_agent::request_provision` cannot send a `grantee` pin, and the
//! judge needs one: it must become **its own delegated identity** (grantee
//! `*` — the approver mints/names it, requested handle `judge`), with the
//! bundled warrants delegated **from the approver** (grantor absent → the
//! approver chooses which of their identities delegates). Grantor `self`
//! plus an as-you grantee is exactly the contradiction the broker refuses
//! (registrar `request_pin_contradiction`).

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use browserid_agent::{DeviceCredential, GrantRequest};
use browserid_core::KeyPair;
use serde_json::{json, Value};

/// The request body: device pubkey, requested handle, `services` namespace,
/// the grants, label/message, and `grantee: "*"` with no grantor pin.
pub fn request_body(device_pub_b64: &str, handle: &str, grants: &[GrantRequest], label: &str, message: &str) -> Value {
    json!({
        "provisioning_pubkey": { "algorithm": "Ed25519", "publicKey": device_pub_b64 },
        "requested_handles": { "names": [handle] },
        "namespace": "services",
        "grants": grants,
        "label": label,
        "message": message,
        "grantee": "*",
    })
}

#[allow(dead_code)]
pub struct Pending {
    http: reqwest::Client,
    broker: String,
    device_key: KeyPair,
    pub code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub fingerprint: String,
    pub interval_seconds: u64,
    pub expires_in_seconds: u64,
}

pub struct Provisioned {
    pub credential: DeviceCredential,
    /// `(audience, "warrant~config_cert")` per approved grant.
    pub grants: Vec<(String, String)>,
    pub grants_denied: Option<String>,
}

pub async fn request(broker: &str, handle: &str, grants: &[GrantRequest], label: &str, message: &str) -> Result<Pending> {
    let http = reqwest::Client::builder().no_proxy().build()?;
    let device_key = KeyPair::generate();
    let body = request_body(&device_key.public_key().to_base64(), handle, grants, label, message);
    let broker = broker.trim_end_matches('/').to_string();
    let resp = http.post(format!("{broker}/agent-provision/request")).json(&body).send().await?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() || v["success"] != Value::Bool(true) {
        bail!("IdP rejected the request ({}): {}", status.as_u16(), v["reason"].as_str().unwrap_or("no reason given"));
    }
    let f = |k: &str| v[k].as_str().map(String::from).with_context(|| format!("provision response missing {k}"));
    Ok(Pending {
        http,
        broker,
        device_key,
        code: f("code")?,
        user_code: f("user_code")?,
        verification_uri: f("verification_uri")?,
        verification_uri_complete: f("verification_uri_complete")?,
        fingerprint: f("fingerprint")?,
        interval_seconds: v["interval"].as_u64().unwrap_or(5),
        expires_in_seconds: v["expires_in"].as_u64().unwrap_or(900),
    })
}

impl Pending {
    /// Poll until the human resolves the approval (mirrors `PendingProvision::wait`).
    pub async fn wait(self) -> Result<Provisioned> {
        let url = format!("{}/agent-provision/poll", self.broker);
        let interval = std::time::Duration::from_secs(self.interval_seconds.max(1));
        loop {
            let resp = self.http.post(&url).json(&json!({ "code": self.code })).send().await?;
            let status = resp.status();
            let v: Value = resp.json().await.unwrap_or_default();
            match v["status"].as_str() {
                Some("pending") => tokio::time::sleep(interval).await,
                Some("denied") => bail!("provisioning denied by the approver"),
                Some("expired") => bail!("provisioning request expired"),
                Some("failed") => bail!("provisioning failed: {}", v["reason"].as_str().unwrap_or("no reason given")),
                Some("completed") => {
                    let cred = &v["credential"];
                    let f = |k: &str| cred[k].as_str().map(String::from).with_context(|| format!("poll credential missing {k}"));
                    let credential = DeviceCredential {
                        device_key: B64.encode(self.device_key.secret_bytes()),
                        agent_device_cert: f("device_cert")?,
                        idp: f("idp")?,
                        access_mint: cred["access_mint"].as_str().map(String::from),
                        identity: cred["identity"].as_str().map(String::from),
                    };
                    let grants = v["grants"]
                        .as_array()
                        .map(|gs| {
                            gs.iter()
                                .filter_map(|g| Some((g["audience"].as_str()?.to_string(), g["warrant"].as_str()?.to_string())))
                                .collect()
                        })
                        .unwrap_or_default();
                    return Ok(Provisioned { credential, grants, grants_denied: v["grants_denied"].as_str().map(String::from) });
                }
                _ if status.as_u16() == 410 => bail!("provisioning request expired"),
                _ if status.is_client_error() && status.as_u16() != 429 => {
                    return Err(anyhow!("IdP rejected the poll ({}): {}", status.as_u16(), v["reason"].as_str().unwrap_or("no reason given")))
                }
                _ => tokio::time::sleep(interval).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_a_delegated_agent_not_an_as_you_grant() {
        let grants = vec![
            GrantRequest { audience: "sbo+raw://avail:turing:530/".into(), scopes: vec!["action:post".into()] },
            GrantRequest { audience: "https://pay.dsp.fyi".into(), scopes: vec!["ruling".into()] },
        ];
        let b = request_body("PUB", "judge", &grants, "adjudicator", "why");
        // The approver mints/names the judge's own identity …
        assert_eq!(b["grantee"], "*");
        assert_eq!(b["requested_handles"]["names"][0], "judge");
        assert_eq!(b["namespace"], "services");
        // … and delegates the warrants from an identity they choose: no grantor
        // pin at all (absent → `*`), never `self` (contradictory with any
        // as-you grantee, and wrong here: the warrants must be grantor =
        // approver, grantee = judge).
        assert!(b.get("grantor").is_none());
        assert_eq!(b["grants"].as_array().unwrap().len(), 2);
        assert_eq!(b["grants"][1]["scopes"][0], "ruling");
        assert_eq!(b["provisioning_pubkey"]["algorithm"], "Ed25519");
    }
}
