//! Escrow: a payment warrant with a release condition (agreements draft,
//! `docs/specs/agreement-v1.md`). The payer's agent locks price + fees +
//! contribution against a contract; release happens on the payer's
//! acknowledgement, on the named adjudicator's ruling, or (later) on a
//! deadline. The custodian never parses the private terms; it holds the
//! sealed copy for the adjudicator.

use crate::db::{self, Db, Receipt, WarrantRow};
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
    async fn lock(&self, contract_id: &str, locked_at: i64, accounts: (&str, &str));
    /// `price_minor`/`currency` feed the attestation's value band (never the
    /// amount); `parties` = (payer, payee), the acting identities.
    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str, price_minor: i64, currency: &str);
    /// The custodian's own record that it accepted a replacement adjudicator.
    /// The parties' nominations are already on chain; this says the swap was
    /// honoured, so a ruling by someone the proposal never named is explicable
    /// from the ledger alone.
    async fn adjudicator_replaced(&self, contract_id: &str, from: &str, to: &str);
}
#[allow(dead_code)]
pub struct Log;
#[async_trait::async_trait]
impl Publisher for Log {
    async fn adjudicator_replaced(&self, contract_id: &str, from: &str, to: &str) {
        tracing::info!(contract_id, from, to, "adjudicator replaced (publisher: log only)");
    }
    async fn lock(&self, contract_id: &str, locked_at: i64, accounts: (&str, &str)) {
        let _ = accounts;
        tracing::info!(contract_id, locked_at, "escrow lock (publisher: log only)");
    }
    async fn settled(&self, contract_id: &str, parties: (&str, &str), outcome: &str, price_minor: i64, currency: &str) {
        tracing::info!(contract_id, payer = parties.0, payee = parties.1, outcome, price_minor, currency, "escrow settled (publisher: log only)");
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Escrow {
    pub id: String,
    pub state: String,
    /// The acting identities named in the proposal (the agents).
    pub payer: String,
    pub payee: String,
    /// Whose money: the grantor behind each party's contract warrant.
    pub payer_account: String,
    pub payee_account: String,
    /// The reputation cards the custodian saw at lock (when a grant required
    /// one), as JSON: `{ "payee": card, "payer": card }`.
    pub reputation_seen: Option<String>,
    pub adjudicator: String,
    pub price_minor: i64,
    pub custodian_fee_minor: i64,
    pub adjudicator_fee_minor: i64,
    pub contribution_minor: i64,
    pub contribution_recipient: Option<String>,
    pub currency: String,
    pub locked_at: i64,
    /// When the dispute was filed, if one was.
    pub disputed_at: Option<i64>,
    /// The latest deadline in the terms, as the payer reported it at lock.
    pub latest_deadline: Option<i64>,
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
           ruling TEXT,
           payer_account TEXT NOT NULL DEFAULT '',
           payee_account TEXT NOT NULL DEFAULT '',
           reputation_seen TEXT
         );
         CREATE INDEX IF NOT EXISTS escrows_party ON escrows(payer, payee);",
    )?;
    // Rows created before accounts were separate from parties.
    for col in ["payer_account", "payee_account"] {
        let _ = c.execute(&format!("ALTER TABLE escrows ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"), []);
    }
    let _ = c.execute("ALTER TABLE escrows ADD COLUMN reputation_seen TEXT", []);
    // When the dispute was filed: the adjudicator's ruling window runs from
    // here (browserid-pay-6ai6).
    let _ = c.execute("ALTER TABLE escrows ADD COLUMN disputed_at INTEGER", []);
    // The agreement's own latest deadline: the abandonment backstop scales with
    // it (browserid-pay-ov7g).
    let _ = c.execute("ALTER TABLE escrows ADD COLUMN latest_deadline INTEGER", []);
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
        payer_account: r.get(16)?,
        payee_account: r.get(17)?,
        reputation_seen: r.get(18)?,
        disputed_at: r.get(19)?,
        latest_deadline: r.get(20)?,
    })
}
const COLS: &str = "id, state, payer, payee, adjudicator, price_minor, custodian_fee_minor, adjudicator_fee_minor, contribution_minor, contribution_recipient, currency, sealed_terms, proposal, locked_at, settled_at, outcome, payer_account, payee_account, reputation_seen, disputed_at, latest_deadline";

pub fn get(c: &Connection, id: &str) -> anyhow::Result<Option<Escrow>> {
    Ok(c.query_row(&format!("SELECT {COLS} FROM escrows WHERE id=?1"), params![id], row).optional()?)
}

