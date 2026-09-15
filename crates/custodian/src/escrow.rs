//! Escrow: a payment warrant with a release condition (agreements draft,
//! `docs/specs/agreement-v1.md`). The payer's agent locks price + fees +
//! contribution against a contract; release happens on the payer's
//! acknowledgement, on the named adjudicator's ruling, or (later) on a
//! deadline. The custodian never parses the private terms; it holds the
//! sealed copy for the adjudicator.

use crate::db::{self, Db, Receipt};
use crate::verify::Verify;
use browserid_core::device::{parse_iso_duration_secs, parse_money_minor, ScopeEntry};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const CONTRACT_SCOPE: &str = "contract:agreement";

#[derive(Debug, Serialize)]
pub struct EscrowError {
    pub error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
fn err(error: &'static str, d: impl Into<String>) -> EscrowError {
    EscrowError { error, detail: Some(d.into()) }
}

/// Where lock objects and attestations go: the chain writer signing as the
/// custodian's identity (`crate::publish::ChainPublisher`); `Log` for
/// dev/tests. Publication is best-effort and never fails the money move.
#[async_trait::async_trait]
pub trait Publisher: Send + Sync {
    async fn lock(&self, contract_id: &str, locked_at: i64);
    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str);
}
#[allow(dead_code)]
pub struct Log;
#[async_trait::async_trait]
impl Publisher for Log {
    async fn lock(&self, contract_id: &str, locked_at: i64) {
        tracing::info!(contract_id, locked_at, "escrow lock (publisher: log only)");
    }
    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str) {
        tracing::info!(contract_id, payer = parties.0, payee = parties.1, outcome, "escrow settled (publisher: log only)");
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Escrow {
    pub id: String,
    pub state: String,
    pub payer: String,
    pub payee: String,
    pub adjudicator: String,
    pub price_minor: i64,
    pub custodian_fee_minor: i64,
    pub adjudicator_fee_minor: i64,
    pub contribution_minor: i64,
    pub contribution_recipient: Option<String>,
    pub currency: String,
    pub locked_at: i64,
    pub settled_at: Option<i64>,
    pub outcome: Option<String>,
    #[serde(skip)]
    pub sealed_terms: Option<String>,
    #[serde(skip)]
    #[allow(dead_code)]
    pub proposal: Value,
}

pub fn schema(c: &Connection) -> anyhow::Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS escrows (
           id TEXT PRIMARY KEY,
           state TEXT NOT NULL,
           payer TEXT NOT NULL,
           payee TEXT NOT NULL,
           adjudicator TEXT NOT NULL,
           price_minor INTEGER NOT NULL,
           custodian_fee_minor INTEGER NOT NULL,
           adjudicator_fee_minor INTEGER NOT NULL,
           contribution_minor INTEGER NOT NULL,
           contribution_recipient TEXT,
           currency TEXT NOT NULL,
           sealed_terms TEXT,
           proposal TEXT NOT NULL,
           warrant_key TEXT NOT NULL,
           locked_at INTEGER NOT NULL,
           settled_at INTEGER,
           outcome TEXT,
           ruling TEXT
         );
         CREATE INDEX IF NOT EXISTS escrows_party ON escrows(payer, payee);",
    )?;
    Ok(())
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Escrow> {
    Ok(Escrow {
        id: r.get(0)?,
        state: r.get(1)?,
        payer: r.get(2)?,
        payee: r.get(3)?,
        adjudicator: r.get(4)?,
        price_minor: r.get(5)?,
        custodian_fee_minor: r.get(6)?,
        adjudicator_fee_minor: r.get(7)?,
        contribution_minor: r.get(8)?,
        contribution_recipient: r.get(9)?,
        currency: r.get(10)?,
        sealed_terms: r.get(11)?,
        proposal: serde_json::from_str::<Value>(&r.get::<_, String>(12)?).unwrap_or(Value::Null),
        locked_at: r.get(13)?,
        settled_at: r.get(14)?,
        outcome: r.get(15)?,
    })
}
const COLS: &str = "id, state, payer, payee, adjudicator, price_minor, custodian_fee_minor, adjudicator_fee_minor, contribution_minor, contribution_recipient, currency, sealed_terms, proposal, locked_at, settled_at, outcome";

