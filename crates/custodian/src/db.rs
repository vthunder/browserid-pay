//! SQLite store: accounts, warrant counters, receipts, ledger.
//!
//! Money is integer minor units (cents) in one deployment currency. Every
//! balance change is a ledger row; receipts are the payment-level record the
//! human reads. Warrant counters are keyed by the warrant's status ref
//! (`uri#idx`) — the warrant's identity — so two warrants are two budgets.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub identity: String,
    pub currency: String,
    pub balance_minor: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarrantRow {
    pub key: String,
    pub grantor: String,
    pub grantee: String,
    pub scope: String,
    pub cap_minor: i64,
    pub currency: String,
    pub window_secs: Option<i64>,
    pub iat: i64,
    pub exp: i64,
    pub first_seen: i64,
    pub revoked_at: Option<i64>,
    /// Set for a per-agreement `contract:agreement` grant.
    pub agreement_id: Option<String>,
    /// JSON array of the grant's `counterparties` parameter, if any.
    pub counterparties: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub id: String,
    pub kind: String,
    pub grantor: String,
    pub grantee: String,
    pub warrant_key: Option<String>,
    pub amount_minor: i64,
    pub currency: String,
    pub payee: Option<String>,
    pub memo: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedgerRow {
    pub id: i64,
    pub identity: String,
    pub delta_minor: i64,
    pub kind: String,
    pub reference: Option<String>,
    pub at: i64,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("open {path}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS accounts (
               identity TEXT PRIMARY KEY,
               currency TEXT NOT NULL,
               balance_minor INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS warrants (
               key TEXT PRIMARY KEY,
               grantor TEXT NOT NULL,
               grantee TEXT NOT NULL,
               scope TEXT NOT NULL,
               cap_minor INTEGER NOT NULL,
               currency TEXT NOT NULL,
               window_secs INTEGER,
               iat INTEGER NOT NULL,
               exp INTEGER NOT NULL,
               first_seen INTEGER NOT NULL,
               revoked_at INTEGER
             );
             CREATE TABLE IF NOT EXISTS spends (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               warrant_key TEXT NOT NULL REFERENCES warrants(key),
               amount_minor INTEGER NOT NULL,
               at INTEGER NOT NULL,
               receipt_id TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS spends_warrant ON spends(warrant_key, at);
             CREATE TABLE IF NOT EXISTS receipts (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               grantor TEXT NOT NULL,
               grantee TEXT NOT NULL,
               warrant_key TEXT,
               amount_minor INTEGER NOT NULL,
               currency TEXT NOT NULL,
               payee TEXT,
               memo TEXT,
               at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS receipts_grantor ON receipts(grantor, at);
             CREATE TABLE IF NOT EXISTS ledger (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               identity TEXT NOT NULL,
               delta_minor INTEGER NOT NULL,
               kind TEXT NOT NULL,
               reference TEXT,
               at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS ledger_identity ON ledger(identity, at);",
        )?;
        // Per-agreement grants (added after the first deploy): ignore "duplicate column".
        for col in ["agreement_id TEXT", "counterparties TEXT"] {
            let _ = conn.execute(&format!("ALTER TABLE warrants ADD COLUMN {col}"), []);
        }
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut c = self.0.lock().expect("db lock");
        f(&mut c)
    }

    pub fn account(&self, identity: &str) -> Result<Option<Account>> {
        self.with(|c| {
            Ok(c.query_row(
                "SELECT identity, currency, balance_minor, created_at FROM accounts WHERE identity=?1",
                params![identity],
                |r| {
                    Ok(Account {
                        identity: r.get(0)?,
                        currency: r.get(1)?,
                        balance_minor: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                },
            )
            .optional()?)
        })
    }

    /// Credit (or debit, negative) an account, creating it on first touch.
    /// Returns the new balance. Refuses to overdraw.
    pub fn adjust(
        c: &Connection,
        identity: &str,
        currency: &str,
        delta_minor: i64,
        kind: &str,
        reference: Option<&str>,
    ) -> Result<i64> {
        let t = now();
        c.execute(
            "INSERT OR IGNORE INTO accounts(identity, currency, balance_minor, created_at) VALUES(?1, ?2, 0, ?3)",
            params![identity, currency, t],
        )?;
        let (bal, cur): (i64, String) = c.query_row(
            "SELECT balance_minor, currency FROM accounts WHERE identity=?1",
            params![identity],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        anyhow::ensure!(cur == currency, "account currency mismatch");
        let new = bal + delta_minor;
        anyhow::ensure!(new >= 0, "insufficient_funds");
        c.execute(
            "UPDATE accounts SET balance_minor=?2 WHERE identity=?1",
            params![identity, new],
        )?;
        c.execute(
            "INSERT INTO ledger(identity, delta_minor, kind, reference, at) VALUES(?1,?2,?3,?4,?5)",
            params![identity, delta_minor, kind, reference, t],
        )?;
        Ok(new)
    }

    pub fn upsert_warrant(c: &Connection, w: &WarrantRow) -> Result<()> {
        c.execute(
            "INSERT INTO warrants(key, grantor, grantee, scope, cap_minor, currency, window_secs, iat, exp, first_seen, agreement_id, counterparties)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(key) DO UPDATE SET cap_minor=excluded.cap_minor, window_secs=excluded.window_secs, exp=excluded.exp,
               agreement_id=COALESCE(excluded.agreement_id, warrants.agreement_id), counterparties=COALESCE(excluded.counterparties, warrants.counterparties)",
            params![w.key, w.grantor, w.grantee, w.scope, w.cap_minor, w.currency, w.window_secs, w.iat, w.exp, w.first_seen, w.agreement_id, w.counterparties],
        )?;
        Ok(())
    }

    /// Sum of spends against `key` since `since` (inclusive).
    pub fn spent_since(c: &Connection, key: &str, since: i64) -> Result<i64> {
        Ok(c.query_row(
            "SELECT COALESCE(SUM(amount_minor),0) FROM spends WHERE warrant_key=?1 AND at>=?2",
            params![key, since],
            |r| r.get(0),
        )?)
    }

    pub fn record_spend(c: &Connection, key: &str, amount_minor: i64, receipt_id: &str) -> Result<()> {
        c.execute(
            "INSERT INTO spends(warrant_key, amount_minor, at, receipt_id) VALUES(?1,?2,?3,?4)",
            params![key, amount_minor, now(), receipt_id],
        )?;
        Ok(())
    }

    pub fn insert_receipt(c: &Connection, r: &Receipt) -> Result<()> {
        c.execute(
            "INSERT INTO receipts(id, kind, grantor, grantee, warrant_key, amount_minor, currency, payee, memo, at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![r.id, r.kind, r.grantor, r.grantee, r.warrant_key, r.amount_minor, r.currency, r.payee, r.memo, r.at],
        )?;
        Ok(())
    }

    pub fn warrants_for(&self, grantor: &str) -> Result<Vec<WarrantRow>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT key, grantor, grantee, scope, cap_minor, currency, window_secs, iat, exp, first_seen, revoked_at, agreement_id, counterparties
                 FROM warrants WHERE grantor=?1 ORDER BY first_seen DESC",
            )?;
            let rows = st
                .query_map(params![grantor], |r| {
                    Ok(WarrantRow {
                        key: r.get(0)?,
                        grantor: r.get(1)?,
                        grantee: r.get(2)?,
                        scope: r.get(3)?,
                        cap_minor: r.get(4)?,
                        currency: r.get(5)?,
                        window_secs: r.get(6)?,
                        iat: r.get(7)?,
                        exp: r.get(8)?,
                        first_seen: r.get(9)?,
                        revoked_at: r.get(10)?,
                        agreement_id: r.get(11)?,
                        counterparties: r.get(12)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn receipts_for(&self, identity: &str, limit: usize) -> Result<Vec<Receipt>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, kind, grantor, grantee, warrant_key, amount_minor, currency, payee, memo, at
                 FROM receipts WHERE grantor=?1 OR payee=?1 ORDER BY at DESC LIMIT ?2",
            )?;
            let rows = st
                .query_map(params![identity, limit as i64], |r| {
                    Ok(Receipt {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        grantor: r.get(2)?,
                        grantee: r.get(3)?,
                        warrant_key: r.get(4)?,
                        amount_minor: r.get(5)?,
                        currency: r.get(6)?,
                        payee: r.get(7)?,
                        memo: r.get(8)?,
                        at: r.get(9)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Every ledger row for `identity`, oldest first (the running balance
    /// needs the whole history).
    pub fn ledger_all(c: &Connection, identity: &str) -> Result<Vec<LedgerRow>> {
        let mut st = c.prepare(
            "SELECT id, identity, delta_minor, kind, reference, at FROM ledger WHERE identity=?1 ORDER BY at ASC, id ASC",
        )?;
        let rows = st
            .query_map(params![identity], |r| {
                Ok(LedgerRow { id: r.get(0)?, identity: r.get(1)?, delta_minor: r.get(2)?, kind: r.get(3)?, reference: r.get(4)?, at: r.get(5)? })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every ledger row (any account) carrying `reference` — e.g.
    /// `escrow:<id>` — oldest first: the money that moved for one agreement.
    pub fn ledger_by_reference(c: &Connection, reference: &str) -> Result<Vec<LedgerRow>> {
        let mut st = c.prepare(
            "SELECT id, identity, delta_minor, kind, reference, at FROM ledger WHERE reference=?1 ORDER BY at ASC, id ASC",
        )?;
        let rows = st
            .query_map(params![reference], |r| {
                Ok(LedgerRow { id: r.get(0)?, identity: r.get(1)?, delta_minor: r.get(2)?, kind: r.get(3)?, reference: r.get(4)?, at: r.get(5)? })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Pair each row (given oldest first) with the balance after it.
pub fn with_running_balance(rows: &[LedgerRow]) -> Vec<(LedgerRow, i64)> {
    let mut bal = 0i64;
    rows.iter()
        .map(|r| {
            bal += r.delta_minor;
            (r.clone(), bal)
        })
        .collect()
}

/// Rolling window start for a warrant: windows tile from `iat` in steps of
/// `window_secs`; the current one is the latest boundary at or before `now`.
/// No window ⇒ lifetime (start = iat).
/// Total committed under a warrant since `since`: every escrow it locked that
/// has not been refunded outright. A settled escrow still counts — the money
/// was spent — which is what makes a rolling cap a spending limit rather than
/// a concurrency limit.
pub fn locked_since(c: &Connection, warrant_key: &str, since: i64) -> rusqlite::Result<i64> {
    c.query_row(
        "SELECT COALESCE(SUM(price_minor + custodian_fee_minor + adjudicator_fee_minor + contribution_minor), 0)
           FROM escrows WHERE warrant_key = ?1 AND locked_at >= ?2",
        rusqlite::params![warrant_key, since],
        |r| r.get(0),
    )
}

pub fn window_start(iat: i64, window_secs: Option<i64>, now: i64) -> i64 {
    match window_secs {
        Some(w) if w > 0 && now >= iat => iat + ((now - iat) / w) * w,
        _ => iat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_tile_from_iat() {
        assert_eq!(window_start(1000, Some(100), 1000), 1000);
        assert_eq!(window_start(1000, Some(100), 1099), 1000);
        assert_eq!(window_start(1000, Some(100), 1100), 1100);
        assert_eq!(window_start(1000, Some(100), 1250), 1200);
        assert_eq!(window_start(1000, None, 99_999), 1000);
    }

    #[test]
    fn running_balance_follows_the_ledger() {
        let db = Db::open(":memory:").unwrap();
        db.with(|c| {
            Db::adjust(c, "a@x", "USD", 10_000, "credit", None)?;
            Db::adjust(c, "a@x", "USD", -5_600, "escrow_lock", Some("escrow:agr_1"))?;
            Db::adjust(c, "a@x", "USD", 500, "escrow_refund", Some("escrow:agr_1"))?;
            let rows = Db::ledger_all(c, "a@x")?;
            let rb = with_running_balance(&rows);
            assert_eq!(rb.iter().map(|(_, b)| *b).collect::<Vec<_>>(), vec![10_000, 4_400, 4_900]);
            assert_eq!(Db::ledger_by_reference(c, "escrow:agr_1")?.len(), 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn adjust_refuses_overdraw() {
        let db = Db::open(":memory:").unwrap();
        db.with(|c| {
            assert_eq!(Db::adjust(c, "a@x", "USD", 500, "credit", None).unwrap(), 500);
            assert!(Db::adjust(c, "a@x", "USD", -600, "debit", None).is_err());
            assert_eq!(Db::adjust(c, "a@x", "USD", -500, "debit", None).unwrap(), 0);
            assert!(Db::adjust(c, "a@x", "EUR", 1, "credit", None).is_err());
            Ok(())
        })
        .unwrap();
    }
}