pub fn for_party(c: &Connection, identity: &str) -> anyhow::Result<Vec<Escrow>> {
    let mut st = c.prepare(&format!("SELECT {COLS} FROM escrows WHERE payer=?1 OR payee=?1 OR payer_account=?1 OR payee_account=?1 ORDER BY locked_at DESC"))?;
    let v = st.query_map(params![identity], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// The `contract:agreement` entry and its bounds.
struct ContractGrant {
    min_reputation: Option<browserid_core::MinReputation>,
    cap_minor: i64,
    currency: String,
    counterparties: Option<Vec<String>>,
    max_duration_secs: Option<i64>,
    /// Rolling window on the cap, from the warrant's `iat`. `None` is a
    /// lifetime cap — which for a per-agreement warrant is the whole story,
    /// and for a standing one means "this much, ever".
    window_secs: Option<i64>,
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
        let window_secs = cap.window_secs().map_err(|_| err("no_grant", "malformed cap window"))?.map(|w| w as i64);
        return Ok(ContractGrant { min_reputation: p.min_reputation.clone(), cap_minor, currency: cap.currency.clone(), counterparties: p.counterparties.clone(), max_duration_secs, window_secs });
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
    /// Ignored. The payee's consent and identity now come from the ACCEPTANCE
    /// OBJECT on chain, whose attribution proves who wrote it — a presentation
    /// embedded in a permanent object carries a five-minute assertion and
    /// cannot be verified later (browserid-pay-3a8p). Kept so older clients
    /// still parse.
    #[serde(default)]
    pub acceptance_bundle: Option<String>,
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

/// Verify a presentation for `audience` and return
/// (grantor = the account behind it, grantee = the acting identity, scope
/// entries, warrant key). Parties in a proposal are acting identities
/// (agents); money belongs to their grantors.
async fn caller(verify: &Verify, audience: &str, presentation: &str) -> Result<(String, String, Vec<ScopeEntry>, Option<String>), EscrowError> {
    let c = caller_full(verify, audience, presentation).await?;
    Ok((c.account, c.actor, c.scopes, c.warrant_key))
}

/// Like [`caller`], but accepting a standing warrant on the collection as well
/// as one for this agreement. Every contract presentation in the lifecycle —
/// lock, acknowledge, dispute — has to accept both, or a party that approved a
/// blanket grant can open an agreement and then not settle it.
async fn caller_either(verify: &Verify, id: &str, presentation: &str) -> Result<(String, String, Vec<ScopeEntry>, Option<String>), EscrowError> {
    let c = caller_for_contract(verify, &contract_audience(id), presentation).await?;
    Ok((c.account, c.actor, c.scopes, c.warrant_key))
}

/// Everything the lock needs to record a party's grant as a budget.
struct Caller {
    account: String,
    actor: String,
    scopes: Vec<ScopeEntry>,
    warrant_key: Option<String>,
    iat: i64,
    exp: i64,
}

async fn caller_full(verify: &Verify, audience: &str, presentation: &str) -> Result<Caller, EscrowError> {
    let v = verify.presentation_for(audience, presentation).await.map_err(|e| {
        if e.contains("revoked") { err("revoked", e) } else { err("verification_failed", e) }
    })?;
    let warrant_key = v.identity.warrant_status.as_ref().map(|s| format!("{}#{}", s.uri, s.idx));
    Ok(Caller { account: v.identity.email, actor: v.identity.grantee, scopes: v.scopes, warrant_key, iat: v.iat, exp: v.exp })
}

/// Verify a contract presentation against the per-agreement audience, falling
/// back to the standing collection audience. Each party chooses independently:
/// the payer may be acting under a blanket grant while the payee approved this
/// one deal, or the other way round.
async fn caller_for_contract(verify: &Verify, agreement_audience: &str, presentation: &str) -> Result<Caller, EscrowError> {
    match caller_full(verify, agreement_audience, presentation).await {
        Ok(c) => Ok(c),
        Err(narrow) => match caller_full(verify, COLLECTION_AUDIENCE, presentation).await {
            Ok(c) => Ok(c),
            // Report the narrow failure: it is the one the caller most likely
            // meant, and a revocation must not be masked by an audience miss.
            Err(broad) => Err(if narrow.error == "revoked" { narrow } else { broad }),
        },
    }
}

/// A party's `contract:agreement` grant as a budget row: cap, counterparties,
/// pinned to this agreement. What the account page lists under the agent.
fn grant_row(c: &Caller, key: &str, g: &ContractGrant, agreement_id: &str, now: i64) -> WarrantRow {
    WarrantRow {
        key: key.to_string(),
        grantor: c.account.clone(),
        grantee: c.actor.clone(),
        scope: CONTRACT_SCOPE.to_string(),
        cap_minor: g.cap_minor,
        currency: g.currency.clone(),
        window_secs: g.window_secs,
        iat: c.iat,
        exp: c.exp,
        first_seen: now,
        revoked_at: None,
        agreement_id: Some(agreement_id.to_string()),
        counterparties: g.counterparties.as_ref().map(|l| serde_json::to_string(l).unwrap_or_default()),
    }
}

pub async fn lock(
    verify: &Verify,
    db: &Db,
    publisher: &dyn Publisher,
    reputation: &crate::reputation::Reputation,
    origin: &str,
    deployment_currency: &str,
    reader: &dyn chain::ChainReader,
    req: LockRequest,
) -> Result<Escrow, EscrowError> {
    let id = proposal_str(&req.proposal, &["id"])?.to_string();
    if !id.starts_with("agr_") || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(err("bad_request", "proposal.id"));
    }
    // The LEDGER is the source of truth, not what the caller handed us. Both
    // objects are read back and their attribution is the proof of who made
    // them — the same guarantee that made the embedded presentations
    // redundant. A caller cannot lock against a proposal it invented.
    let path = format!("/agreements/{id}/");
    let proposal_obj = reader.get_object(&path, "proposal").await
        .map_err(|e| err("chain_unavailable", e.to_string()))?
        .ok_or_else(|| err("no_proposal", id.clone()))?;
    let acceptance_obj = reader.get_object(&path, "acceptance").await
        .map_err(|e| err("chain_unavailable", e.to_string()))?
        .ok_or_else(|| err("not_accepted", "no acceptance on chain yet"))?;
    let p = &proposal_obj.value;
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
    // Either authority is valid: a warrant for THIS agreement, or a standing
    // warrant on the collection. Try the narrow one first so a one-off
    // approval is preferred when the human gave one.
    let audience = contract_audience(&id);
    let pc = caller_for_contract(verify, &audience, &req.presentation).await?;
    let (payer_account, actor) = (pc.account.clone(), pc.actor.clone());
    if actor != payer {
        return Err(err("not_payer", format!("{actor} is not the payer")));
    }
    let Some(warrant_key) = pc.warrant_key.clone() else { return Err(err("no_grant", "warrant has no status ref")) };
    let grant = contract_grant(&pc.scopes)?;
    // Who made each object is settled by the chain's attribution: the daemon
    // will not accept a write attributed to an identity the writer cannot act
    // for. So the proposal proves the proposer, the acceptance proves the
    // acceptor, and neither needs to carry a credential.
    if proposal_obj.creator != proposer {
        return Err(err("bad_attribution", format!("proposal was written by {}, not {proposer}", proposal_obj.creator)));
    }
    if acceptance_obj.creator != acceptor {
        return Err(err("bad_attribution", format!("acceptance was written by {}, not {acceptor}", acceptance_obj.creator)));
    }
    // No proposal-hash check: the proposal is read from the chain, and a
    // (path, id) slot holds exactly one first-valid write, so there is no other
    // version for the acceptance to have referenced. The hash mattered only
    // while the caller supplied the proposal.
    // The payee's account: the human behind the accepting identity. Resolved
    // from a warrant this custodian has already recorded for them — which is
    // the same as saying they must have an account here to be paid into.
    let payee_account = db
        .with(|c| Ok(db::account_for_identity(c, &payee)?))
        .map_err(|e: anyhow::Error| err("internal", e.to_string()))?
        .ok_or_else(|| err("payee_has_no_account", format!("{payee} has never presented a grant to this custodian")))?;
    // The acceptor's own requirements travel on their acceptance object, which
    // is attributed to them, so it is their statement in the same way a scope
    // parameter was.
    let payee_min_reputation: Option<browserid_core::MinReputation> = acceptance_obj
        .value
        .get("min_reputation")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    if grant.currency != deployment_currency {
        return Err(err("unsupported_currency", format!("cap is in {}", grant.currency)));
    }
    if !covers(&grant.counterparties, &payee) {
        return Err(err("counterparty_not_allowed", payee.clone()));
    }
    // Reputation-gated grants: each side's `min_reputation` (if any) is
    // checked against the OTHER party's signed card at the indexer that side's
    // grantor named. Composes with `counterparties` (both must hold).
    let mut seen = serde_json::Map::new();
    for (whose, mr_opt, other, key) in [("payer", &grant.min_reputation, &payee, "payee"), ("payee", &payee_min_reputation, &payer, "payer")] {
        if let Some(mr) = mr_opt {
            match reputation.require(&mr.indexer, other, mr.score).await {
                Ok(card) => { seen.insert(key.to_string(), card); }
                Err(crate::reputation::ReputationError::Below { score, threshold, indexer }) => {
                    return Err(err("reputation_below_minimum", format!("{other} is rated {score} at {indexer}; the {whose}'s grant requires {threshold}")));
                }
                Err(crate::reputation::ReputationError::Unavailable(d)) => {
                    return Err(err("reputation_unavailable", d));
                }
            }
        }
    }
    let reputation_seen = if seen.is_empty() { None } else { Some(Value::Object(seen).to_string()) };
    let price = money(&req.price, "price")?;
    let custodian_fee = req.fees.get("custodian").map(|v| money(v, "fees.custodian")).transpose()?.unwrap_or(0);
    let adjudicator_fee = req.fees.get("adjudicator").map(|v| money(v, "fees.adjudicator")).transpose()?.unwrap_or(0);
    let contribution_recipient = req.contribution.get("recipient").and_then(|v| v.as_str()).map(String::from);
    let contribution = contribution_minor(&req.contribution, price)?;
    let total = price + custodian_fee + adjudicator_fee + contribution;
    if price <= 0 {
        return Err(err("bad_request", "price must be positive"));
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
            // The cap binds ACROSS agreements under the same warrant, not just
            // this one — otherwise a standing "$200 a month" would silently
            // mean "$200 per agreement". Inside the transaction, so two locks
            // racing cannot both read a stale total and both pass.
            let window_from = db::window_start(pc.iat, grant.window_secs, now);
            let already = db::locked_since(&tx, &warrant_key, window_from)?;
            if already + total > grant.cap_minor {
                let remaining = (grant.cap_minor - already).max(0);
                return Ok(Err(err("cap_exceeded", if already > 0 {
                    format!("total {} exceeds the {} left of the {} cap ({} already committed in this window)",
                        crate::pay::fmt_minor(total), crate::pay::fmt_minor(remaining),
                        crate::pay::fmt_minor(grant.cap_minor), crate::pay::fmt_minor(already))
                } else {
                    format!("total {} exceeds cap {}", crate::pay::fmt_minor(total), crate::pay::fmt_minor(grant.cap_minor))
                })));
            }
            let reference = format!("escrow:{id}");
            if Db::adjust(&tx, &payer_account, deployment_currency, -total, "escrow_lock", Some(&reference)).is_err() {
                return Ok(Err(EscrowError { error: "insufficient_funds", detail: None }));
            }
            tx.execute(
                "INSERT INTO escrows(id, state, payer, payee, adjudicator, price_minor, custodian_fee_minor, adjudicator_fee_minor, contribution_minor, contribution_recipient, currency, sealed_terms, proposal, warrant_key, locked_at, payer_account, payee_account, reputation_seen, latest_deadline)
                 VALUES(?1,'locked',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
                params![id, payer, payee, adjudicator, price, custodian_fee, adjudicator_fee, contribution, contribution_recipient, deployment_currency, req.sealed_terms, p.to_string(), warrant_key, now, payer_account, payee_account, reputation_seen, req.latest_deadline],
            )?;
            Db::insert_receipt(
                &tx,
                &Receipt { id: reference.clone(), kind: "escrow:lock".into(), grantor: payer_account.clone(), grantee: actor.clone(), warrant_key: Some(warrant_key.clone()), amount_minor: total, currency: deployment_currency.into(), payee: Some(payee.clone()), memo: Some(id.clone()), at: now },
            )?;
            // Both parties' agreement grants show up as budgets under their
            // agents; the lock is the payer's spend against its cap.
            Db::upsert_warrant(&tx, &grant_row(&pc, &warrant_key, &grant, &id, now))?;
            Db::record_spend(&tx, &warrant_key, total, &reference)?;
            // The payee's grant is no longer presented at lock, so there is no
            // budget row to record for them here; it appears the next time they
            // present one (settling, disputing). The payer's is recorded above.
            let e = get(&tx, &id)?.expect("just inserted");
            tx.commit()?;
            Ok(Ok(e))
        })
        .map_err(|e| err("internal", e.to_string()))??;
    publisher.lock(&id, now, (&payer_account, &payee_account)).await;
    Ok(escrow)
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub presentation: String,
    /// Which agreement's audience the presentation was minted for, when it is a
    /// per-agreement warrant. Omit for a standing one.
    #[serde(default)]
    pub agreement: Option<String>,
}

/// Record the identity → account mapping from a presented contract grant, so
/// the payee can be paid later without presenting anything (their consent by
/// then is the attributed acceptance on chain). Idempotent.
pub async fn register(verify: &Verify, db: &Db, req: RegisterRequest) -> Result<Value, EscrowError> {
    let audience = req.agreement.as_deref().map(contract_audience).unwrap_or_else(|| COLLECTION_AUDIENCE.to_string());
    let c = caller_for_contract(verify, &audience, &req.presentation).await?;
    let grant = contract_grant(&c.scopes)?;
    let Some(key) = c.warrant_key.clone() else { return Err(err("no_grant", "warrant has no status ref")) };
    let now = db::now();
    db.with(|conn| Db::upsert_warrant(conn, &grant_row(&c, &key, &grant, "", now)))
        .map_err(|e: anyhow::Error| err("internal", e.to_string()))?;
    Ok(serde_json::json!({ "identity": c.actor, "account": c.account }))
}

/// The audience a warrant for ONE agreement names.
pub fn contract_audience(id: &str) -> String {
    format!("{COLLECTION_AUDIENCE}/{id}")
}

/// The audience a STANDING warrant names: the agreements collection on the
/// ledger, not one agreement and not one custodian. A grant here covers every
/// agreement its parameters admit — `counterparties`, `cap` (with a window),
/// `min_reputation`, `max_duration` — so a human approves the shape of the
/// deals once instead of each deal.
pub const COLLECTION_AUDIENCE: &str = "sbo+raw://avail:turing:530/agreements";

/// Payer acknowledges delivery: price → payee, fees → custodian and (unused)
/// adjudicator fee back to payer, contribution → recipient.
pub async fn acknowledge(verify: &Verify, db: &Db, publisher: &dyn Publisher, id: &str, presentation: &str) -> Result<Escrow, EscrowError> {
    let (_, actor, _, _) = caller_either(verify, id, presentation).await?;
    let e = settle(db, id, |e| {
        if actor != e.payer {
            return Err(err("not_payer", actor.clone()));
        }
        if e.state != "locked" && e.state != "disputed" {
            return Err(err("bad_state", e.state.clone()));
        }
        Ok(Settlement { to_payee: e.price_minor, to_payer: e.adjudicator_fee_minor, to_adjudicator: 0, outcome: "acknowledged".into() })
    })?;
    publish_settled(publisher, &e).await;
    Ok(e)
}

/// The abandonment backstop: how long an escrow may sit after its clock ran out
/// with neither party doing anything, before it is split 50/50.
///
/// Scales with the agreement (twice its delivery time) because a long job
/// deserves a long grace, but bounded at both ends. The FLOOR matters because a
/// six-minute agreement must not get a six-minute backstop; the CEILING because
/// the custodian is carrying the funds and the risk the whole time.
///
/// 50/50 rather than a refund, deliberately. A refund would mean "take the work,
/// go quiet, get your money back" — the stalling payoff this protocol keeps
/// removing. An arbitrary split denies BOTH sides a win for disappearing, which
/// is the property that matters when it fires with nobody watching.
const BACKSTOP_FLOOR_SECS: i64 = 86_400;
const BACKSTOP_CEILING_SECS: i64 = 30 * 86_400;

/// When the backstop falls due for an escrow, or `None` when it cannot yet.
///
/// Returns `None` for anything DISPUTED: a dispute pauses this entirely. Without
/// that, attacking the adjudicator would be a strategy — stall the ruling, let
/// this expire, collect half of what you were going to lose outright.
pub fn backstop_due(e: &Escrow, clock_expired_at: i64) -> Option<i64> {
    if e.state != "locked" {
        return None;
    }
    let agreement_secs = e.latest_deadline.map(|d| (d - e.locked_at).max(0)).unwrap_or(0);
    let window = (2 * agreement_secs).clamp(BACKSTOP_FLOOR_SECS, BACKSTOP_CEILING_SECS);
    Some(clock_expired_at + window)
}

/// How long the named adjudicator has to rule once a dispute is filed, before
/// either party may declare it unresponsive. Deliberately longer than the
/// judge's own window for the counterparty to respond, so a judge that is merely
/// deliberating is not swapped out mid-thought — but not much longer: an
/// automated adjudicator should rule in minutes, not days (Dan, 2026-09-17).
/// Must exceed the judge's OWN window for the disputed party to respond (1h), or
/// the parties could declare it unresponsive at the moment it first becomes able
/// to rule.
const RULING_WINDOW_SECS: i64 = 6 * 3600;

/// Replace an adjudicator that never ruled.
///
/// Both parties must have posted a `replace_adjudicator` object naming the SAME
/// identity. Requiring both is what makes this safe: neither side can shop for
/// a friendlier judge on its own. The ruling window must have lapsed first, so a
/// slow-but-alive adjudicator cannot be swapped out by an impatient party.
///
/// The escrowed fee follows whoever rules: the row's `adjudicator` is what
/// `pay_out` pays, so swapping it moves the fee with the work.
pub async fn replace_adjudicator(
    verify: &Verify,
    db: &Db,
    publisher: &dyn Publisher,
    reader: &dyn chain::ChainReader,
    id: &str,
    presentation: &str,
) -> Result<Escrow, EscrowError> {
    let (_, actor, _, _) = caller_either(verify, id, presentation).await?;
    let e = db.with(|c| get(c, id)).map_err(|e| err("internal", e.to_string()))?
        .ok_or_else(|| err("not_found", id))?;
    if actor != e.payer && actor != e.payee {
        return Err(err("not_party", actor));
    }
    if e.state != "disputed" {
        return Err(err("bad_state", format!("{} — only a disputed escrow has an adjudicator to replace", e.state)));
    }
    let since = e.disputed_at.unwrap_or(e.locked_at);
    if db::now() <= since + RULING_WINDOW_SECS {
        return Err(err("ruling_window_open", format!("{} has until {}", e.adjudicator, since + RULING_WINDOW_SECS)));
    }

    // Both parties' nominations, read off the ledger and attributed by it.
    let objs = reader.list_prefix(&format!("/agreements/{id}/evidence/")).await
        .map_err(|x| err("chain_unavailable", x.to_string()))?;
    let nomination = |who: &str| -> Option<String> {
        objs.iter()
            .filter(|o| o.creator == who && o.value.get("kind").and_then(|v| v.as_str()) == Some("replace_adjudicator"))
            .filter_map(|o| o.value.get("adjudicator").and_then(|v| v.as_str()).map(String::from))
            .next_back()
    };
    let (Some(a), Some(b)) = (nomination(&e.payer), nomination(&e.payee)) else {
        return Err(err("not_agreed", "both parties must nominate a replacement"));
    };
    if a != b {
        return Err(err("not_agreed", format!("the parties named different replacements: {a} and {b}")));
    }
    if a == e.adjudicator {
        return Err(err("bad_request", "that is already the adjudicator"));
    }
    db.with(|c| {
        c.execute("UPDATE escrows SET adjudicator=?2 WHERE id=?1", params![id, a])?;
        Ok(())
    })
    .map_err(|x: anyhow::Error| err("internal", x.to_string()))?;
    publisher.adjudicator_replaced(id, &e.adjudicator, &a).await;
    db.with(|c| get(c, id)).map_err(|x| err("internal", x.to_string()))?
        .ok_or_else(|| err("not_found", id))
}

/// Split an abandoned escrow 50/50.
///
/// Nobody is wronged by this — both parties had an exit and neither took it —
/// but the custodian cannot carry the funds for ever, and held funds are a real
/// liability. This is the ONLY place money moves without somebody asking, which
/// is why it is the most conservative clock in the system and why a dispute
/// suspends it outright.
///
/// The non-price parts follow the ordinary settlement shape: the custodian fee
/// is earned (it did hold the funds), the adjudicator fee is refunded because
/// nobody adjudicated, the contribution is paid because it is a term of the
/// agreement. Only the PRICE is split.
pub async fn sweep_abandoned(db: &Db, publisher: &dyn Publisher, id: &str) -> Result<Escrow, EscrowError> {
    let e = settle(db, id, |e| {
        if e.state != "locked" {
            return Err(err("bad_state", e.state.clone()));
        }
        let half = e.price_minor / 2;
        Ok(Settlement {
            to_payee: half,
            // Odd pennies and the unused adjudicator fee go back to the payer,
            // whose money it was.
            to_payer: (e.price_minor - half) + e.adjudicator_fee_minor,
            to_adjudicator: 0,
            outcome: "abandoned".into(),
        })
    })?;
    publish_settled(publisher, &e).await;
    Ok(e)
}

/// Either party opens a dispute: the escrow freezes and the sealed terms are
/// handed back for delivery to the adjudicator.
pub async fn dispute(verify: &Verify, db: &Db, id: &str, presentation: &str) -> Result<(Escrow, Option<String>), EscrowError> {
    let (_, actor, _, _) = caller_either(verify, id, presentation).await?;
    db.with(|c| {
        let Some(e) = get(c, id)? else { return Ok(Err(err("not_found", id))) };
        if actor != e.payer && actor != e.payee {
            return Ok(Err(err("not_party", actor)));
        }
        if e.state != "locked" {
            return Ok(Err(err("bad_state", e.state)));
        }
        c.execute("UPDATE escrows SET state='disputed', disputed_at=?2 WHERE id=?1", params![id, db::now()])?;
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
    let (_, actor, _, _) = caller(verify, origin, &req.presentation).await?;
    let outcome = req.ruling.get("outcome").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let split = req.ruling.get("split").cloned();
    let fee = req.ruling.get("fee").and_then(|v| v.as_str()).unwrap_or("escrow").to_string();
    let ruling_json = req.ruling.to_string();
    let db2 = db.clone();
    let idc = id.to_string();
    let r = settle(db, id, |e| {
        if actor != e.adjudicator {
            return Err(err("not_adjudicator", actor.clone()));
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
            pay_out(&e.payee_account, s.to_payee, "escrow_release")?;
            pay_out(&e.payer_account, s.to_payer, "escrow_refund")?;
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
                &Receipt { id: format!("{reference}:settle"), kind: format!("escrow:{}", s.outcome), grantor: e.payer_account.clone(), grantee: e.payer.clone(), warrant_key: None, amount_minor: s.to_payee, currency: cur, payee: Some(e.payee_account.clone()), memo: Some(id.to_string()), at: now },
            )?;
            let e = get(&tx, id)?.expect("exists");
            tx.commit()?;
            Ok(Ok(e))
        })
        .map_err(|e| err("internal", e.to_string()))??;
    Ok(out)
}

async fn publish_settled(publisher: &dyn Publisher, e: &Escrow) {
    publisher.settled(&e.id, (&e.payer, &e.payee), e.outcome.as_deref().unwrap_or(""), e.price_minor, &e.currency).await;
}

pub fn view(e: &Escrow) -> Value {
    json!({
        "id": e.id, "state": e.state, "payer": e.payer, "payee": e.payee,
        "payer_account": e.payer_account, "payee_account": e.payee_account, "adjudicator": e.adjudicator,
        "reputation": e.reputation_seen.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "price": crate::pay::fmt_minor(e.price_minor),
        "fees": { "custodian": crate::pay::fmt_minor(e.custodian_fee_minor), "adjudicator": crate::pay::fmt_minor(e.adjudicator_fee_minor) },
        "contribution": { "amount": crate::pay::fmt_minor(e.contribution_minor), "recipient": e.contribution_recipient },
        "currency": e.currency, "locked_at": e.locked_at, "settled_at": e.settled_at, "outcome": e.outcome,
    })
}

/// The contribution in minor units, for a price in minor units.
///
/// `fn` combines the two operands — `max` (the usual floor: "1bp, or at least a
/// cent"), `min` (a ceiling on both) — and is REQUIRED when both are present:
/// guessing would silently charge the wrong party. A single operand needs no
/// `fn`. A malformed contribution is a refusal, never a silent zero: charging
/// nothing because a field did not parse is exactly the failure a chain-enforced
/// floor exists to prevent.
///
/// The chain policy bounds the *declared* operands (see the root policy in
/// `crates/genesis`); this is the arithmetic the policy deliberately cannot do,
/// because it needs the price, which is sealed in commitment mode.
pub fn contribution_minor(c: &Value, price_minor: i64) -> Result<i64, EscrowError> {
    let pct = match c.get("percentage") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let p = v.as_f64().filter(|p| p.is_finite() && *p >= 0.0)
                .ok_or_else(|| err("bad_request", "contribution.percentage must be a non-negative number"))?;
            Some(((price_minor as f64) * p / 100.0).ceil() as i64)
        }
    };
    let abs = match c.get("amount") {
        None | Some(Value::Null) => None,
        Some(v) => Some(money(v, "contribution.amount")?),
    };
    match (abs, pct) {
        (None, None) => Ok(0),
        (Some(a), None) => Ok(a),
        (None, Some(p)) => Ok(p),
        (Some(a), Some(p)) => match c.get("fn").and_then(|v| v.as_str()) {
            Some("max") => Ok(a.max(p)),
            Some("min") => Ok(a.min(p)),
            _ => Err(err(
                "bad_request",
                "contribution carries both amount and percentage; fn must be \"max\" or \"min\"",
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrib(v: serde_json::Value, price: i64) -> Result<i64, EscrowError> {
        contribution_minor(&v, price)
    }

    #[test]
    fn contribution_combines_operands_and_refuses_ambiguity() {
        // max(): the floor. 0.01% of $100 is 1 cent; on a $1 deal the absolute wins.
        assert_eq!(contrib(json!({"fn": "max", "amount": "0.01", "percentage": 0.01}), 10000).unwrap(), 1);
        assert_eq!(contrib(json!({"fn": "max", "amount": "0.01", "percentage": 0.01}), 100).unwrap(), 1);
        assert_eq!(contrib(json!({"fn": "max", "amount": "0.01", "percentage": 1.0}), 100000).unwrap(), 1000);
        // min(): a ceiling on both.
        assert_eq!(contrib(json!({"fn": "min", "amount": "0.01", "percentage": 1.0}), 100000).unwrap(), 1);
        // One operand needs no fn.
        assert_eq!(contrib(json!({"percentage": 1.0}), 10000).unwrap(), 100);
        assert_eq!(contrib(json!({"amount": "0.25"}), 10000).unwrap(), 25);
        assert_eq!(contrib(json!({}), 10000).unwrap(), 0);
        // Both operands with no fn: refused, not guessed.
        assert!(contrib(json!({"amount": "0.01", "percentage": 1.0}), 10000).is_err());
    }

    #[test]
    fn a_malformed_contribution_refuses_instead_of_charging_zero() {
        // The old behavior silently fell through to 0 — the exact failure a
        // chain-enforced floor exists to prevent.
        assert!(contrib(json!({"percentage": "lots"}), 10000).is_err());
        assert!(contrib(json!({"percentage": -1.0}), 10000).is_err());
        assert!(contrib(json!({"amount": "free"}), 10000).is_err());
    }

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
            data_dir: String::new(), chain_audience: String::new(), handle: String::new(), fee: "0.50".into(), daemon: String::new(), repo: String::new(), index_origin: String::new(), judge_origin: String::new(),
            test_credit: true, test_credit_max_minor: 50_000, test_credit_ceiling_minor: 100_000,
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
            min_reputation: None,
        })
    }

    fn contract_scope_rated(amount: &str, indexer: &str, score: u8) -> ScopeEntry {
        ScopeEntry::Parameterized(ScopeParams {
            scope: CONTRACT_SCOPE.into(), mode: None,
            cap: Some(Cap { amount: amount.into(), currency: "USD".into(), window: None }),
            counterparties: None, max_duration: None,
            min_reputation: Some(browserid_core::MinReputation { indexer: indexer.into(), score }),
        })
    }

    /// An in-process indexer stub: serves a signed card with `score` for
    /// every subject (or a card signed by the wrong key when `bad_sig`).
    async fn stub_indexer(score: u8, bad_sig: bool) -> (String, KeyPair) {
        use axum::{routing::get, Router, extract::Query, Json};
        let key = KeyPair::generate();
        let signer = std::sync::Arc::new(if bad_sig { KeyPair::generate() } else { KeyPair::from_seed(&key_seed(&key)).unwrap() });
        let pk = key.public_key().to_base64();
        let pk2 = pk.clone();
        let app = Router::new()
            .route("/v1/reputation/key", get(move || { let pk = pk2.clone(); async move { Json(json!({ "alg": "Ed25519", "public_key": pk })) } }))
            .route("/v1/reputation", get(move |Query(q): Query<std::collections::HashMap<String, String>>| {
                let signer = signer.clone();
                async move {
                    let mut card = json!({ "subject": q.get("subject").cloned().unwrap_or_default(), "score": score, "formula": "browserid-pay-reputation-v1", "components": { "tenure": 0.5 } });
                    card["signature"] = json!(crate::reputation::sign_for_tests(&card, &signer));
                    Json(card)
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        (origin, key)
    }
    fn key_seed(k: &KeyPair) -> [u8; 32] { *k.secret_bytes() }

    #[tokio::test]
    async fn min_reputation_gates_the_lock() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        // Bob is rated 61: a grant requiring 40 locks; one requiring 80 is refused.
        let (idx, _) = stub_indexer(61, false).await;
        let mut ok = lock_req(&s, "agr_r1", "60.00", None);
        ok.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &contract_audience("agr_r1"), vec![contract_scope_rated("60.00", &idx, 40)], 1);
        let e = lock_t(&s, &rep, ok).await.unwrap();
        let seen: Value = serde_json::from_str(e.reputation_seen.as_deref().unwrap()).unwrap();
        assert_eq!(seen["payee"]["score"], 61, "the card seen is stored with the escrow");
        let mut low = lock_req(&s, "agr_r2", "60.00", None);
        low.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &contract_audience("agr_r2"), vec![contract_scope_rated("60.00", &idx, 80)], 2);
        let err = lock_t(&s, &rep, low).await.err().unwrap();
        assert_eq!(err.error, "reputation_below_minimum");
        assert!(err.detail.unwrap().contains("rated 61"));
        // A card signed by the wrong key is unavailable, never a pass.
        let (bad, _) = stub_indexer(99, true).await;
        let mut b = lock_req(&s, "agr_r3", "60.00", None);
        b.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &contract_audience("agr_r3"), vec![contract_scope_rated("60.00", &bad, 10)], 3);
        assert_eq!(lock_t(&s, &rep, b).await.err().unwrap().error, "reputation_unavailable");
        // An unreachable indexer is unavailable too.
        let mut u = lock_req(&s, "agr_r4", "60.00", None);
        u.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &contract_audience("agr_r4"), vec![contract_scope_rated("60.00", "http://127.0.0.1:9", 10)], 4);
        assert_eq!(lock_t(&s, &rep, u).await.err().unwrap().error, "reputation_unavailable");
    }

    fn proposal(id: &str) -> Value {
        json!({ "v": 1, "id": id, "mode": "commitment", "commitment": "sha256:00",
            "parties": { "proposer": "alice+agent@x.test", "acceptor": "bob+agent@y.test" },
            "roles": { "payer": "alice+agent@x.test", "payee": "bob+agent@y.test" },
            "adjudicator": "judge+agent@dsp.test", "custodian": ORIGIN,
            "contribution": { "recipient": "fund@dsp.test", "percentage": 1.0 }, "expires": 4_000_000_000i64 })
    }

    /// A chain the tests can put objects on, with the attribution that the
    /// lock now depends on. `creator` is the whole point: it is what proves who
    /// wrote each object, in place of an embedded credential.
    #[derive(Default)]
    struct FakeChain(std::sync::Mutex<Vec<chain::ObjectView>>);
    impl FakeChain {
        fn put(&self, path: &str, id: &str, creator: &str, value: Value) {
            self.0.lock().unwrap().push(chain::ObjectView {
                path: path.into(), id: id.into(), creator: creator.into(), value, confirmed: true,
            });
        }
        /// The usual pair: alice proposed it, bob accepted it.
        fn agreement(id: &str) -> Self {
            let c = Self::default();
            let path = format!("/agreements/{id}/");
            c.put(&path, "proposal", "alice+agent@x.test", proposal(id));
            c.put(&path, "acceptance", "bob+agent@y.test", json!({ "v": 1, "id": id }));
            c
        }
    }
    #[async_trait::async_trait]
    impl chain::ChainReader for FakeChain {
        async fn get_object(&self, path: &str, id: &str) -> anyhow::Result<Option<chain::ObjectView>> {
            Ok(self.0.lock().unwrap().iter().find(|o| o.path == path && o.id == id).cloned())
        }
        async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<chain::ObjectView>> {
            Ok(self.0.lock().unwrap().iter().filter(|o| o.path.starts_with(prefix)).cloned().collect())
        }
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
            acceptance_bundle: Some(presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", &aud, vec![contract_scope("60.00", None)], 9)),
            proposal: proposal(id), price: json!("50.00"), fees: json!({ "custodian": "0.50", "adjudicator": "5.00" }),
            contribution: json!({ "recipient": "fund@dsp.test", "percentage": 1.0 }), sealed_terms: Some("sealed".into()), latest_deadline: None,
        }
    }

    /// Lock against a chain carrying the proposal and acceptance for whatever
    /// agreement the request names — the arrangement every test wants.
    async fn lock_t(s: &Setup, rep: &crate::reputation::Reputation, req: LockRequest) -> Result<Escrow, EscrowError> {
        let id = req.proposal.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        // The payee registers once, the way a real acceptor does at accept
        // time, so the custodian knows which account its earnings belong to.
        register_payee(s, "bob@y.test", "bob+agent@y.test").await;
        lock(&s.verify, &s.db, &Log, rep, ORIGIN, "USD", &FakeChain::agreement(&id), req).await
    }

    async fn register_payee(s: &Setup, account: &str, identity: &str) {
        let pres = presentation(&s.issuer, account, identity, COLLECTION_AUDIENCE, vec![standing_scope("500.00", Some("P30D"), None)], 77);
        register(&s.verify, &s.db, RegisterRequest { presentation: pres, agreement: None }).await.unwrap();
    }

    fn get_state(s: &Setup, id: &str) -> String { s.db.with(|c| get(c, id)).unwrap().unwrap().state }

    fn bal(s: &Setup, who: &str) -> i64 { s.db.account(who).unwrap().map(|a| a.balance_minor).unwrap_or(0) }

    /// A standing grant: audience is the COLLECTION, so the same warrant is
    /// presented for every agreement. `idx` is what identifies the warrant, so
    /// reusing it is what makes the cap accumulate.
    fn standing_scope(amount: &str, window: Option<&str>, counterparties: Option<Vec<&str>>) -> ScopeEntry {
        ScopeEntry::Parameterized(ScopeParams {
            scope: CONTRACT_SCOPE.into(), mode: None,
            cap: Some(Cap { amount: amount.into(), currency: "USD".into(), window: window.map(String::from) }),
            counterparties: counterparties.map(|v| v.into_iter().map(String::from).collect()),
            max_duration: None, min_reputation: None,
        })
    }

    fn standing_lock_req(s: &Setup, id: &str, cap: &str, window: Option<&str>) -> LockRequest {
        LockRequest {
            presentation: presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE, vec![standing_scope(cap, window, None)], 1),
            acceptance_bundle: Some(presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", COLLECTION_AUDIENCE, vec![standing_scope("500.00", window, None)], 9)),
            proposal: proposal(id), price: json!("50.00"), fees: json!({ "custodian": "0.50", "adjudicator": "5.00" }),
            contribution: json!({ "recipient": "fund@dsp.test", "percentage": 1.0 }), sealed_terms: Some("sealed".into()), latest_deadline: None,
        }
    }

    #[tokio::test]
    async fn a_standing_grant_covers_many_agreements_until_its_cap_runs_out() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();
        // One warrant on the collection, no per-agreement approval anywhere.
        // Each agreement costs 56.00, so a 150.00 cap admits exactly two.
        for id in ["agr_s1", "agr_s2"] {
            let e = lock_t(&s, &rep, standing_lock_req(&s, id, "150.00", Some("P30D"))).await.unwrap();
            assert_eq!(e.state, "locked", "{id} should lock under the standing grant");
        }
        // The third exceeds what is left, and the error says how much is left.
        let e = lock_t(&s, &rep, standing_lock_req(&s, "agr_s3", "150.00", Some("P30D"))).await.err().unwrap();
        assert_eq!(e.error, "cap_exceeded");
        let d = e.detail.unwrap_or_default();
        assert!(d.contains("38.00") && d.contains("112.00"), "should report remaining and committed: {d}");
    }

    #[tokio::test]
    async fn a_standing_grant_carries_the_whole_lifecycle_not_just_the_lock() {
        // Regression: lock accepted the collection audience but acknowledge and
        // dispute did not, so a party on a blanket grant could open an
        // agreement and then be unable to settle or dispute it.
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();

        lock_t(&s, &rep, standing_lock_req(&s, "agr_life", "500.00", Some("P30D"))).await.unwrap();
        // Settle with a presentation minted for the COLLECTION audience.
        let ack = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE, vec![standing_scope("500.00", Some("P30D"), None)], 1);
        let e = acknowledge(&s.verify, &s.db, &Log, "agr_life", &ack).await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(bal(&s, "bob@y.test"), 5_000);

        // And a dispute under the same standing authority.
        lock_t(&s, &rep, standing_lock_req(&s, "agr_life2", "500.00", Some("P30D"))).await.unwrap();
        let d = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", COLLECTION_AUDIENCE, vec![standing_scope("500.00", Some("P30D"), None)], 9);
        let (e2, sealed) = dispute(&s.verify, &s.db, "agr_life2", &d).await.unwrap();
        assert_eq!(e2.state, "disputed");
        assert_eq!(sealed.as_deref(), Some("sealed"));
    }

    #[tokio::test]
    async fn a_one_off_warrant_still_works_alongside_a_standing_one() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();
        // A small standing grant cannot cover this agreement...
        let e = lock_t(&s, &rep, standing_lock_req(&s, "agr_big", "20.00", Some("P30D"))).await.err().unwrap();
        assert_eq!(e.error, "cap_exceeded");
        // ...so the human approves just this one, on the per-agreement
        // audience, without raising the blanket cap.
        let e = lock_t(&s, &rep, lock_req(&s, "agr_big", "60.00", None)).await.unwrap();
        assert_eq!(e.state, "locked");
    }

    #[tokio::test]
    async fn a_standing_grant_still_honors_counterparties() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();
        let mut r = standing_lock_req(&s, "agr_c1", "500.00", Some("P30D"));
        // Blanket on amount, but only for one domain — and the payee is not in it.
        r.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE,
            vec![standing_scope("500.00", Some("P30D"), Some(vec!["*@z.test"]))], 1);
        assert_eq!(lock_t(&s, &rep, r).await.err().unwrap().error, "counterparty_not_allowed");

        // The same grant widened to the payee's domain admits it.
        let mut r = standing_lock_req(&s, "agr_c2", "500.00", Some("P30D"));
        r.presentation = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE,
            vec![standing_scope("500.00", Some("P30D"), Some(vec!["*@y.test"]))], 1);
        assert_eq!(lock_t(&s, &rep, r).await.unwrap().state, "locked");
    }

    /// A chain carrying an agreement plus whatever evidence a test needs.
    fn chain_with(id: &str, evidence: Vec<(&str, &str, i64, Value)>) -> FakeChain {
        let c = FakeChain::agreement(id);
        let path = format!("/agreements/{id}/evidence/");
        for (eid, creator, at, extra) in evidence {
            let mut v = extra.as_object().cloned().unwrap_or_default();
            v.insert("at".into(), json!(at));
            c.put(&path, eid, creator, Value::Object(v));
        }
        c
    }

    #[tokio::test]
    async fn an_unresponsive_adjudicator_can_be_replaced_but_only_by_both_parties() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();
        let payer = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE, vec![standing_scope("600.00", None, None)], 1);
        let payee = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", COLLECTION_AUDIENCE, vec![standing_scope("600.00", None, None)], 9);
        let nominate = |id: &str, who: &str, pick: &str| (id.to_string(), who.to_string(), pick.to_string());
        let chain_for = |id: &str, noms: Vec<(String, String, String)>| {
            let c = FakeChain::agreement(id);
            let path = format!("/agreements/{id}/evidence/");
            for (i, (_, who, pick)) in noms.iter().enumerate() {
                c.put(&path, &format!("ev_{i}"), who, json!({ "kind": "replace_adjudicator", "adjudicator": pick, "at": 1 }));
            }
            c
        };

        // Not disputed at all: there is no adjudicator to replace.
        lock_t(&s, &rep, lock_req(&s, "agr_j1", "600.00", None)).await.unwrap();
        let c1 = chain_for("agr_j1", vec![]);
        assert_eq!(replace_adjudicator(&s.verify, &s.db, &Log, &c1, "agr_j1", &payer).await.err().unwrap().error, "bad_state");

        // Disputed, but the ruling window is still open: a judge that is merely
        // deliberating must not be swapped out.
        lock_t(&s, &rep, lock_req(&s, "agr_j2", "600.00", None)).await.unwrap();
        dispute(&s.verify, &s.db, "agr_j2", &payer).await.unwrap();
        let c2 = chain_for("agr_j2", vec![nominate("agr_j2", "alice+agent@x.test", "judge2@dsp.test"), nominate("agr_j2", "bob+agent@y.test", "judge2@dsp.test")]);
        assert_eq!(replace_adjudicator(&s.verify, &s.db, &Log, &c2, "agr_j2", &payer).await.err().unwrap().error, "ruling_window_open");

        // Backdate the dispute past the window and try the interesting cases.
        let age = |id: &str| {
            s.db.with(|c| {
                c.execute("UPDATE escrows SET disputed_at=?2 WHERE id=?1", params![id, db::now() - RULING_WINDOW_SECS - 60])?;
                Ok(())
            })
            .unwrap_or_else(|e: anyhow::Error| panic!("{e}"))
        };
        age("agr_j2");

        // ONE party nominating is not enough — otherwise either side could shop
        // for a friendlier judge on its own.
        let only_one = chain_for("agr_j2", vec![nominate("agr_j2", "alice+agent@x.test", "judge2@dsp.test")]);
        assert_eq!(replace_adjudicator(&s.verify, &s.db, &Log, &only_one, "agr_j2", &payer).await.err().unwrap().error, "not_agreed");

        // Both, but naming DIFFERENT replacements, is also no agreement.
        let disagree = chain_for("agr_j2", vec![nominate("agr_j2", "alice+agent@x.test", "judge2@dsp.test"), nominate("agr_j2", "bob+agent@y.test", "judge3@dsp.test")]);
        assert_eq!(replace_adjudicator(&s.verify, &s.db, &Log, &disagree, "agr_j2", &payer).await.err().unwrap().error, "not_agreed");

        // Both agreeing, after the window: the swap takes effect.
        let e = replace_adjudicator(&s.verify, &s.db, &Log, &c2, "agr_j2", &payer).await.unwrap();
        assert_eq!(e.adjudicator, "judge2@dsp.test");

        // And the fee follows the work: the replacement is who gets paid.
        let ruling_by_new = presentation(&s.issuer, "judge2@dsp.test", "judge2@dsp.test", ORIGIN, vec![ScopeEntry::Bare("ruling".into())], 5);
        let r = ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_j2", RulingRequest {
            presentation: ruling_by_new, ruling: json!({ "outcome": "payee", "fee": "escrow" }),
        }).await.unwrap();
        assert_eq!(r.state, "settled");
        assert_eq!(bal(&s, "judge2@dsp.test"), 500, "the replacement is paid the escrowed fee");
        assert_eq!(bal(&s, "judge+agent@dsp.test"), 0, "the one that never ruled gets nothing");
    }

    /// The parties' nominations are on chain already (each is an attributed
    /// evidence object). The custodian records that it HONOURED them, so a
    /// ruling by an identity the proposal never named is explicable from the
    /// ledger alone rather than only from the custodian's private row.
    #[tokio::test]
    async fn the_custodian_records_the_replacement_on_chain() {
        use std::sync::{Arc, Mutex};
        #[derive(Default)]
        struct Recorder(Arc<Mutex<Vec<(String, String, String)>>>);
        #[async_trait::async_trait]
        impl Publisher for Recorder {
            async fn lock(&self, _: &str, _: i64, _: (&str, &str)) {}
            async fn settled(&self, _: &str, _: (&str, &str), _: &str, _: i64, _: &str) {}
            async fn adjudicator_replaced(&self, id: &str, from: &str, to: &str) {
                self.0.lock().unwrap().push((id.into(), from.into(), to.into()));
            }
        }
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        s.db.with(|c| Db::adjust(c, "alice@x.test", "USD", 100_000, "credit", None)).unwrap();
        let payer = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", COLLECTION_AUDIENCE, vec![standing_scope("600.00", None, None)], 1);
        lock_t(&s, &rep, lock_req(&s, "agr_rec", "600.00", None)).await.unwrap();
        dispute(&s.verify, &s.db, "agr_rec", &payer).await.unwrap();
        s.db.with(|c| {
            c.execute("UPDATE escrows SET disputed_at=?2 WHERE id=?1", params!["agr_rec", db::now() - RULING_WINDOW_SECS - 60])?;
            Ok(())
        }).unwrap_or_else(|e: anyhow::Error| panic!("{e}"));

        let c = FakeChain::agreement("agr_rec");
        for (i, who) in ["alice+agent@x.test", "bob+agent@y.test"].iter().enumerate() {
            c.put("/agreements/agr_rec/evidence/", &format!("ev_{i}"), who,
                json!({ "kind": "replace_adjudicator", "adjudicator": "judge2@dsp.test", "at": 1 }));
        }
        let rec = Recorder::default();
        let seen = rec.0.clone();
        replace_adjudicator(&s.verify, &s.db, &rec, &c, "agr_rec", &payer).await.unwrap();
        let got = seen.lock().unwrap().clone();
        assert_eq!(got, vec![("agr_rec".into(), "judge+agent@dsp.test".into(), "judge2@dsp.test".into())],
            "the swap is published with both the old and the new adjudicator");
    }

    #[test]
    fn the_backstop_scales_with_the_agreement_but_is_bounded_and_paused_by_a_dispute() {
        let base = |state: &str, deadline: Option<i64>| Escrow {
            id: "x".into(), state: state.into(), payer: "p".into(), payee: "q".into(),
            payer_account: "pa".into(), payee_account: "qa".into(), reputation_seen: None,
            adjudicator: "j".into(), price_minor: 5_000, custodian_fee_minor: 50,
            adjudicator_fee_minor: 500, contribution_minor: 50, contribution_recipient: None,
            currency: "USD".into(), locked_at: 0, disputed_at: None, latest_deadline: deadline,
            settled_at: None, outcome: None, sealed_terms: None, proposal: Value::Null,
        };
        // A six-hour agreement wants 12h, but the floor holds it at 24h — a
        // short job must not get a short backstop.
        assert_eq!(backstop_due(&base("locked", Some(6 * 3600)), 1_000), Some(1_000 + 86_400));
        // A 60-day agreement wants 120 days; the ceiling caps it at 30.
        assert_eq!(backstop_due(&base("locked", Some(60 * 86_400)), 0), Some(30 * 86_400));
        // In between, it really does scale: 10 days -> 20. (The ceiling binds for
        // anything longer than 15 days, since 2x then exceeds 30.)
        assert_eq!(backstop_due(&base("locked", Some(10 * 86_400)), 0), Some(20 * 86_400));
        // No deadline recorded: the floor.
        assert_eq!(backstop_due(&base("locked", None), 0), Some(86_400));

        // THE SAFETY PROPERTY: a dispute suspends it entirely. Otherwise
        // attacking the adjudicator would be a way to run the clock out and
        // collect half of what you were going to lose.
        assert_eq!(backstop_due(&base("disputed", Some(6 * 3600)), 1_000), None);
        assert_eq!(backstop_due(&base("settled", Some(6 * 3600)), 1_000), None);
    }

    #[tokio::test]
    async fn sweeping_an_abandoned_escrow_splits_the_price_and_refunds_the_unused_fee() {
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        lock_t(&s, &rep, lock_req(&s, "agr_ab", "600.00", None)).await.unwrap();
        let (payer_before, payee_before) = (bal(&s, "alice@x.test"), bal(&s, "bob@y.test"));

        let e = sweep_abandoned(&s.db, &Log, "agr_ab").await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(e.outcome.as_deref(), Some("abandoned"));

        // Price 50.00 splits; the 5.00 adjudicator fee goes back to the payer
        // because nobody adjudicated; the custodian keeps its 0.50; the
        // contribution is paid as a term of the agreement.
        assert_eq!(bal(&s, "bob@y.test") - payee_before, 2_500, "half the price to the payee");
        assert_eq!(bal(&s, "alice@x.test") - payer_before, 2_500 + 500, "half back, plus the unused fee");
        assert_eq!(bal(&s, "custodian"), 50);
        assert_eq!(bal(&s, "fund@dsp.test"), 50);

        // It is a settlement like any other: it cannot happen twice.
        assert_eq!(sweep_abandoned(&s.db, &Log, "agr_ab").await.err().unwrap().error, "bad_state");
    }

    #[tokio::test]
    async fn silence_releases_nothing_the_only_ways_out_are_acknowledge_or_dispute() {
        // A lapsed response window used to let the payee take the money without
        // ever facing an adjudicator, which is what made withholding pay: hold
        // the work back, wait out an inattentive payer, collect. There is now no
        // unilateral release at all — silence leaves the escrow locked and
        // somebody has to ask for a decision.
        let s = setup();
        let rep = crate::reputation::Reputation::new(true);
        let payee = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", COLLECTION_AUDIENCE,
            vec![standing_scope("500.00", None, None)], 9);
        lock_t(&s, &rep, lock_req(&s, "agr_s1", "600.00", None)).await.unwrap();

        // The payee cannot settle in their own favour by any route.
        assert_eq!(acknowledge(&s.verify, &s.db, &Log, "agr_s1", &payee).await.err().unwrap().error, "not_payer");
        assert_eq!(get_state(&s, "agr_s1"), "locked", "still locked however long nobody answers");
        let before = bal(&s, "bob@y.test");

        // What they CAN do is put it to the adjudicator.
        let (e, sealed) = dispute(&s.verify, &s.db, "agr_s1", &payee).await.unwrap();
        assert_eq!(e.state, "disputed");
        assert_eq!(sealed.as_deref(), Some("sealed"), "and the adjudicator gets the terms");
        assert_eq!(bal(&s, "bob@y.test"), before, "nothing has moved yet");
    }

    #[tokio::test]
    async fn lock_then_acknowledge_pays_everyone() {
        let s = setup();
        let e = lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_1", "60.00", None)).await.unwrap();
        assert_eq!(e.state, "locked");
        assert_eq!((e.payer.as_str(), e.payer_account.as_str()), ("alice+agent@x.test", "alice@x.test"));
        assert_eq!((e.payee.as_str(), e.payee_account.as_str()), ("bob+agent@y.test", "bob@y.test"));
        // The payee's consent is the ACCEPTANCE ON CHAIN, not a bundle in the
        // request: no acceptance object, no lock.
        let rep = crate::reputation::Reputation::new(true);
        let bare = FakeChain::default();
        bare.put("/agreements/agr_9/", "proposal", "alice+agent@x.test", proposal("agr_9"));
        let e9 = lock(&s.verify, &s.db, &Log, &rep, ORIGIN, "USD", &bare, lock_req(&s, "agr_9", "60.00", None)).await.err().unwrap();
        assert_eq!(e9.error, "not_accepted");
        // And an acceptance written by somebody who is not the named acceptor
        // is refused on attribution, which is what replaced the bundle check.
        let forged = FakeChain::default();
        forged.put("/agreements/agr_9/", "proposal", "alice+agent@x.test", proposal("agr_9"));
        forged.put("/agreements/agr_9/", "acceptance", "carol+agent@z.test", json!({ "v": 1, "id": "agr_9" }));
        let ef = lock(&s.verify, &s.db, &Log, &rep, ORIGIN, "USD", &forged, lock_req(&s, "agr_9", "60.00", None)).await.err().unwrap();
        assert_eq!(ef.error, "bad_attribution");
        // A proposal not written by the identity it names is refused too.
        let spoof = FakeChain::default();
        spoof.put("/agreements/agr_9/", "proposal", "mallory@evil.test", proposal("agr_9"));
        spoof.put("/agreements/agr_9/", "acceptance", "bob+agent@y.test", json!({ "v": 1, "id": "agr_9" }));
        let es = lock(&s.verify, &s.db, &Log, &rep, ORIGIN, "USD", &spoof, lock_req(&s, "agr_9", "60.00", None)).await.err().unwrap();
        assert_eq!(es.error, "bad_attribution");
        // 50 + 0.50 + 5 + 1% of 50 (0.50) = 56.00 locked
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600);
        // Locking twice is refused.
        assert_eq!(lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_1", "60.00", None)).await.err().unwrap().error, "already_locked");

        let aud = contract_audience("agr_1");
        let ack = presentation(&s.issuer, "alice@x.test", "alice+agent@x.test", &aud, vec![contract_scope("60.00", None)], 1);
        let e = acknowledge(&s.verify, &s.db, &Log, "agr_1", &ack).await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(e.outcome.as_deref(), Some("acknowledged"));
        assert_eq!(bal(&s, "bob@y.test"), 5_000, "the price lands on bob's human");
        assert_eq!(bal(&s, "bob+agent@y.test"), 0, "never on the agent");
        assert_eq!(bal(&s, "custodian"), 50);
        assert_eq!(bal(&s, "fund@dsp.test"), 50);
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600 + 500, "unused adjudicator fee returned to alice's human");
        assert_eq!(bal(&s, "judge+agent@dsp.test"), 0);
    }

    #[tokio::test]
    async fn lock_records_both_parties_grants_as_budgets() {
        let s = setup();
        lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_b", "60.00", Some(vec!["bob+agent@y.test"]))).await.unwrap();
        // Alice (the payer's human) sees her agent's agreement cap with the lock spent against it.
        let alice = s.db.warrants_for("alice@x.test").unwrap();
        let g = alice.iter().find(|w| w.agreement_id.as_deref() == Some("agr_b")).expect("payer grant recorded");
        assert_eq!(g.scope, CONTRACT_SCOPE);
        assert_eq!(g.grantee, "alice+agent@x.test");
        assert_eq!(g.cap_minor, 6_000);
        assert_eq!(g.counterparties.as_deref(), Some(r#"["bob+agent@y.test"]"#));
        let spent = s.db.with(|c| Db::spent_since(c, &g.key, 0)).unwrap();
        assert_eq!(spent, 5_600, "the lock is the spend against the cap");
        // Bob's grant is no longer presented at the lock — the chain proves he
        // accepted — so what his human sees is the grant he REGISTERED, not one
        // pinned to this agreement, and nothing is spent against it.
        let bob = s.db.warrants_for("bob@y.test").unwrap();
        let gb = bob.first().expect("the payee's registered grant is visible to their human");
        assert_eq!(gb.grantee, "bob+agent@y.test");
        assert_eq!(s.db.with(|c| Db::spent_since(c, &gb.key, 0)).unwrap(), 0);
    }

    #[tokio::test]
    async fn cap_and_counterparties_bound_the_lock() {
        let s = setup();
        assert_eq!(lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_2", "55.00", None)).await.err().unwrap().error, "cap_exceeded");
        assert_eq!(lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_2", "60.00", Some(vec!["*@z.test"]))).await.err().unwrap().error, "counterparty_not_allowed");
        lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_2", "60.00", Some(vec!["*@y.test"]))).await.unwrap();
        // The payee cannot lock (not the payer), even with a grant.
        let aud = contract_audience("agr_3");
        let mut r = lock_req(&s, "agr_3", "60.00", None);
        r.presentation = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", &aud, vec![contract_scope("60.00", None)], 2);
        assert_eq!(lock_t(&s, &crate::reputation::Reputation::new(true), r).await.err().unwrap().error, "not_payer");
    }

    #[tokio::test]
    async fn dispute_then_ruling_only_from_the_named_adjudicator() {
        let s = setup();
        lock_t(&s, &crate::reputation::Reputation::new(true), lock_req(&s, "agr_4", "60.00", None)).await.unwrap();
        let aud = contract_audience("agr_4");
        let bob = presentation(&s.issuer, "bob@y.test", "bob+agent@y.test", &aud, vec![contract_scope("60.00", None)], 3);
        let (e, sealed) = dispute(&s.verify, &s.db, "agr_4", &bob).await.unwrap();
        assert_eq!(e.state, "disputed");
        assert_eq!(sealed.as_deref(), Some("sealed"));
        // A stranger's ruling is refused; the named adjudicator's is honored once.
        let stranger = presentation(&s.issuer, "someone@z.test", "someone@z.test", ORIGIN, vec![ScopeEntry::Bare("login".into())], 4);
        // The judge's HUMAN (its grantor) is not the adjudicator either — the acting identity is.
        let judges_human = presentation(&s.issuer, "dan@dsp.test", "dan@dsp.test", ORIGIN, vec![ScopeEntry::Bare("login".into())], 6);
        let rr = |p: &str| RulingRequest { presentation: p.into(), ruling: json!({ "outcome": "split", "split": { "payee": "30.00", "payer": "20.00" }, "fee": "escrow", "reasoning": "partial delivery" }) };
        assert_eq!(ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&stranger)).await.err().unwrap().error, "not_adjudicator");
        assert_eq!(ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&judges_human)).await.err().unwrap().error, "not_adjudicator");
        // The judge is a delegated agent: dan@dsp.test granted judge+agent@dsp.test.
        let judge = presentation(&s.issuer, "dan@dsp.test", "judge+agent@dsp.test", ORIGIN, vec![ScopeEntry::Bare("ruling".into())], 5);
        let e = ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&judge)).await.unwrap();
        assert_eq!(e.state, "settled");
        assert_eq!(e.outcome.as_deref(), Some("ruled:split"));
        assert_eq!(bal(&s, "bob@y.test"), 3_000, "payee share to bob's human");
        assert_eq!(bal(&s, "judge+agent@dsp.test"), 500, "the adjudicator is paid as named");
        assert_eq!(bal(&s, "dan@dsp.test"), 0);
        assert_eq!(bal(&s, "alice@x.test"), 10_000 - 5_600 + 2_000, "payer share to alice's human");
        assert_eq!(ruling(&s.verify, &s.db, &Log, ORIGIN, "agr_4", rr(&judge)).await.err().unwrap().error, "bad_state");
    }
}