pub fn get(c: &Connection, id: &str) -> anyhow::Result<Option<Escrow>> {
    Ok(c.query_row(&format!("SELECT {COLS} FROM escrows WHERE id=?1"), params![id], row).optional()?)
}

pub fn for_party(c: &Connection, identity: &str) -> anyhow::Result<Vec<Escrow>> {
    let mut st = c.prepare(&format!("SELECT {COLS} FROM escrows WHERE payer=?1 OR payee=?1 ORDER BY locked_at DESC"))?;
    let v = st.query_map(params![identity], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// The `contract:agreement` entry and its bounds.
struct ContractGrant {
    cap_minor: i64,
    currency: String,
    counterparties: Option<Vec<String>>,
    max_duration_secs: Option<i64>,
}

fn contract_grant(scopes: &[ScopeEntry]) -> Result<ContractGrant, EscrowError> {
    for e in scopes {
        let ScopeEntry::Parameterized(p) = e else { continue };
        if p.scope != CONTRACT_SCOPE {
            continue;
        }
        let cap = p.cap.as_ref().ok_or_else(|| err("no_grant", "contract scope without a cap"))?;
        let cap_minor = cap.amount_minor().ok_or_else(|| err("no_grant", "malformed cap amount"))? as i64;
        let max_duration_secs = match &p.max_duration {
            Some(d) => Some(parse_iso_duration_secs(d).ok_or_else(|| err("no_grant", "malformed max_duration"))? as i64),
            None => None,
        };
        return Ok(ContractGrant { cap_minor, currency: cap.currency.clone(), counterparties: p.counterparties.clone(), max_duration_secs });
    }
    Err(EscrowError { error: "no_grant", detail: None })
}

fn covers(counterparties: &Option<Vec<String>>, other: &str) -> bool {
    let Some(list) = counterparties else { return true };
    let domain = other.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
    list.iter().any(|c| c == other || c == "*" || c.strip_prefix("*@").is_some_and(|d| d.eq_ignore_ascii_case(domain)))
}

/// A money field: a decimal string, or `{ "amount": "50.00", "currency": … }`
/// (the terms' own shape, which the SDK forwards verbatim).
fn money(v: &Value, what: &str) -> Result<i64, EscrowError> {
    let s = v
        .as_str()
        .or_else(|| v.get("amount").and_then(|a| a.as_str()))
        .ok_or_else(|| err("bad_request", format!("{what} must be a decimal string or {{amount}}")))?;
    Ok(parse_money_minor(s).ok_or_else(|| err("bad_request", format!("{what} malformed")))? as i64)
}

#[derive(Deserialize)]
pub struct LockRequest {
    pub presentation: String,
    pub proposal: Value,
    pub price: Value,
    #[serde(default)]
    pub fees: Value,
    #[serde(default)]
    pub contribution: Value,
    #[serde(default)]
    pub sealed_terms: Option<String>,
    /// Latest obligation deadline (unix), so `max_duration` can be checked
    /// without reading the sealed terms.
    #[serde(default)]
    pub latest_deadline: Option<i64>,
}

fn proposal_str<'a>(p: &'a Value, path: &[&str]) -> Result<&'a str, EscrowError> {
    let mut v = p;
    for k in path {
        v = v.get(k).ok_or_else(|| err("bad_request", format!("proposal.{}", path.join("."))))?;
    }
    v.as_str().ok_or_else(|| err("bad_request", format!("proposal.{} must be a string", path.join("."))))
}

/// Verify the caller's presentation for the contract audience and return
/// (identity, scope entries).
async fn caller(verify: &Verify, audience: &str, presentation: &str) -> Result<(String, String, Vec<ScopeEntry>, Option<String>), EscrowError> {
    let v = verify.presentation_for(audience, presentation).await.map_err(|e| {
        if e.contains("revoked") { err("revoked", e) } else { err("verification_failed", e) }
    })?;
    let key = v.identity.warrant_status.as_ref().map(|s| format!("{}#{}", s.uri, s.idx));
    Ok((v.identity.email, v.identity.grantee, v.scopes, key))
}

