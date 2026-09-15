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
    /// `open` → `ruled` → `delivered`; or `failed` (with `error`).
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
        Ok(Self(Arc::new(Mutex::new(c))))
    }

    pub fn insert(&self, case: &Case) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "INSERT INTO cases(contract_id, state, disputed_by, dispute_note, custodian, parties, terms, proposal, evidence, ruling, submission, error, opened_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                case.contract_id, case.state, case.disputed_by, case.dispute_note, case.custodian,
                serde_json::to_string(&case.parties)?, case.terms.to_string(), case.proposal.to_string(), case.evidence.to_string(),
                case.ruling.as_ref().map(|r| r.to_string()), case.submission, case.error, case.opened_at, case.updated_at
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

    pub fn get(&self, id: &str) -> Result<Option<Case>> {
        let c = self.0.lock().unwrap();
        Ok(c.query_row(
            "SELECT contract_id, state, disputed_by, dispute_note, custodian, parties, terms, proposal, evidence, ruling, submission, error, opened_at, updated_at FROM cases WHERE contract_id=?1",
            params![id],
            |r| {
                let j = |i: usize| -> rusqlite::Result<serde_json::Value> {
                    let s: String = r.get(i)?;
                    Ok(serde_json::from_str(&s).unwrap_or(serde_json::Value::Null))
                };
                let ruling: Option<String> = r.get(9)?;
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
                })
            },
        )
        .optional()?)
    }

    pub fn count(&self) -> Result<(i64, i64)> {
        let c = self.0.lock().unwrap();
        let total: i64 = c.query_row("SELECT COUNT(*) FROM cases", [], |r| r.get(0))?;
        let ruled: i64 = c.query_row("SELECT COUNT(*) FROM cases WHERE state IN ('ruled','delivered')", [], |r| r.get(0))?;
        Ok((total, ruled))
    }
}
