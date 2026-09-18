//! Case store (SQLite): one row per contract the judge has been asked about.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Cases(Arc<Mutex<Connection>>);

#[derive(Debug, Clone, Serialize)]
pub struct Case {
    pub contract_id: String,
    /// `open` (response window running) → `responded` | (window passed) →
    /// `ruled` → `delivered`; or `failed` (with `error`).
    pub state: String,
    pub disputed_by: String,
    pub dispute_note: Option<String>,
    pub custodian: String,
    pub parties: Vec<String>,
    pub terms: serde_json::Value,
    pub proposal: serde_json::Value,
    pub evidence: serde_json::Value,
    pub ruling: Option<serde_json::Value>,
    pub submission: Option<String>,
    pub error: Option<String>,
    pub opened_at: i64,
    pub updated_at: i64,
    /// End of the counterparty's response window, as a BLOCK HEIGHT — not a
    /// wall-clock time. This decides whether a party forfeited their chance to
    /// answer, so it must not rest on anyone's clock (browserid-pay-e1ou).
    ///
    /// The column is deliberately renamed alongside the type change: a height
    /// and a unix timestamp are both integers, so an old database would be
    /// silently misread (every `respond_by` far in the "future", cases never
    /// becoming due). A missing column errors loudly instead.
    pub respond_by_block: i64,
    /// The counterparty's response, once filed: `{ by, note, reveals: [{ evidence_id, matched, content? }], at }`.
    pub response: Option<serde_json::Value>,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Cases {
    pub fn open(path: &str) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS cases (
               contract_id TEXT PRIMARY KEY,
               state TEXT NOT NULL,
               disputed_by TEXT NOT NULL,
               dispute_note TEXT,
               custodian TEXT NOT NULL,
               parties TEXT NOT NULL,
               terms TEXT NOT NULL,
               proposal TEXT NOT NULL,
               evidence TEXT NOT NULL,
               ruling TEXT,
               submission TEXT,
               error TEXT,
               opened_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );",
        )?;
        // Response phase (2026-09-16): added after the first live cases existed.
        //
        // `respond_by_block` defaults to i64::MAX, NOT 0. This column replaced a
        // wall-clock `respond_by` (2026-09-18, browserid-pay-e1ou), and a row
        // carried over from that schema has no height to offer. A default of 0
        // would make every such case instantly "due" and rule it unanswered —
        // moving money on a window that never ran. i64::MAX fails the other way:
        // the case simply never becomes due, which is visible and harmless.
        for col in ["respond_by_block INTEGER NOT NULL DEFAULT 9223372036854775807", "response TEXT"] {
            let _ = c.execute(&format!("ALTER TABLE cases ADD COLUMN {col}"), []);
        }
        Ok(Self(Arc::new(Mutex::new(c))))
    }

    pub fn insert(&self, case: &Case) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "INSERT INTO cases(contract_id, state, disputed_by, dispute_note, custodian, parties, terms, proposal, evidence, ruling, submission, error, opened_at, updated_at, respond_by_block, response)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                case.contract_id, case.state, case.disputed_by, case.dispute_note, case.custodian,
                serde_json::to_string(&case.parties)?, case.terms.to_string(), case.proposal.to_string(), case.evidence.to_string(),
                case.ruling.as_ref().map(|r| r.to_string()), case.submission, case.error, case.opened_at, case.updated_at,
                case.respond_by_block, case.response.as_ref().map(|r| r.to_string())
            ],
        )?;
        Ok(())
    }

    pub fn update(&self, id: &str, state: &str, ruling: Option<&serde_json::Value>, submission: Option<&str>, error: Option<&str>) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "UPDATE cases SET state=?2, ruling=COALESCE(?3, ruling), submission=COALESCE(?4, submission), error=?5, updated_at=?6 WHERE contract_id=?1",
            params![id, state, ruling.map(|r| r.to_string()), submission, error, now()],
        )?;
        Ok(())
    }

    /// Record the counterparty's response and move the case to `responded`.
    pub fn respond(&self, id: &str, response: &serde_json::Value) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "UPDATE cases SET state='responded', response=?2, updated_at=?3 WHERE contract_id=?1",
            params![id, response.to_string(), now()],
        )?;
        Ok(())
    }

    const COLS: &'static str = "contract_id, state, disputed_by, dispute_note, custodian, parties, terms, proposal, evidence, ruling, submission, error, opened_at, updated_at, respond_by_block, response";

    fn row(r: &rusqlite::Row) -> rusqlite::Result<Case> {
        let j = |i: usize| -> rusqlite::Result<serde_json::Value> {
            let s: String = r.get(i)?;
            Ok(serde_json::from_str(&s).unwrap_or(serde_json::Value::Null))
        };
        let ruling: Option<String> = r.get(9)?;
        let response: Option<String> = r.get(15)?;
        Ok(Case {
            contract_id: r.get(0)?,
            state: r.get(1)?,
            disputed_by: r.get(2)?,
            dispute_note: r.get(3)?,
            custodian: r.get(4)?,
            parties: serde_json::from_value(j(5)?).unwrap_or_default(),
            terms: j(6)?,
            proposal: j(7)?,
            evidence: j(8)?,
            ruling: ruling.and_then(|s| serde_json::from_str(&s).ok()),
            submission: r.get(10)?,
            error: r.get(11)?,
            opened_at: r.get(12)?,
            updated_at: r.get(13)?,
            respond_by_block: r.get(14)?,
            response: response.and_then(|s| serde_json::from_str(&s).ok()),
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<Case>> {
        let c = self.0.lock().unwrap();
        Ok(c.query_row(&format!("SELECT {} FROM cases WHERE contract_id=?1", Self::COLS), params![id], Self::row).optional()?)
    }

    /// Open cases whose response window has passed: due for a ruling.
    /// `head` is the chain's current block height, not a wall-clock time.
    pub fn due(&self, head: u64) -> Result<Vec<Case>> {
        let c = self.0.lock().unwrap();
        let mut st = c.prepare(&format!("SELECT {} FROM cases WHERE state='open' AND respond_by_block<=?1", Self::COLS))?;
        let v = st.query_map(params![head as i64], Self::row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Cases (any state) where `identity` is a party, newest first.
    pub fn for_party(&self, identity: &str) -> Result<Vec<Case>> {
        let c = self.0.lock().unwrap();
        let mut st = c.prepare(&format!("SELECT {} FROM cases ORDER BY opened_at DESC", Self::COLS))?;
        let v = st.query_map([], Self::row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v.into_iter().filter(|k| k.parties.iter().any(|p| p == identity)).collect())
    }

    #[cfg(test)]
    pub fn with_conn(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<()>) {
        f(&self.0.lock().unwrap()).unwrap();
    }

    pub fn count(&self) -> Result<(i64, i64)> {
        let c = self.0.lock().unwrap();
        let total: i64 = c.query_row("SELECT COUNT(*) FROM cases", [], |r| r.get(0))?;
        let ruled: i64 = c.query_row("SELECT COUNT(*) FROM cases WHERE state IN ('ruled','delivered')", [], |r| r.get(0))?;
        Ok((total, ruled))
    }
}