pub async fn lock(
    verify: &Verify,
    db: &Db,
    publisher: &dyn Publisher,
    origin: &str,
    deployment_currency: &str,
    req: LockRequest,
) -> Result<Escrow, EscrowError> {
    let p = &req.proposal;
    let id = proposal_str(p, &["id"])?.to_string();
    if !id.starts_with("agr_") || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(err("bad_request", "proposal.id"));
    }
    let payer = proposal_str(p, &["roles", "payer"])?.to_string();
    let payee = proposal_str(p, &["roles", "payee"])?.to_string();
    let proposer = proposal_str(p, &["parties", "proposer"])?;
    let acceptor = proposal_str(p, &["parties", "acceptor"])?;
    let adjudicator = proposal_str(p, &["adjudicator"])?.to_string();
    let custodian = proposal_str(p, &["custodian"])?;
    if custodian.trim_end_matches('/') != origin.trim_end_matches('/') {
        return Err(err("wrong_custodian", format!("proposal names {custodian}")));
    }
    if !((payer == proposer && payee == acceptor) || (payer == acceptor && payee == proposer)) {
        return Err(err("bad_request", "roles must be the two parties"));
    }
    let audience = contract_audience(&id);
    let (identity, grantee, scopes, warrant_key) = caller(verify, &audience, &req.presentation).await?;
    if identity != payer {
        return Err(err("not_payer", format!("{identity} is not the payer")));
    }
    let Some(warrant_key) = warrant_key else { return Err(err("no_grant", "warrant has no status ref")) };
    let grant = contract_grant(&scopes)?;
    if grant.currency != deployment_currency {
        return Err(err("unsupported_currency", format!("cap is in {}", grant.currency)));
    }
    if !covers(&grant.counterparties, &payee) {
        return Err(err("counterparty_not_allowed", payee.clone()));
    }
    let price = money(&req.price, "price")?;
    let custodian_fee = req.fees.get("custodian").map(|v| money(v, "fees.custodian")).transpose()?.unwrap_or(0);
    let adjudicator_fee = req.fees.get("adjudicator").map(|v| money(v, "fees.adjudicator")).transpose()?.unwrap_or(0);
    let contribution_recipient = req.contribution.get("recipient").and_then(|v| v.as_str()).map(String::from);
    let contribution = match (req.contribution.get("percentage").and_then(|v| v.as_f64()), req.contribution.get("amount")) {
        (_, Some(a)) => money(a, "contribution.amount")?,
        (Some(pct), None) if pct >= 0.0 => ((price as f64) * pct / 100.0).round() as i64,
        _ => 0,
    };
    let total = price + custodian_fee + adjudicator_fee + contribution;
    if price <= 0 {
        return Err(err("bad_request", "price must be positive"));
    }
    if total > grant.cap_minor {
        return Err(err("cap_exceeded", format!("total {} exceeds cap {}", crate::pay::fmt_minor(total), crate::pay::fmt_minor(grant.cap_minor))));
    }
    if let (Some(max), Some(deadline)) = (grant.max_duration_secs, req.latest_deadline) {
        if deadline - db::now() > max {
            return Err(err("duration_exceeded", "latest deadline is beyond max_duration"));
        }
    }
    let now = db::now();
    let escrow = db
        .with(|c| {
            let tx = c.transaction()?;
            if get(&tx, &id)?.is_some() {
                return Ok(Err(err("already_locked", id.clone())));
            }
            let reference = format!("escrow:{id}");
            if Db::adjust(&tx, &payer, deployment_currency, -total, "escrow_lock", Some(&reference)).is_err() {
                return Ok(Err(EscrowError { error: "insufficient_funds", detail: None }));
            }
            tx.execute(
                "INSERT INTO escrows(id, state, payer, payee, adjudicator, price_minor, custodian_fee_minor, adjudicator_fee_minor, contribution_minor, contribution_recipient, currency, sealed_terms, proposal, warrant_key, locked_at)
                 VALUES(?1,'locked',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![id, payer, payee, adjudicator, price, custodian_fee, adjudicator_fee, contribution, contribution_recipient, deployment_currency, req.sealed_terms, p.to_string(), warrant_key, now],
            )?;
            Db::insert_receipt(
                &tx,
                &Receipt { id: reference, kind: "escrow:lock".into(), grantor: payer.clone(), grantee: grantee.clone(), warrant_key: Some(warrant_key.clone()), amount_minor: total, currency: deployment_currency.into(), payee: Some(payee.clone()), memo: Some(id.clone()), at: now },
            )?;
            let e = get(&tx, &id)?.expect("just inserted");
            tx.commit()?;
            Ok(Ok(e))
        })
        .map_err(|e| err("internal", e.to_string()))??;
    publisher.lock(&id, now).await;
    Ok(escrow)
}

