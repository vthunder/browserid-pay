//! The scorecard — `browserid-pay-reputation-v1`, one indexer's view
//! (docs/plans/2026-09-16-reputation-design-draft.md, "The scorecard").
//!
//! Everything here is computed from public, attributed objects; the card
//! carries the references behind every number and is signed by this
//! indexer's key so a consumer can hold it to its published view. Scores
//! live off chain and are subjective: another indexer may weigh the same
//! facts differently.
//!
//! Wording rule: the "grantor" of a party's warrant is the CONTROLLING
//! ACCOUNT behind an agent — never "the human". Accounts are cheap to mint;
//! nothing here is sybil-proof. The one thing the grantor buys us is refusing
//! to count an agreement an account made with itself.

use crate::index::{norm, Agreement, Attestation, Index};
use anyhow::Result;
use base64::Engine;
use browserid_core::KeyPair;
use serde_json::{json, Value};

pub const FORMULA: &str = "browserid-pay-reputation-v1";
/// Tenure is measured in blocks since the subject's first proposal. Avail
/// turing produces a block roughly every 20 s, so 90 days ≈ 388 800 blocks.
pub const BLOCKS_90_DAYS: f64 = 388_800.0;
/// A ruling that awarded this side at least this share counts as "mostly for".
const MOSTLY_FOR: f64 = 0.8;
/// … and below this share counts as a ruling against.
const AGAINST_BELOW: f64 = 0.5;

/// Which side of an agreement the subject is on, and whether the agreement
/// counts (reached lock, two distinct controlling accounts).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Input {
    pub id: String,
    pub state: String,
    pub role: String,
    pub counterparty: String,
    pub my_grantor: Option<String>,
    pub counterparty_grantor: Option<String>,
    pub counted: bool,
    pub why_not_counted: Option<&'static str>,
    pub outcome: Option<String>,
    /// This side's share of the price on a ruling (0..1).
    pub share: Option<f64>,
    pub late: bool,
    pub responded: Option<bool>,
    pub reveals_matched: Option<bool>,
    pub references: Vec<crate::index::Reference>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Components {
    pub completion: f64,
    pub dispute_record: f64,
    pub responsiveness: f64,
    pub promptness: f64,
    pub tenure: f64,
}

pub struct Signer {
    key: KeyPair,
}

impl Signer {
    /// Load (or generate) the indexer's Ed25519 key at `path`.
    pub fn load_or_create(path: &std::path::Path) -> Result<Self> {
        if let Ok(text) = std::fs::read_to_string(path) {
            let v: Value = serde_json::from_str(&text)?;
            let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(v["seed"].as_str().unwrap_or(""))?;
            return Ok(Self { key: KeyPair::from_seed(&seed)? });
        }
        let key = KeyPair::generate();
        let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.secret_bytes());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(&json!({ "alg": "Ed25519", "seed": seed }))?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(Self { key })
    }

    pub fn from_key(key: KeyPair) -> Self {
        Self { key }
    }

    pub fn public_key_b64(&self) -> String {
        self.key.public_key().to_base64()
    }

    /// Compact JWS (EdDSA) over the canonical JSON of `card`.
    pub fn sign(&self, card: &Value) -> String {
        let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
        let header = b64(json!({ "alg": "EdDSA", "typ": "browserid-pay-reputation-card", "kid": self.public_key_b64() }).to_string().as_bytes());
        let payload = b64(canonical(card).as_bytes());
        let signing_input = format!("{header}.{payload}");
        let sig = self.key.sign(signing_input.as_bytes());
        format!("{signing_input}.{}", b64(&sig))
    }
}

/// Verify a card's signature against the indexer's public key (for tests and
/// consumers): the JWS payload must equal the canonical JSON of the card
/// without its `signature`.
pub fn verify(card: &Value, public_key_b64: &str) -> bool {
    let Some(jws) = card.get("signature").and_then(|s| s.as_str()) else { return false };
    let mut parts = jws.split('.');
    let (Some(h), Some(p), Some(s)) = (parts.next(), parts.next(), parts.next()) else { return false };
    let dec = |x: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(x).ok();
    let (Some(pb), Some(sb)) = (dec(p), dec(s)) else { return false };
    let mut unsigned = card.clone();
    unsigned.as_object_mut().map(|m| m.remove("signature"));
    if pb != canonical(&unsigned).as_bytes() {
        return false;
    }
    let Ok(pk) = browserid_core::PublicKey::from_base64(public_key_b64) else { return false };
    pk.verify(format!("{h}.{p}").as_bytes(), &sb).is_ok()
}

