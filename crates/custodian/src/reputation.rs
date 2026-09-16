//! Reputation-gated grants (reputation draft, "Plugging it back in").
//!
//! A `contract:agreement` entry may carry `min_reputation { indexer, score }`.
//! The custodian is the audience, so it enforces it at lock: fetch the
//! counterparty's signed scorecard from the indexer the GRANTOR named, verify
//! the card's signature against that indexer's published key, and refuse
//! below the threshold. The card seen is stored with the escrow so the
//! decision is auditable. One indexer's subjective view, chosen by the human.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

pub const CARD_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub enum ReputationError {
    /// The indexer answered with a valid card and the score is below the threshold.
    Below { score: u8, threshold: u8, indexer: String },
    /// The indexer could not be reached, or its card failed verification.
    Unavailable(String),
}

pub struct Reputation {
    http: reqwest::Client,
    /// Dev only: permit `http://` indexers (the demo runs one on localhost).
    allow_http: bool,
    /// Cached indexer public keys, by indexer origin.
    keys: Mutex<HashMap<String, String>>,
}

impl Reputation {
    pub fn new(allow_http: bool) -> Self {
        Self {
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().expect("http client"),
            allow_http,
            keys: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch and verify `subject`'s card at `indexer`; `Ok(card)` iff
    /// `card.score >= threshold`.
    pub async fn require(&self, indexer: &str, subject: &str, threshold: u8) -> Result<Value, ReputationError> {
        let indexer = indexer.trim_end_matches('/');
        if !(indexer.starts_with("https://") || (self.allow_http && indexer.starts_with("http://"))) {
            return Err(ReputationError::Unavailable(format!("indexer {indexer} must be https")));
        }
        let card = self.fetch_json(&format!("{indexer}/v1/reputation?subject={}", urlencode(subject))).await?;
        // Verify against the published key (cached per indexer). On a
        // signature failure, refresh the key once (rotation) and retry.
        let key = self.key_for(indexer, false).await?;
        let ok = verify(&card, &key) || {
            let fresh = self.key_for(indexer, true).await?;
            verify(&card, &fresh)
        };
        if !ok {
            return Err(ReputationError::Unavailable(format!("card from {indexer} failed signature verification")));
        }
        if card.get("subject").and_then(|s| s.as_str()) != Some(subject) {
            return Err(ReputationError::Unavailable(format!("card from {indexer} is for another subject")));
        }
        let score = card.get("score").and_then(|s| s.as_f64()).ok_or_else(|| ReputationError::Unavailable("card has no score".into()))?;
        let score = score.round().clamp(0.0, 100.0) as u8;
        if score < threshold {
            return Err(ReputationError::Below { score, threshold, indexer: indexer.to_string() });
        }
        Ok(card)
    }

    async fn key_for(&self, indexer: &str, refresh: bool) -> Result<String, ReputationError> {
        if !refresh {
            if let Some(k) = self.keys.lock().expect("keys").get(indexer) {
                return Ok(k.clone());
            }
        }
        let v = self.fetch_json(&format!("{indexer}/v1/reputation/key")).await?;
        let k = v.get("public_key").and_then(|s| s.as_str()).ok_or_else(|| ReputationError::Unavailable("indexer key endpoint has no public_key".into()))?.to_string();
        self.keys.lock().expect("keys").insert(indexer.to_string(), k.clone());
        Ok(k)
    }

    async fn fetch_json(&self, url: &str) -> Result<Value, ReputationError> {
        let r = self.http.get(url).send().await.map_err(|e| ReputationError::Unavailable(format!("{url}: {e}")))?;
        if !r.status().is_success() {
            return Err(ReputationError::Unavailable(format!("{url}: HTTP {}", r.status())));
        }
        let bytes = r.bytes().await.map_err(|e| ReputationError::Unavailable(format!("{url}: {e}")))?;
        if bytes.len() > CARD_MAX_BYTES {
            return Err(ReputationError::Unavailable(format!("{url}: response too large")));
        }
        serde_json::from_slice(&bytes).map_err(|e| ReputationError::Unavailable(format!("{url}: bad JSON: {e}")))
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Verify a scorecard's compact JWS (EdDSA) against the indexer's key: the
/// payload must equal the canonical JSON of the card without `signature`.
/// Mirrors `crates/indexer/src/reputation.rs`.
pub fn verify(card: &Value, public_key_b64: &str) -> bool {
    let Some(jws) = card.get("signature").and_then(|s| s.as_str()) else { return false };
    let mut parts = jws.split('.');
    let (Some(h), Some(p), Some(s)) = (parts.next(), parts.next(), parts.next()) else { return false };
    let dec = |x: &str| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, x).ok();
    let (Some(pb), Some(sb)) = (dec(p), dec(s)) else { return false };
    let mut unsigned = card.clone();
    if let Some(m) = unsigned.as_object_mut() {
        m.remove("signature");
    }
    if pb != canonical(&unsigned).as_bytes() {
        return false;
    }
    let Ok(pk) = browserid_core::PublicKey::from_base64(public_key_b64) else { return false };
    pk.verify(format!("{h}.{p}").as_bytes(), &sb).is_ok()
}

/// Canonical JSON as the indexer produces it (keys sorted, no whitespace,
/// integral floats printed as integers).
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical(&m[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => serde_json::to_string(other).unwrap(),
    }
}

/// A signed card for tests and the demo stub: the indexer's exact signing
/// procedure over `card`.
pub fn sign_for_tests(card: &Value, key: &browserid_core::KeyPair) -> String {
    let b64 = |b: &[u8]| base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b);
    let header = b64(serde_json::json!({ "alg": "EdDSA", "typ": "browserid-pay-reputation-card", "kid": key.public_key().to_base64() }).to_string().as_bytes());
    let payload = b64(canonical(card).as_bytes());
    let signing_input = format!("{header}.{payload}");
    format!("{signing_input}.{}", b64(&key.sign(signing_input.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn signed_card_verifies_and_tamper_fails() {
        let key = browserid_core::KeyPair::generate();
        let mut card = json!({ "subject": "bob+agent@y.test", "score": 61, "components": { "tenure": 1.0 } });
        card["signature"] = json!(sign_for_tests(&card, &key));
        assert!(verify(&card, &key.public_key().to_base64()));
        card["score"] = json!(99);
        assert!(!verify(&card, &key.public_key().to_base64()));
        let other = browserid_core::KeyPair::generate();
        assert!(!verify(&card, &other.public_key().to_base64()));
    }
}