pub fn contract_audience(id: &str) -> String {
    format!("sbo+raw://avail:turing:530/agreements/{id}")
}

/// Payer acknowledges delivery: price → payee, fees → custodian and (unused)
/// adjudicator fee back to payer, contribution → recipient.
pub async fn acknowledge(verify: &Verify, db: &Db, publisher: &dyn Publisher, id: &str, presentation: &str) -> Result<Escrow, EscrowError> {
    let (identity, _, _, _) = caller(verify, &contract_audience(id), presentation).await?;
    let e = settle(db, id, |e| {
        if identity != e.payer {
            return Err(err("not_payer", identity.clone()));
        }
        if e.state != "locked" && e.state != "disputed" {
            return Err(err("bad_state", e.state.clone()));
        }
        Ok(Settlement { to_payee: e.price_minor, to_payer: e.adjudicator_fee_minor, to_adjudicator: 0, outcome: "acknowledged".into() })
    })?;
    publish_settled(publisher, &e).await;
    Ok(e)
}

/// Either party opens a dispute: the escrow freezes and the sealed terms are
/// handed back for delivery to the adjudicator.
pub async fn dispute(verify: &Verify, db: &Db, id: &str, presentation: &str) -> Result<(Escrow, Option<String>), EscrowError> {
    let (identity, _, _, _) = caller(verify, &contract_audience(id), presentation).await?;
    db.with(|c| {
        let Some(e) = get(c, id)? else { return Ok(Err(err("not_found", id))) };
        if identity != e.payer && identity != e.payee {
            return Ok(Err(err("not_party", identity)));
        }
        if e.state != "locked" {
            return Ok(Err(err("bad_state", e.state)));
        }
        c.execute("UPDATE escrows SET state='disputed' WHERE id=?1", params![id])?;
        let e = get(c, id)?.expect("exists");
        let sealed = e.sealed_terms.clone();
        Ok(Ok((e, sealed)))
    })
    .map_err(|e| err("internal", e.to_string()))?
}

#[derive(Deserialize)]
pub struct RulingRequest {
    pub presentation: String,
    pub ruling: Value,
}

/// The named adjudicator rules; honored once, only from that identity,
/// authenticated by its own presentation for the custodian's audience.
pub async fn ruling(verify: &Verify, db: &Db, publisher: &dyn Publisher, origin: &str, id: &str, req: RulingRequest) -> Result<Escrow, EscrowError> {
    let (identity, _, _, _) = caller(verify, origin, &req.presentation).await?;
    let outcome = req.ruling.get("outcome").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let split = req.ruling.get("split").cloned();
    let fee = req.ruling.get("fee").and_then(|v| v.as_str()).unwrap_or("escrow").to_string();
    let ruling_json = req.ruling.to_string();
    let db2 = db.clone();
    let idc = id.to_string();
    let r = settle(db, id, |e| {
        if identity != e.adjudicator {
            return Err(err("not_adjudicator", identity.clone()));
        }
        if e.state != "disputed" && e.state != "locked" {
            return Err(err("bad_state", e.state.clone()));
        }
        let (to_payee, to_payer) = match outcome.as_str() {
            "payee" => (e.price_minor, 0),
            "payer" => (0, e.price_minor),
            "split" => {
                let s = split.as_ref().ok_or_else(|| err("bad_request", "split missing"))?;
                let a = money(s.get("payee").unwrap_or(&Value::Null), "split.payee")?;
                let b = money(s.get("payer").unwrap_or(&Value::Null), "split.payer")?;
                if a + b != e.price_minor {
                    return Err(err("bad_request", "split must sum to the price"));
                }
                (a, b)
            }
            _ => return Err(err("bad_request", "outcome")),
        };
        // The adjudicator fee: from escrow by default; "payer"/"payee" shifts
        // it onto that party's share (never beyond what is escrowed).
        let (to_payee, to_payer, to_adjudicator) = match fee.as_str() {
            "payee" => (to_payee - e.adjudicator_fee_minor.min(to_payee), to_payer + e.adjudicator_fee_minor, e.adjudicator_fee_minor),
            "payer" => (to_payee + e.adjudicator_fee_minor, to_payer - e.adjudicator_fee_minor.min(to_payer), e.adjudicator_fee_minor),
            _ => (to_payee, to_payer, e.adjudicator_fee_minor),
        };
        Ok(Settlement { to_payee, to_payer, to_adjudicator, outcome: format!("ruled:{outcome}") })
    })?;
    db2.with(|c| {
        c.execute("UPDATE escrows SET ruling=?2 WHERE id=?1", params![idc, ruling_json])?;
        Ok(())
    })
    .map_err(|e| err("internal", e.to_string()))?;
    publish_settled(publisher, &r).await;
    Ok(r)
}