/// Canonical JSON (JCS-style): keys sorted, no whitespace, and numbers in
/// the shortest round-trip form a JavaScript `JSON.stringify` would produce —
/// integral values print without a fractional part — so a consumer that
/// parses the card and re-serializes it reaches the same bytes.
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

/// Classify one agreement from the subject's point of view. `subject_is_grantor`
/// aggregates: the subject is an account and "my side" is whichever agent it
/// controls.
fn classify(a: &Agreement, subject: &str, ruled_att: Option<&Attestation>) -> Option<Input> {
    let (role, me, other, my_grantor, other_grantor) = if a.payer == subject || a.proposer_grantor.as_deref() == Some(subject) && a.proposer == a.payer || a.acceptor_grantor.as_deref() == Some(subject) && a.acceptor == a.payer {
        let (mg, og) = if a.proposer == a.payer { (&a.proposer_grantor, &a.acceptor_grantor) } else { (&a.acceptor_grantor, &a.proposer_grantor) };
        ("payer", &a.payer, &a.payee, mg, og)
    } else if a.payee == subject || a.proposer_grantor.as_deref() == Some(subject) && a.proposer == a.payee || a.acceptor_grantor.as_deref() == Some(subject) && a.acceptor == a.payee {
        let (mg, og) = if a.proposer == a.payee { (&a.proposer_grantor, &a.acceptor_grantor) } else { (&a.acceptor_grantor, &a.proposer_grantor) };
        ("payee", &a.payee, &a.payer, mg, og)
    } else {
        return None;
    };
    let _ = me;
    let reached_lock = matches!(a.state.as_str(), "locked" | "disputed" | "settled" | "ruled");
    let why_not = if !reached_lock {
        Some("never reached lock")
    } else if my_grantor.is_none() || other_grantor.is_none() {
        Some("a party's controlling account is unknown")
    } else if my_grantor == other_grantor {
        Some("both agents are controlled by the same account")
    } else {
        None
    };
    // Outcome and this side's share: from the ruling object (split amounts),
    // preferring the judge's attestation `share` when present.
    let (outcome, share) = match (&a.state[..], &a.ruling) {
        ("ruled", Some(r)) => {
            let att_share = ruled_att.and_then(|t| t.value.get("share")).and_then(|s| s.as_str()).and_then(|s| s.parse::<f64>().ok());
            let payee_share = match r.get("outcome").and_then(|o| o.as_str()) {
                Some("payee") => 1.0,
                Some("payer") => 0.0,
                _ => {
                    let m = |k: &str| r["split"][k].as_str().and_then(|s| s.replace('.', "").parse::<f64>().ok()).unwrap_or(0.0);
                    let (p, q) = (m("payee"), m("payer"));
                    if p + q > 0.0 { p / (p + q) } else { 0.5 }
                }
            };
            let mine = att_share.unwrap_or(if role == "payee" { payee_share } else { 1.0 - payee_share });
            (Some(format!("ruled:{}", r.get("outcome").and_then(|o| o.as_str()).unwrap_or("?"))), Some(mine))
        }
        ("settled", _) => (Some("acknowledged".into()), None),
        (s, _) => (Some(s.to_string()), None),
    };
    let flag = |k: &str| ruled_att.and_then(|t| t.value.get(k)).and_then(|v| v.as_bool());
    Some(Input {
        id: a.id.clone(),
        state: a.state.clone(),
        role: role.into(),
        counterparty: other.clone(),
        my_grantor: my_grantor.clone(),
        counterparty_grantor: other_grantor.clone(),
        counted: why_not.is_none(),
        why_not_counted: why_not,
        outcome,
        share,
        late: flag("late").unwrap_or(false),
        responded: flag("responded"),
        reveals_matched: flag("reveals_matched"),
        references: a.references.clone(),
    })
}

