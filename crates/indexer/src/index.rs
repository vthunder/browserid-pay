//! The reverse index. SQLite; rebuilt from chain reads on every poll (the
//! chain is small and the daemon's list reads are the source of truth).
//!
//! Identity matching is **exact string** (domain part lowercased): the
//! daemon exposes no subject→controller resolution, so `alice` and
//! `alice@x.test` stay distinct here. When the daemon resolves names this is
//! the one place to widen.

use crate::chain::{Obj, StateRoot};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Index(Arc<Mutex<Connection>>);

/// A reference a client can re-fetch from the daemon and verify itself.
#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    pub path: String,
    pub id: String,
    pub block: u64,
    pub object_hash: String,
}

impl From<&Obj> for Reference {
    fn from(o: &Obj) -> Self {
        Self { path: o.path.clone(), id: o.id.clone(), block: o.block, object_hash: o.object_hash.clone() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Agreement {
    pub id: String,
    pub state: String,
    pub proposer: String,
    pub acceptor: String,
    pub payer: String,
    pub payee: String,
    pub adjudicator: String,
    pub custodian: String,
    pub mode: String,
    pub expires: Option<i64>,
    pub proposal_block: u64,
    pub acceptance_block: Option<u64>,
    pub lock_block: Option<u64>,
    pub ruling_block: Option<u64>,
    pub evidence: Vec<Value>,
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attestation {
    pub issuer: String,
    pub subject: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub value: Value,
    pub issued_at: Option<i64>,
    pub expires: Option<i64>,
    pub reference: Reference,
}

pub fn norm(identity: &str) -> String {
    match identity.rsplit_once('@') {
        Some((local, domain)) => format!("{local}@{}", domain.to_ascii_lowercase()),
        None => identity.to_string(),
    }
}

impl Index {
    pub fn open(path: &str) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS agreements (
               id TEXT PRIMARY KEY, state TEXT NOT NULL,
               proposer TEXT NOT NULL, acceptor TEXT NOT NULL, payer TEXT NOT NULL, payee TEXT NOT NULL,
               adjudicator TEXT NOT NULL, custodian TEXT NOT NULL, mode TEXT NOT NULL, expires INTEGER,
               proposal_block INTEGER NOT NULL, acceptance_block INTEGER, lock_block INTEGER, ruling_block INTEGER,
               evidence TEXT NOT NULL, refs TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS agreements_proposer ON agreements(proposer);
             CREATE INDEX IF NOT EXISTS agreements_acceptor ON agreements(acceptor);
             CREATE INDEX IF NOT EXISTS agreements_adjudicator ON agreements(adjudicator);
             CREATE TABLE IF NOT EXISTS attestations (
               path TEXT NOT NULL, id TEXT NOT NULL, issuer TEXT NOT NULL, subject TEXT NOT NULL, typ TEXT NOT NULL,
               value TEXT NOT NULL, issued_at INTEGER, expires INTEGER, block INTEGER NOT NULL, object_hash TEXT NOT NULL,
               PRIMARY KEY (path, id)
             );
             CREATE INDEX IF NOT EXISTS attestations_subject ON attestations(subject, typ);
             CREATE TABLE IF NOT EXISTS providers (
               kind TEXT NOT NULL, name TEXT NOT NULL, payload TEXT NOT NULL, block INTEGER NOT NULL, object_hash TEXT NOT NULL,
               PRIMARY KEY (kind, name)
             );",
        )?;
        Ok(Self(Arc::new(Mutex::new(c))))
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut c = self.0.lock().expect("index lock");
        f(&mut c)
    }

    /// Replace the whole index from one consistent set of chain reads.
    pub fn rebuild(&self, agreements: &[Obj], attestations: &[Obj], providers: &[Obj], contribution: Option<&Obj>, root: &StateRoot) -> Result<()> {
        let derived = derive_agreements(agreements, attestations);
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute_batch("DELETE FROM agreements; DELETE FROM attestations; DELETE FROM providers;")?;
            for a in &derived {
                tx.execute(
                    "INSERT INTO agreements(id,state,proposer,acceptor,payer,payee,adjudicator,custodian,mode,expires,proposal_block,acceptance_block,lock_block,ruling_block,evidence,refs)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                    params![a.id, a.state, a.proposer, a.acceptor, a.payer, a.payee, a.adjudicator, a.custodian, a.mode, a.expires,
                            a.proposal_block as i64, a.acceptance_block.map(|b| b as i64), a.lock_block.map(|b| b as i64), a.ruling_block.map(|b| b as i64),
                            serde_json::to_string(&a.evidence)?, serde_json::to_string(&a.references)?],
                )?;
            }
            for o in attestations {
                let Some(att) = parse_attestation(o) else { continue };
                tx.execute(
                    "INSERT OR REPLACE INTO attestations(path,id,issuer,subject,typ,value,issued_at,expires,block,object_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![o.path, o.id, att.issuer, att.subject, att.typ, att.value.to_string(), att.issued_at, att.expires, o.block as i64, o.object_hash],
                )?;
            }
            for o in providers {
                // /providers/<kind>/<name>  →  path "/providers/custodians/", id = name
                let kind = o.path.trim_start_matches("/providers/").trim_end_matches('/').to_string();
                if kind != "custodians" && kind != "adjudicators" {
                    continue;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO providers(kind,name,payload,block,object_hash) VALUES(?1,?2,?3,?4,?5)",
                    params![kind, o.id, o.json().to_string(), o.block as i64, o.object_hash],
                )?;
            }
            let contribution = contribution.map(|o| json!({ "value": o.json(), "reference": Reference::from(o) })).unwrap_or(Value::Null);
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('contribution',?1)", params![contribution.to_string()])?;
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('state_root',?1)", params![serde_json::to_string(root)?])?;
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('indexed_at',?1)", params![chrono::Utc::now().timestamp().to_string()])?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn meta(&self, k: &str) -> Result<Option<Value>> {
        self.with(|c| {
            let s: Option<String> = c.query_row("SELECT v FROM meta WHERE k=?1", params![k], |r| r.get(0)).optional()?;
            Ok(s.and_then(|s| serde_json::from_str(&s).ok().or(Some(Value::String(s)))))
        })
    }

    pub fn agreements_by_party(&self, party: &str) -> Result<Vec<Agreement>> {
        let p = norm(party);
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {AG_COLS} FROM agreements WHERE proposer=?1 OR acceptor=?1 OR payer=?1 OR payee=?1 OR adjudicator=?1 OR custodian=?1 ORDER BY proposal_block DESC"
            ))?;
            let v = st.query_map(params![p], ag_row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn agreement(&self, id: &str) -> Result<Option<Agreement>> {
        self.with(|c| Ok(c.query_row(&format!("SELECT {AG_COLS} FROM agreements WHERE id=?1"), params![id], ag_row).optional()?))
    }

    pub fn attestations_by_subject(&self, subject: &str, typ: Option<&str>) -> Result<Vec<Attestation>> {
        let s = norm(subject);
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT path,id,issuer,subject,typ,value,issued_at,expires,block,object_hash FROM attestations WHERE subject=?1 AND (?2 IS NULL OR typ=?2) ORDER BY block DESC",
            )?;
            let v = st
                .query_map(params![s, typ], |r| {
                    Ok(Attestation {
                        issuer: r.get(2)?,
                        subject: r.get(3)?,
                        typ: r.get(4)?,
                        value: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
                        issued_at: r.get(6)?,
                        expires: r.get(7)?,
                        reference: Reference { path: r.get(0)?, id: r.get(1)?, block: r.get::<_, i64>(8)? as u64, object_hash: r.get(9)? },
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn providers(&self, kind: &str) -> Result<Vec<Value>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT name,payload,block,object_hash FROM providers WHERE kind=?1 ORDER BY name")?;
            let v = st
                .query_map(params![kind], |r| {
                    let payload: Value = serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or(Value::Null);
                    Ok(json!({
                        "name": r.get::<_, String>(0)?, "listing": payload,
                        "reference": { "path": format!("/providers/{kind}/"), "id": r.get::<_, String>(0)?, "block": r.get::<_, i64>(2)?, "object_hash": r.get::<_, String>(3)? }
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    /// One indexer's subjective view. Inputs are verifiable; the counts are
    /// not a protocol quantity.
    pub fn reputation(&self, subject: &str) -> Result<Value> {
        let s = norm(subject);
        let atts = self.attestations_by_subject(&s, None)?;
        let now = chrono::Utc::now().timestamp();
        let mut by_type: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> = Default::default();
        for a in atts.iter().filter(|a| a.expires.map_or(true, |e| e > now)) {
            by_type.entry(a.typ.clone()).or_default().insert(a.issuer.clone());
        }
        let ags = self.agreements_by_party(&s)?;
        let settled = ags.iter().filter(|a| a.state == "settled").count();
        let ruled = ags.iter().filter(|a| a.state == "ruled").count();
        let disputed = ags.iter().filter(|a| a.state == "disputed").count();
        Ok(json!({
            "subject": s,
            "view": "index.dsp.fyi/v1 — one indexer's subjective view; recompute from the references",
            "attestations": by_type.iter().map(|(t, issuers)| (t.clone(), json!({ "distinct_issuers": issuers.len(), "issuers": issuers }))).collect::<serde_json::Map<_, _>>(),
            "agreements": { "as_party": ags.len(), "settled": settled, "ruled": ruled, "disputed": disputed },
        }))
    }
}

const AG_COLS: &str = "id,state,proposer,acceptor,payer,payee,adjudicator,custodian,mode,expires,proposal_block,acceptance_block,lock_block,ruling_block,evidence,refs";

fn ag_row(r: &rusqlite::Row) -> rusqlite::Result<Agreement> {
    Ok(Agreement {
        id: r.get(0)?,
        state: r.get(1)?,
        proposer: r.get(2)?,
        acceptor: r.get(3)?,
        payer: r.get(4)?,
        payee: r.get(5)?,
        adjudicator: r.get(6)?,
        custodian: r.get(7)?,
        mode: r.get(8)?,
        expires: r.get(9)?,
        proposal_block: r.get::<_, i64>(10)? as u64,
        acceptance_block: r.get::<_, Option<i64>>(11)?.map(|b| b as u64),
        lock_block: r.get::<_, Option<i64>>(12)?.map(|b| b as u64),
        ruling_block: r.get::<_, Option<i64>>(13)?.map(|b| b as u64),
        evidence: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
        references: serde_json::from_str::<Vec<Value>>(&r.get::<_, String>(15)?)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                Some(Reference {
                    path: v.get("path")?.as_str()?.to_string(),
                    id: v.get("id")?.as_str()?.to_string(),
                    block: v.get("block")?.as_u64()?,
                    object_hash: v.get("object_hash")?.as_str()?.to_string(),
                })
            })
            .collect(),
    })
}

pub struct ParsedAttestation {
    pub issuer: String,
    pub subject: String,
    pub typ: String,
    pub value: Value,
    pub issued_at: Option<i64>,
    pub expires: Option<i64>,
}

/// `/<issuer>/attestations/<subject>/<type>` (Attestation spec layout); the
/// payload's `subject`/`type` win when present.
pub fn parse_attestation(o: &Obj) -> Option<ParsedAttestation> {
    let segs: Vec<&str> = o.path.trim_matches('/').split('/').collect();
    let (issuer, path_subject) = match segs.as_slice() {
        [issuer, "attestations", subject] => (issuer.to_string(), Some(subject.to_string())),
        [issuer, "attestations"] => (issuer.to_string(), None),
        _ => return None,
    };
    let v = o.json();
    let subject = v.get("subject").and_then(|s| s.as_str()).map(String::from).or(path_subject)?;
    let typ = v.get("type").and_then(|s| s.as_str()).map(String::from).unwrap_or_else(|| o.id.clone());
    Some(ParsedAttestation {
        issuer: norm(&issuer),
        subject: norm(&subject),
        typ,
        value: v.get("value").cloned().unwrap_or(Value::Null),
        issued_at: v.get("issued_at").and_then(|x| x.as_i64()),
        expires: v.get("expires").and_then(|x| x.as_i64()),
    })
}

/// Derive one row per agreement from the objects under `/agreements/` plus
/// the settlement attestations.
///
/// State rules (first match wins):
/// - `ruled`     — a `ruling` object exists after the proposal.
/// - `settled`   — an `acknowledgement` evidence object by the payer exists, or
///                 an `agreement:settled` attestation names the contract.
/// - `disputed`  — a `dispute` evidence object exists (and no ruling).
/// - `locked`    — a `lock` object exists after the acceptance.
/// - `accepted`  — an `acceptance` whose `proposal_hash` is `sha256:<hash of
///                 the proposal object>` exists at a block ≥ the proposal's.
///                 (The daemon's `object_hash` is the object-level hash; we
///                 accept either that or a hash the acceptance carries
///                 matching it — see README.)
/// - `expired`   — no acceptance and the offer expiry has passed.
/// - `proposed`  — otherwise.
/// An acceptance with no preceding proposal is ignored (dangling paper).
pub fn derive_agreements(agreements: &[Obj], attestations: &[Obj]) -> Vec<Agreement> {
    use std::collections::BTreeMap;
    let mut by_id: BTreeMap<String, Vec<&Obj>> = BTreeMap::new();
    for o in agreements {
        // /agreements/<id>/  or  /agreements/<id>/evidence/
        let rest = o.path.trim_start_matches("/agreements/");
        let Some(id) = rest.split('/').next().filter(|s| !s.is_empty()) else { continue };
        by_id.entry(id.to_string()).or_default().push(o);
    }
    let settled_ids: std::collections::HashSet<String> = attestations
        .iter()
        .filter_map(parse_attestation)
        .filter(|a| a.typ == "agreement:settled")
        .filter_map(|a| a.value.get("contract").and_then(|c| c.as_str()).map(String::from))
        .collect();
    let now = chrono::Utc::now().timestamp();
    let mut out = Vec::new();
    for (id, objs) in by_id {
        let base = format!("/agreements/{id}/");
        let find = |name: &str| objs.iter().find(|o| o.path == base && o.id == name).copied();
        let Some(proposal) = find("proposal") else { continue };
        let p = proposal.json();
        let s = |path: &[&str]| -> String {
            let mut v = &p;
            for k in path {
                v = v.get(k).unwrap_or(&Value::Null);
            }
            v.as_str().map(norm).unwrap_or_default()
        };
        let acceptance = find("acceptance").filter(|a| {
            a.block >= proposal.block
                && a.json().get("proposal_hash").and_then(|h| h.as_str()).map_or(false, |h| {
                    let h = h.strip_prefix("sha256:").unwrap_or(h);
                    h.eq_ignore_ascii_case(&proposal.object_hash)
                })
        });
        let lock = acceptance.and_then(|a| find("lock").filter(|l| l.block >= a.block));
        let ruling = find("ruling").filter(|r| r.block >= proposal.block);
        let evidence_dir = format!("{base}evidence/");
        let mut evidence: Vec<&Obj> = objs.iter().filter(|o| o.path == evidence_dir).copied().collect();
        evidence.sort_by_key(|o| (o.block, o.id.clone()));
        let kind = |o: &Obj| o.json().get("kind").and_then(|k| k.as_str()).unwrap_or("").to_string();
        let acknowledged = evidence.iter().any(|o| kind(o) == "acknowledgement");
        let disputed = evidence.iter().any(|o| kind(o) == "dispute");
        let expires = p.get("expires").and_then(|e| e.as_i64());
        let state = if ruling.is_some() {
            "ruled"
        } else if acknowledged || settled_ids.contains(&id) {
            "settled"
        } else if disputed {
            "disputed"
        } else if lock.is_some() {
            "locked"
        } else if acceptance.is_some() {
            "accepted"
        } else if expires.map_or(false, |e| e < now) {
            "expired"
        } else {
            "proposed"
        };
        let mut references: Vec<Reference> = vec![proposal.into()];
        references.extend(acceptance.map(Reference::from));
        references.extend(lock.map(Reference::from));
        references.extend(ruling.map(Reference::from));
        references.extend(evidence.iter().map(|o| Reference::from(*o)));
        out.push(Agreement {
            id,
            state: state.into(),
            proposer: s(&["parties", "proposer"]),
            acceptor: s(&["parties", "acceptor"]),
            payer: s(&["roles", "payer"]),
            payee: s(&["roles", "payee"]),
            adjudicator: s(&["adjudicator"]),
            custodian: p.get("custodian").and_then(|c| c.as_str()).unwrap_or("").to_string(),
            mode: p.get("mode").and_then(|m| m.as_str()).unwrap_or("commitment").to_string(),
            expires,
            proposal_block: proposal.block,
            acceptance_block: acceptance.map(|a| a.block),
            lock_block: lock.map(|l| l.block),
            ruling_block: ruling.map(|r| r.block),
            evidence: evidence.iter().map(|o| json!({ "id": o.id, "block": o.block, "kind": kind(o), "by": o.json().get("by"), "commitment": o.json().get("commitment") })).collect(),
            references,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(path: &str, id: &str, block: u64, v: Value) -> Obj {
        Obj {
            path: path.into(),
            id: id.into(),
            creator: "e_x".into(),
            content_schema: None,
            block,
            object_hash: format!("h-{path}{id}"),
            value: Some(v),
            payload_text: String::new(),
            confirmed: true,
        }
    }

    fn proposal(id: &str, block: u64) -> Obj {
        obj(&format!("/agreements/{id}/"), "proposal", block, json!({
            "v": 1, "id": id, "mode": "commitment", "commitment": "sha256:00",
            "parties": { "proposer": "alice@X.test", "acceptor": "bob@y.test" },
            "roles": { "payer": "alice@x.test", "payee": "bob@y.test" },
            "adjudicator": "judge@dsp.fyi", "custodian": "https://pay.dsp.fyi", "expires": 4_000_000_000i64 }))
    }
    fn acceptance(id: &str, block: u64, hash: &str) -> Obj {
        obj(&format!("/agreements/{id}/"), "acceptance", block, json!({ "v": 1, "id": id, "proposal_hash": format!("sha256:{hash}") }))
    }
    fn evidence(id: &str, eid: &str, block: u64, kind: &str) -> Obj {
        obj(&format!("/agreements/{id}/evidence/"), eid, block, json!({ "v": 1, "id": id, "kind": kind, "by": "bob@y.test" }))
    }

    #[test]
    fn state_derivation_follows_chain_order() {
        let p = proposal("agr_1", 10);
        let h = p.object_hash.clone();
        // proposed
        assert_eq!(derive_agreements(&[p.clone()], &[])[0].state, "proposed");
        // dangling acceptance (no proposal) is ignored
        assert!(derive_agreements(&[acceptance("agr_9", 5, "zz")], &[]).is_empty());
        // acceptance before the proposal's block does not count
        assert_eq!(derive_agreements(&[p.clone(), acceptance("agr_1", 9, &h)], &[])[0].state, "proposed");
        // wrong hash does not count
        assert_eq!(derive_agreements(&[p.clone(), acceptance("agr_1", 11, "nope")], &[])[0].state, "proposed");
        let acc = acceptance("agr_1", 11, &h);
        assert_eq!(derive_agreements(&[p.clone(), acc.clone()], &[])[0].state, "accepted");
        let lock = obj("/agreements/agr_1/", "lock", 12, json!({ "v": 1, "id": "agr_1", "locked_at": 1 }));
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone()], &[])[0].state, "locked");
        let disp = evidence("agr_1", "e1", 13, "dispute");
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone(), disp.clone()], &[])[0].state, "disputed");
        let ack = evidence("agr_1", "e2", 14, "acknowledgement");
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone(), ack], &[])[0].state, "settled");
        let ruling = obj("/agreements/agr_1/", "ruling", 15, json!({ "v": 1, "id": "agr_1", "outcome": "payee" }));
        let a = &derive_agreements(&[p.clone(), acc.clone(), lock.clone(), disp, ruling], &[])[0];
        assert_eq!(a.state, "ruled");
        assert_eq!(a.references.len(), 5);
        assert_eq!(a.proposer, "alice@x.test", "domain part lowercased");
        // settled via the custodian's attestation
        let att = obj("/escrow@dsp.fyi/attestations/alice@x.test/", "agreement:settled", 16, json!({ "subject": "alice@x.test", "type": "agreement:settled", "value": { "contract": "agr_1" }, "issued_at": 1 }));
        assert_eq!(derive_agreements(&[p.clone(), acc, lock], &[att])[0].state, "settled");
        // expired offer
        let mut old = proposal("agr_2", 20);
        old.value.as_mut().unwrap()["expires"] = json!(1);
        assert_eq!(derive_agreements(&[old], &[])[0].state, "expired");
    }

    #[test]
    fn reverse_index_queries() {
        let ix = Index::open(":memory:").unwrap();
        let p = proposal("agr_1", 10);
        let h = p.object_hash.clone();
        let acc = acceptance("agr_1", 11, &h);
        let att1 = obj("/bob@y.test/attestations/alice@x.test/", "agreement:completed", 12, json!({ "subject": "alice@x.test", "type": "agreement:completed", "value": { "contract": "agr_1" }, "issued_at": 5 }));
        let att2 = obj("/judge@dsp.fyi/attestations/alice@x.test/", "agreement:ruled", 13, json!({ "subject": "alice@X.test", "type": "agreement:ruled", "value": {}, "issued_at": 5, "expires": 1 }));
        let prov = obj("/providers/adjudicators/", "judge", 3, json!({ "identity": "judge@dsp.fyi", "price": { "amount": "5.00", "currency": "USD" } }));
        let contrib = obj("/sys/", "contribution", 1, json!({ "recipient": "fund@dsp.fyi", "suggested_percentage": 1.0 }));
        ix.rebuild(&[p, acc], &[att1, att2], &[prov], Some(&contrib), &StateRoot { block: 13, state_root: "r".into() }).unwrap();

        assert_eq!(ix.agreements_by_party("ALICE@x.test").unwrap().len(), 0, "local part is byte-exact");
        assert_eq!(ix.agreements_by_party("alice@X.TEST").unwrap().len(), 1);
        assert_eq!(ix.agreements_by_party("judge@dsp.fyi").unwrap()[0].state, "accepted");
        assert!(ix.agreements_by_party("nobody@z").unwrap().is_empty());
        assert_eq!(ix.agreement("agr_1").unwrap().unwrap().acceptance_block, Some(11));
        assert_eq!(ix.attestations_by_subject("alice@x.test", None).unwrap().len(), 2);
        assert_eq!(ix.attestations_by_subject("alice@x.test", Some("agreement:completed")).unwrap()[0].issuer, "bob@y.test");
        assert_eq!(ix.attestations_by_subject("alice", None).unwrap().len(), 0, "no name resolution");
        assert_eq!(ix.providers("adjudicators").unwrap()[0]["name"], "judge");
        assert!(ix.providers("custodians").unwrap().is_empty());
        assert_eq!(ix.meta("contribution").unwrap().unwrap()["value"]["recipient"], "fund@dsp.fyi");
        let rep = ix.reputation("alice@x.test").unwrap();
        assert_eq!(rep["attestations"]["agreement:completed"]["distinct_issuers"], 1);
        assert!(rep["attestations"].get("agreement:ruled").is_none(), "expired attestations drop out");
        assert_eq!(rep["agreements"]["as_party"], 1);
    }
}