struct Settlement {
    to_payee: i64,
    to_payer: i64,
    to_adjudicator: i64,
    outcome: String,
}

fn settle(db: &Db, id: &str, decide: impl FnOnce(&Escrow) -> Result<Settlement, EscrowError>) -> Result<Escrow, EscrowError> {
    let now = db::now();
    let out = db
        .with(|c| {
            let tx = c.transaction()?;
            let Some(e) = get(&tx, id)? else { return Ok(Err(err("not_found", id))) };
            let s = match decide(&e) {
                Ok(s) => s,
                Err(x) => return Ok(Err(x)),
            };
            let total = e.price_minor + e.custodian_fee_minor + e.adjudicator_fee_minor + e.contribution_minor;
            let paid = s.to_payee + s.to_payer + s.to_adjudicator + e.custodian_fee_minor + e.contribution_minor;
            anyhow::ensure!(paid == total, "settlement does not balance: {paid} vs {total}");
            let reference = format!("escrow:{id}");
            let cur = e.currency.clone();
            let pay_out = |to: &str, amt: i64, kind: &str| -> anyhow::Result<()> {
                if amt > 0 {
                    Db::adjust(&tx, to, &cur, amt, kind, Some(&reference))?;
                }
                Ok(())
            };
            pay_out(&e.payee, s.to_payee, "escrow_release")?;
            pay_out(&e.payer, s.to_payer, "escrow_refund")?;
            pay_out(&e.adjudicator, s.to_adjudicator, "adjudicator_fee")?;
            pay_out("custodian", e.custodian_fee_minor, "custodian_fee")?;
            if let Some(r) = &e.contribution_recipient {
                pay_out(r, e.contribution_minor, "contribution")?;
            }
            tx.execute(
                "UPDATE escrows SET state='settled', settled_at=?2, outcome=?3 WHERE id=?1",
                params![id, now, s.outcome],
            )?;
            Db::insert_receipt(
                &tx,
                &Receipt { id: format!("{reference}:settle"), kind: format!("escrow:{}", s.outcome), grantor: e.payer.clone(), grantee: e.payer.clone(), warrant_key: None, amount_minor: s.to_payee, currency: cur, payee: Some(e.payee.clone()), memo: Some(id.to_string()), at: now },
            )?;
            let e = get(&tx, id)?.expect("exists");
            tx.commit()?;
            Ok(Ok(e))
        })
        .map_err(|e| err("internal", e.to_string()))??;
    Ok(out)
}

async fn publish_settled(publisher: &dyn Publisher, e: &Escrow) {
    publisher.settled(&e.id, (&e.payer, &e.payee), e.outcome.as_deref().unwrap_or("")).await;
}