/// Build the (unsigned) card for `subject` — an agent identity, or a
/// controlling account (then its agents' agreements are pooled).
pub fn scorecard(index: &Index, subject: &str, current_block: u64) -> Result<Value> {
    let subject = norm(subject);
    let agreements = index.agreements_by_party(&subject)?;
    let ruled_atts: Vec<Attestation> = {
        // Attestations about the subject and about any agent it controls.
        let mut v = index.attestations_by_subject(&subject, Some("agreement:ruled"))?;
        for a in &agreements {
            for who in [&a.proposer, &a.acceptor] {
                let g = if who == &a.proposer { &a.proposer_grantor } else { &a.acceptor_grantor };
                if g.as_deref() == Some(subject.as_str()) && who != &subject {
                    v.extend(index.attestations_by_subject(who, Some("agreement:ruled"))?);
                }
            }
        }
        v
    };
    let ruled_for = |a: &Agreement, side: &str| -> Option<&Attestation> {
        ruled_atts.iter().find(|t| t.value.get("contract").and_then(|c| c.as_str()) == Some(a.id.as_str()) && t.subject == *side)
    };
    let mut inputs: Vec<Input> = Vec::new();
    let mut first_block: Option<u64> = None;
    for a in &agreements {
        let my_side = if a.payer == subject || a.proposer_grantor.as_deref() == Some(subject.as_str()) && a.proposer == a.payer || a.acceptor_grantor.as_deref() == Some(subject.as_str()) && a.acceptor == a.payer { a.payer.clone() } else { a.payee.clone() };
        let Some(i) = classify(a, &subject, ruled_for(a, &my_side)) else { continue };
        first_block = Some(first_block.map_or(a.proposal_block, |b| b.min(a.proposal_block)));
        inputs.push(i);
    }
    let counted: Vec<&Input> = inputs.iter().filter(|i| i.counted).collect();
    let total = counted.len() as f64;
    let settled_ok = counted.iter().filter(|i| i.outcome.as_deref() == Some("acknowledged")).count() as f64;
    let ruled_mostly_for = counted.iter().filter(|i| i.share.map_or(false, |s| s >= MOSTLY_FOR)).count() as f64;
    let rulings_against = counted.iter().filter(|i| i.share.map_or(false, |s| s < AGAINST_BELOW)).count() as f64;
    let completion = (settled_ok + 0.5 * ruled_mostly_for + 1.0) / (total + 2.0);
    let dispute_record = 1.0 - rulings_against / total.max(1.0);
    // Responsiveness: every dispute against me answered in the window with
    // matching reveals → 1; each miss halves it. Only rulings say.
    let misses = counted.iter().filter(|i| i.responded == Some(false) || i.reveals_matched == Some(false)).count() as i32;
    let responsiveness = 0.5f64.powi(misses);
    let late = counted.iter().filter(|i| i.late).count() as f64;
    let promptness = 1.0 - late / total.max(1.0);
    let tenure = first_block.map_or(0.0, |fb| ((current_block.saturating_sub(fb)) as f64 / BLOCKS_90_DAYS).min(1.0));
    let n_accounts = {
        let mut s: std::collections::BTreeSet<&str> = Default::default();
        for i in &counted {
            if let Some(g) = &i.counterparty_grantor {
                s.insert(g);
            }
        }
        s.len()
    };
    let breadth = 0.5 + 0.5 * ((1.0 + n_accounts as f64).log2() / 4.0).min(1.0);
    let score = 100.0 * completion * dispute_record * (0.7 + 0.3 * responsiveness) * breadth;
    let grantor = inputs.iter().find_map(|i| i.my_grantor.clone());
    let att_refs: Vec<Value> = ruled_atts.iter().map(|t| json!(t.reference)).collect();
    Ok(json!({
        "subject": subject,
        "grantor": grantor,
        "score": (score * 10.0).round() / 10.0,
        "components": Components { completion, dispute_record, responsiveness, promptness, tenure },
        "n_counterparty_accounts": n_accounts,
        "counts": { "counted": counted.len(), "settled_ok": settled_ok, "ruled_mostly_for": ruled_mostly_for, "rulings_against": rulings_against, "late": late, "response_misses": misses },
        "inputs": { "agreements": inputs, "attestations": att_refs },
        "formula": FORMULA,
        "view": "one indexer's subjective view; recompute from the references. Accounts are not people and nothing here is sybil-proof.",
        "constants": { "blocks_90_days": BLOCKS_90_DAYS, "mostly_for": MOSTLY_FOR, "against_below": AGAINST_BELOW },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{Obj, StateRoot};
    use crate::index::grantor_of_bundle;
    use serde_json::json;

    fn obj(path: &str, id: &str, block: u64, schema: &str, v: Value) -> Obj {
        Obj { path: path.into(), id: id.into(), creator: "e".into(), content_schema: Some(schema.into()), block, object_hash: format!("h-{path}{id}"), value: Some(v), payload_text: String::new(), confirmed: true }
    }
    /// A fake presentation bundle whose warrant payload carries `grantor`.
    fn bundle(grantor: &str) -> String {
        let b64 = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s.as_bytes());
        let w = format!("{}.{}.sig", b64("{}"), b64(&json!({ "grantor": grantor }).to_string()));
        format!("ac~as~{w}~cc")
    }
    fn contract(id: &str, block: u64, payer: &str, payee: &str, pg: &str, ag: &str, ruling: Option<Value>) -> Vec<Obj> {
        let base = format!("/agreements/{id}/");
        let mut v = vec![obj(&base, "proposal", block, "agreement.proposal.v1", json!({ "id": id, "parties": { "proposer": payer, "acceptor": payee }, "roles": { "payer": payer, "payee": payee }, "adjudicator": "judge@d", "custodian": "https://pay", "bundle": bundle(pg) }))];
        v.push(obj(&base, "acceptance", block + 1, "agreement.acceptance.v1", json!({ "id": id, "proposal_hash": format!("sha256:h-{base}proposal"), "bundle": bundle(ag) })));
        v.push(obj(&base, "lock", block + 2, "agreement.lock.v1", json!({ "id": id })));
        match ruling {
            Some(r) => {
                v.push(obj(&format!("{base}evidence/"), "d", block + 3, "agreement.evidence.v1", json!({ "kind": "dispute", "by": payer })));
                v.push(obj(&base, "ruling", block + 4, "agreement.ruling.v1", r));
            }
            None => v.push(obj(&format!("{base}evidence/"), "a", block + 3, "agreement.evidence.v1", json!({ "kind": "acknowledgement", "by": payer }))),
        }
        v
    }

    fn build(objs: Vec<Obj>, atts: Vec<Obj>) -> Index {
        let ix = Index::open(":memory:").unwrap();
        ix.rebuild(&objs, &atts, &[], None, &StateRoot { block: 1_000_000, state_root: "r".into() }).unwrap();
        ix
    }

    #[test]
    fn grantor_is_read_from_the_bundle() {
        assert_eq!(grantor_of_bundle(&bundle("Dan@Sandmill.org")).as_deref(), Some("Dan@sandmill.org"));
        assert!(grantor_of_bundle("garbage").is_none());
    }

    #[test]
    fn clean_record_scores_high_and_same_account_counts_zero() {
        let mut objs = vec![];
        // bob delivers for three different accounts, all acknowledged.
        for (i, (payer, pg)) in [("a1@x", "acct-a@x"), ("c1@y", "acct-c@y"), ("d1@z", "acct-d@z")].iter().enumerate() {
            objs.extend(contract(&format!("agr_{i}"), 10 + i as u64 * 10, payer, "bob+agent@y", pg, "acct-b@y", None));
        }
        // one more with bob's own account on both sides: must not count.
        objs.extend(contract("agr_self", 100, "bobs-other-agent@y", "bob+agent@y", "acct-b@y", "acct-b@y", None));
        let ix = build(objs, vec![]);
        let card = scorecard(&ix, "bob+agent@y", 1_000_000).unwrap();
        assert_eq!(card["counts"]["counted"], 3);
        assert_eq!(card["n_counterparty_accounts"], 3);
        let self_input = card["inputs"]["agreements"].as_array().unwrap().iter().find(|i| i["id"] == "agr_self").unwrap();
        assert_eq!(self_input["counted"], false);
        assert_eq!(self_input["why_not_counted"], "both agents are controlled by the same account");
        // completion (3+1)/(3+2)=0.8; dispute 1; resp 1 → 0.7+0.3=1; breadth 0.5+0.5*log2(4)/4=0.75 → 60
        let score = card["score"].as_f64().unwrap();
        assert!((score - 60.0).abs() < 0.2, "score {score}");
        assert_eq!(card["components"]["tenure"], 1.0);
        assert_eq!(card["grantor"], "acct-b@y");
    }

    #[test]
    fn a_lost_ruling_hurts_and_the_judge_flags_are_read() {
        let ruling = json!({ "id": "agr_r", "outcome": "split", "split": { "payee": "10.00", "payer": "40.00" }, "late_party": "payee", "partial": true });
        let objs = contract("agr_r", 10, "a1@x", "bob+agent@y", "acct-a@x", "acct-b@y", Some(ruling));
        let att = obj("/judge@d/attestations/bob+agent@y/", "agreement:ruled", 20, "attestation.v1", json!({ "subject": "bob+agent@y", "type": "agreement:ruled", "value": { "contract": "agr_r", "role": "payee", "share": "0.2000", "late": true, "partial": true, "responded": false, "reveals_matched": false }, "issued_at": 1 }));
        let ix = build(objs, vec![att]);
        let card = scorecard(&ix, "bob+agent@y", 1_000_000).unwrap();
        let i = &card["inputs"]["agreements"][0];
        assert_eq!(i["counted"], true);
        assert_eq!(i["share"], 0.2);
        assert_eq!(i["late"], true);
        assert_eq!(card["counts"]["rulings_against"], 1.0);
        assert_eq!(card["components"]["dispute_record"], 0.0);
        assert_eq!(card["components"]["promptness"], 0.0);
        assert_eq!(card["components"]["responsiveness"], 0.5);
        assert_eq!(card["score"], 0.0);
        // The payer side of the same ruling: share 0.8, mostly for.
        let card_a = scorecard(&ix, "a1@x", 1_000_000).unwrap();
        assert_eq!(card_a["counts"]["ruled_mostly_for"], 1.0);
        assert!(card_a["score"].as_f64().unwrap() > 20.0);
    }

    #[test]
    fn aggregates_to_the_controlling_account() {
        let mut objs = contract("agr_1", 10, "a1@x", "bob-a@y", "acct-a@x", "acct-b@y", None);
        objs.extend(contract("agr_2", 30, "c1@z", "bob-b@y", "acct-c@z", "acct-b@y", None));
        let ix = build(objs, vec![]);
        let card = scorecard(&ix, "acct-b@y", 1_000_000).unwrap();
        assert_eq!(card["counts"]["counted"], 2, "both agents' agreements pooled");
        assert_eq!(card["n_counterparty_accounts"], 2);
        let per_agent = scorecard(&ix, "bob-a@y", 1_000_000).unwrap();
        assert_eq!(per_agent["counts"]["counted"], 1);
    }

    #[test]
    fn canonical_numbers_match_javascript() {
        assert_eq!(canonical(&json!({ "b": 1.0, "a": [2.5, 3, 0.0000360082304526749], "c": 100.0 })), r#"{"a":[2.5,3,0.0000360082304526749],"b":1,"c":100}"#);
    }

    #[test]
    fn card_signature_verifies_and_tamper_fails() {
        let signer = Signer::from_key(KeyPair::generate());
        let mut card = json!({ "subject": "x@y", "score": 42.0, "nested": { "b": 1, "a": [1, 2] } });
        card["signature"] = json!(signer.sign(&card));
        assert!(verify(&card, &signer.public_key_b64()));
        let mut tampered = card.clone();
        tampered["score"] = json!(99.0);
        assert!(!verify(&tampered, &signer.public_key_b64()));
        assert!(!verify(&card, &Signer::from_key(KeyPair::generate()).public_key_b64()));
    }
}