pub fn view(e: &Escrow) -> Value {
    json!({
        "id": e.id, "state": e.state, "payer": e.payer, "payee": e.payee, "adjudicator": e.adjudicator,
        "price": crate::pay::fmt_minor(e.price_minor),
        "fees": { "custodian": crate::pay::fmt_minor(e.custodian_fee_minor), "adjudicator": crate::pay::fmt_minor(e.adjudicator_fee_minor) },
        "contribution": { "amount": crate::pay::fmt_minor(e.contribution_minor), "recipient": e.contribution_recipient },
        "currency": e.currency, "locked_at": e.locked_at, "settled_at": e.settled_at, "outcome": e.outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use browserid_core::device::{AccessCert, Binding, Cap, DeviceCert, Holder, HolderMatcher, Purpose, ScopeParams, Warrant};
    use browserid_core::{Assertion, KeyPair, StatusRef};
    use chrono::Duration;

    const ORIGIN: &str = "https://pay.test";
    const ISSUER: &str = "idp.test";

    fn cfg(issuer_kp: &KeyPair) -> Config {
        Config {
            port: 0, origin: ORIGIN.into(), database_path: ":memory:".into(), admin_token: "t".into(),
            broker: "https://idp.test".into(), accepted_fallbacks: vec![ISSUER.into()], session_secret: "s".into(),
            dev: true, trust_issuers: vec![(ISSUER.into(), issuer_kp.public_key().to_base64())], currency: "USD".into(),
            data_dir: String::new(), chain_audience: String::new(), handle: String::new(), fee: "0.50".into(), daemon: String::new(), repo: String::new(),
        }
    }

    fn presentation(issuer_kp: &KeyPair, grantor: &str, grantee: &str, audience: &str, scopes: Vec<ScopeEntry>, idx: u64) -> String {
        let holder = "ag.k1";
        let access_kp = KeyPair::generate();
        let config_kp = KeyPair::generate();
        let access_cert = AccessCert::create(ISSUER, grantee, Holder::new(holder).unwrap(), &access_kp.public_key(), Duration::hours(1), issuer_kp, None).unwrap();
        let config_cert = DeviceCert::create(ISSUER, &config_kp.public_key(), Purpose::Authorization, Holder::new("br.main").unwrap(), vec![grantor.to_string()], Duration::days(90), issuer_kp, None).unwrap();
        let warrant = Warrant::create_v2(grantor, grantee, Binding::Holder { matcher: HolderMatcher::new(holder).unwrap() }, audience, scopes, Duration::days(30), &config_kp, StatusRef { uri: "https://idp.test/status".into(), idx }).unwrap();
        let assertion = Assertion::create(audience, Duration::minutes(5), &access_kp).unwrap();
        format!("{}~{}~{}~{}", access_cert.encoded(), assertion.encoded(), warrant.encoded(), config_cert.encoded())
    }

    fn contract_scope(amount: &str, counterparties: Option<Vec<&str>>) -> ScopeEntry {
        ScopeEntry::Parameterized(ScopeParams {
            scope: CONTRACT_SCOPE.into(), mode: None,
            cap: Some(Cap { amount: amount.into(), currency: "USD".into(), window: None }),
            counterparties: counterparties.map(|v| v.into_iter().map(String::from).collect()),
            max_duration: None,
        })
    }

    fn proposal(id: &str) -> Value {
        json!({ "v": 1, "id": id, "mode": "commitment", "commitment": "sha256:00",
            "parties": { "proposer": "alice@x.test", "acceptor": "bob@y.test" },
            "roles": { "payer": "alice@x.test", "payee": "bob@y.test" },
            "adjudicator": "judge@dsp.test", "custodian": ORIGIN,
            "contribution": { "recipient": "fund@dsp.test", "percentage": 1.0 }, "expires": 4_000_000_000i64 })
    }

    struct Setup { issuer: KeyPair, db: Db, verify: Verify }
    fn setup() -> Setup {
        let issuer = KeyPair::generate();
        let cfg = cfg(&issuer);
        let db = Db::open(":memory:").unwrap();
        db.with(|c| schema(c)).unwrap();
        db.with(|c| Db::adjust(c, "alice@x.test", "USD", 10_000, "credit", None)).unwrap();
        let verify = Verify::new(&cfg).unwrap();
        Setup { issuer, db, verify }
    }

    fn lock_req(s: &Setup, id: &str, cap: &str, counterparties: Option<Vec<&str>>) -> LockRequest {
        let aud = contract_audience(id);
        LockRequest {
            presentation: presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &aud, vec![contract_scope(cap, counterparties)], 1),
            proposal: proposal(id), price: json!("50.00"), fees: json!({ "custodian": "0.50", "adjudicator": "5.00" }),
            contribution: json!({ "recipient": "fund@dsp.test", "percentage": 1.0 }), sealed_terms: Some("sealed".into()), latest_deadline: None,
        }
    }

    fn bal(s: &Setup, who: &str) -> i64 { s.db.account(who).unwrap().map(|a| a.balance_minor).unwrap_or(0) }

    #[tokio::test]
    async fn lock_then_acknowledge_pays_everyone() {
        let s = setup();
        let e = lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_1", "60.00", None)).await.unwrap();
        assert_eq!(e.state, "locked");
        // 50 + 0.50 + 5 + 1% of 50 (0.50) = 56.00 locked
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600);
        // Locking twice is refused.
        assert_eq!(lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_1", "60.00", None)).await.err().unwrap().error, "already_locked");

        let aud = contract_audience("agr_1");
        let ack = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &aud, vec![contract_scope("60.00", None)], 1);
        let e = acknowledge(&s.verify, &s.db, &Log, "agr_1", &ack).await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(e.outcome.as_deref(), Some("acknowledged"));
        assert_eq!(bal(&s, "bob@y.test"), 5_000);
        assert_eq!(bal(&s, "custodian"), 50);
        assert_eq!(bal(&s, "fund@dsp.test"), 50);
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600 + 500, "unused adjudicator fee returned");
        assert_eq!(bal(&s, "judge@dsp.test"), 0);
    }

    #[tokio::test]
    async fn cap_and_counterparties_bound_the_lock() {
        let s = setup();
        assert_eq!(lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_2", "55.00", None)).await.err().unwrap().error, "cap_exceeded");
        assert_eq!(lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_2", "60.00", Some(vec!["*@z.test"]))).await.err().unwrap().error, "counterparty_not_allowed");
        lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_2", "60.00", Some(vec!["*@y.test"]))).await.unwrap();
        // The payee cannot lock (not the payer), even with a grant.
        let aud = contract_audience("agr_3");
        let mut r = lock_req(&s, "agr_3", "60.00", None);
        r.presentation = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", &aud, vec![contract_scope("60.00", None)], 2);
        assert_eq!(lock(&s.verify, &s.db, &Log, ORIGIN, "USD", r).await.err().unwrap().error, "not_payer");
    }

    #[tokio::test]
    async fn dispute_then_ruling_only_from_the_named_adjudicator() {
        let s = setup();
        lock(&s.verify, &s.db, &Log, ORIGIN, "USD", lock_req(&s, "agr_4", "60.00", None)).await.unwrap();
        let aud = contract_audience("agr_4");
        let bob = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", &aud, vec![contract_scope("60.00", None)], 3);
        let (e, sealed) = dispute(&s.verify, &s.db, "agr_4", &bob).await.unwrap();
        assert_eq!(e.state, "disputed");
        assert_eq!(sealed.as_deref(), Some("sealed"));
        // A stranger's ruling is refused; the named adjudicator's is honored once.
        let stranger = presentation(&s.issuer, "someone@z.test", "someone@z.test", ORIGIN, vec![ScopeEntry::Bare("login".into())], 4);
        let rr = |p: &str| RulingRequest { presentation: p.into(), ruling: json!({ "outcome": "split", "split": { "payee": "30.00", "payer": "20.00" }, "fee": "escrow", "reasoning": "partial delivery" }) };
        assert_eq!(ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&stranger)).await.err().unwrap().error, "not_adjudicator");
        let judge = presentation(&s.issuer, "judge@dsp.test", "judge@dsp.test", ORIGIN, vec![ScopeEntry::Bare("login".into())], 5);
        let e = ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&judge)).await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(e.outcome.as_deref(), Some("ruled:split"));
        assert_eq!(bal(&s, "bob@y.test"), 3_000);
        assert_eq!(bal(&s, "judge@dsp.test"), 500);
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600 + 2_000);
        assert_eq!(ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&judge)).await.err().unwrap().error, "bad_state");
    }
}
