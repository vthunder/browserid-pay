//! The payment path: verify the bundle, find the `pay:<rail>` grant, enforce
//! the cap for the current rolling window, move the money, write the receipt.
//!
//! Rails in v1: `transfer` — an internal move to another account here. x402
//! and cards are later rails behind the same check.

use crate::db::{self, Db, Receipt, WarrantRow};
use crate::verify::{Verified, Verify};
use browserid_core::device::{parse_money_minor, ScopeEntry};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PayRequest {
    pub presentation: String,
    /// Decimal string, e.g. "4.50".
    pub amount: String,
    pub currency: String,
    /// `transfer`: the payee identity (an email).
    pub payee: String,
    #[serde(default)]
    pub memo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PayError {
    pub error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<String>,
}

impl PayError {
    fn new(error: &'static str) -> Self {
        Self { error, detail: None, remaining: None }
    }
    fn detail(error: &'static str, d: impl Into<String>) -> Self {
        Self { error, detail: Some(d.into()), remaining: None }
    }
}

pub fn fmt_minor(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let m = minor.abs();
    format!("{sign}{}.{:02}", m / 100, m % 100)
}

/// The `pay:<rail>` entry this custodian will honor, with its cap.
pub struct PayGrant {
    pub scope: String,
    pub rail: String,
    pub cap_minor: i64,
    pub currency: String,
    pub window_secs: Option<i64>,
}

/// Pick the entry for `rail`. Fail closed on anything odd: a `pay:` scope
/// with no cap is a grant this custodian will not honor (an uncapped budget
/// is never what a consent card said), and a malformed window rejects.
pub fn pay_grant(scopes: &[ScopeEntry], rail: &str) -> Result<PayGrant, PayError> {
    let want = format!("pay:{rail}");
    for e in scopes {
        let ScopeEntry::Parameterized(p) = e else { continue };
        if p.scope != want {
            continue;
        }
        let Some(cap) = &p.cap else {
            return Err(PayError::detail("no_grant", "pay scope without a cap"));
        };
        let cap_minor = cap
            .amount_minor()
            .ok_or_else(|| PayError::detail("no_grant", "malformed cap amount"))?;
        let window_secs = cap
            .window_secs()
            .map_err(|_| PayError::detail("no_grant", "malformed cap window"))?;
        return Ok(PayGrant {
            scope: p.scope.clone(),
            rail: rail.to_string(),
            cap_minor: cap_minor as i64,
            currency: cap.currency.clone(),
            window_secs: window_secs.map(|w| w as i64),
        });
    }
    Err(PayError::new("no_grant"))
}

pub async fn pay(verify: &Verify, db: &Db, deployment_currency: &str, req: PayRequest) -> Result<Receipt, PayError> {
    let rail = "transfer";
    let Some(amount_minor) = parse_money_minor(&req.amount) else {
        return Err(PayError::detail("bad_request", "amount must be a decimal string"));
    };
    if amount_minor == 0 {
        return Err(PayError::detail("bad_request", "amount must be positive"));
    }
    let amount_minor = amount_minor as i64;
    if req.currency != deployment_currency {
        return Err(PayError::detail("unsupported_currency", deployment_currency));
    }
    let payee = req.payee.trim().to_string();
    if payee.is_empty() || !payee.contains('@') {
        return Err(PayError::detail("bad_request", "payee must be an identity"));
    }

    let Verified { identity, scopes, iat, exp } =
        verify.presentation(&req.presentation).await.map_err(|e| {
            if e.contains("revoked") {
                PayError::detail("revoked", e)
            } else {
                PayError::detail("verification_failed", e)
            }
        })?;
    let grant = pay_grant(&scopes, rail)?;
    if grant.currency != deployment_currency {
        return Err(PayError::detail("unsupported_currency", format!("cap is in {}", grant.currency)));
    }
    let Some(status) = &identity.warrant_status else {
        // v2 records always carry one; a v1 warrant cannot carry a cap anyway.
        return Err(PayError::detail("no_grant", "warrant has no status ref"));
    };
    let key = format!("{}#{}", status.uri, status.idx);
    if payee == identity.email {
        return Err(PayError::detail("bad_request", "payee is the payer"));
    }

    let now = db::now();
    db.with(|c| {
        let tx = c.transaction()?;
        {
            let c = &tx;
            Db::upsert_warrant(
                c,
                &WarrantRow {
                    key: key.clone(),
                    grantor: identity.email.clone(),
                    grantee: identity.grantee.clone(),
                    scope: grant.scope.clone(),
                    cap_minor: grant.cap_minor,
                    currency: grant.currency.clone(),
                    window_secs: grant.window_secs,
                    iat,
                    exp,
                    first_seen: now,
                    revoked_at: None,
                    agreement_id: None,
                    counterparties: None,
                },
            )?;
            let since = db::window_start(iat, grant.window_secs, now);
            let spent = Db::spent_since(c, &key, since)?;
            let remaining = grant.cap_minor - spent;
            if amount_minor > remaining {
                return Ok(Err(PayError {
                    error: "cap_exceeded",
                    detail: None,
                    remaining: Some(fmt_minor(remaining.max(0))),
                }));
            }
            let id = format!("rcpt_{}", random_id());
            // Debit the grantor's account; refuse to overdraw.
            if Db::adjust(c, &identity.email, deployment_currency, -amount_minor, "pay", Some(&id)).is_err() {
                return Ok(Err(PayError::new("insufficient_funds")));
            }
            Db::adjust(c, &payee, deployment_currency, amount_minor, "receive", Some(&id))?;
            Db::record_spend(c, &key, amount_minor, &id)?;
            let receipt = Receipt {
                id,
                kind: format!("pay:{}", grant.rail),
                grantor: identity.email.clone(),
                grantee: identity.grantee.clone(),
                warrant_key: Some(key.clone()),
                amount_minor,
                currency: deployment_currency.to_string(),
                payee: Some(payee.clone()),
                memo: req.memo.as_deref().map(|m| m.chars().take(200).collect()),
                at: now,
            };
            Db::insert_receipt(c, &receipt)?;
            tx.commit()?;
            Ok(Ok(receipt))
        }
    })
    .map_err(|e| PayError::detail("internal", e.to_string()))?
}

pub(crate) fn random_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut b);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use browserid_core::device::{
        AccessCert, Binding, Cap, DeviceCert, Holder, HolderMatcher, Purpose, ScopeParams, Warrant,
    };
    use browserid_core::{Assertion, KeyPair, StatusRef};
    use chrono::Duration;

    const ORIGIN: &str = "https://pay.test";
    const ISSUER: &str = "idp.test";

    fn cfg(issuer_kp: &KeyPair) -> Config {
        Config {
            port: 0,
            origin: ORIGIN.into(),
            database_path: ":memory:".into(),
            admin_token: "t".into(),
            broker: "https://idp.test".into(),
            accepted_fallbacks: vec![ISSUER.into()],
            session_secret: "s".into(),
            dev: true,
            trust_issuers: vec![(ISSUER.into(), issuer_kp.public_key().to_base64())],
            currency: "USD".into(),
            data_dir: String::new(), chain_audience: String::new(), handle: String::new(), fee: "0.50".into(), daemon: String::new(), repo: String::new(), index_origin: String::new(), judge_origin: String::new(),
        }
    }

    /// A delegated presentation: `grantor` authorizes `grantee` (an agent) at
    /// the custodian with the given scope entries.
    fn presentation(issuer_kp: &KeyPair, grantor: &str, grantee: &str, scopes: Vec<ScopeEntry>) -> String {
        let holder = "ag.k1";
        let access_kp = KeyPair::generate();
        let config_kp = KeyPair::generate();
        let access_cert = AccessCert::create(
            ISSUER, grantee, Holder::new(holder).unwrap(), &access_kp.public_key(),
            Duration::hours(1), issuer_kp, None,
        )
        .unwrap();
        let config_cert = DeviceCert::create(
            ISSUER, &config_kp.public_key(), Purpose::Authorization, Holder::new("br.main").unwrap(),
            vec![grantor.to_string()], Duration::days(90), issuer_kp, None,
        )
        .unwrap();
        let warrant = Warrant::create_v2(
            grantor, grantee,
            Binding::Holder { matcher: HolderMatcher::new(holder).unwrap() },
            ORIGIN, scopes, Duration::days(30), &config_kp,
            StatusRef { uri: "https://idp.test/status".into(), idx: 7 },
        )
        .unwrap();
        let assertion = Assertion::create(ORIGIN, Duration::minutes(5), &access_kp).unwrap();
        format!("{}~{}~{}~{}", access_cert.encoded(), assertion.encoded(), warrant.encoded(), config_cert.encoded())
    }

    fn capped(scope: &str, amount: &str, window: Option<&str>) -> ScopeEntry {
        ScopeEntry::Parameterized(ScopeParams {
            scope: scope.into(),
            mode: None,
            cap: Some(Cap { amount: amount.into(), currency: "USD".into(), window: window.map(String::from) }),
            counterparties: None,
            max_duration: None,
            min_reputation: None,
        })
    }

    fn req(presentation: &str, amount: &str, payee: &str) -> PayRequest {
        PayRequest { presentation: presentation.into(), amount: amount.into(), currency: "USD".into(), payee: payee.into(), memo: None }
    }

    #[tokio::test]
    async fn pays_within_cap_then_refuses_over_it() {
        let issuer_kp = KeyPair::generate();
        let cfg = cfg(&issuer_kp);
        let db = Db::open(":memory:").unwrap();
        let verify = Verify::new(&cfg).unwrap();
        db.with(|c| Db::adjust(c, "dan@x.test", "USD", 10_000, "credit", None)).unwrap();
        let p = presentation(&issuer_kp, "dan@x.test", "agent@x.test", vec![capped("pay:transfer", "20.00", Some("P30D"))]);

        let r = pay(&verify, &db, "USD", req(&p, "15.00", "shop@y.test")).await.unwrap();
        assert_eq!(r.amount_minor, 1500);
        assert_eq!(r.grantor, "dan@x.test");
        assert_eq!(r.grantee, "agent@x.test");
        assert_eq!(db.account("shop@y.test").unwrap().unwrap().balance_minor, 1500);
        assert_eq!(db.account("dan@x.test").unwrap().unwrap().balance_minor, 8500);

        let e = pay(&verify, &db, "USD", req(&p, "6.00", "shop@y.test")).await.err().unwrap();
        assert_eq!(e.error, "cap_exceeded");
        assert_eq!(e.remaining.as_deref(), Some("5.00"));
        // Exactly the remainder is fine.
        pay(&verify, &db, "USD", req(&p, "5.00", "shop@y.test")).await.unwrap();
        let e = pay(&verify, &db, "USD", req(&p, "0.01", "shop@y.test")).await.err().unwrap();
        assert_eq!(e.error, "cap_exceeded");
        assert_eq!(e.remaining.as_deref(), Some("0.00"));
    }

    #[tokio::test]
    async fn refuses_without_cap_or_grant_or_funds() {
        let issuer_kp = KeyPair::generate();
        let cfg = cfg(&issuer_kp);
        let db = Db::open(":memory:").unwrap();
        let verify = Verify::new(&cfg).unwrap();

        // A bare pay scope carries no cap → refused.
        let p = presentation(&issuer_kp, "dan@x.test", "agent@x.test", vec![ScopeEntry::Bare("pay:transfer".into())]);
        assert_eq!(pay(&verify, &db, "USD", req(&p, "1.00", "shop@y.test")).await.err().unwrap().error, "no_grant");
        // Some other scope → no grant.
        let p = presentation(&issuer_kp, "dan@x.test", "agent@x.test", vec![capped("contract:agreement", "5.00", None)]);
        assert_eq!(pay(&verify, &db, "USD", req(&p, "1.00", "shop@y.test")).await.err().unwrap().error, "no_grant");
        // Capped but unfunded account.
        let p = presentation(&issuer_kp, "dan@x.test", "agent@x.test", vec![capped("pay:transfer", "5.00", None)]);
        assert_eq!(pay(&verify, &db, "USD", req(&p, "1.00", "shop@y.test")).await.err().unwrap().error, "insufficient_funds");
        // Wrong audience → verification fails.
        let e = pay(&verify, &db, "USD", req("not~a~bundle~x", "1.00", "shop@y.test")).await.err().unwrap();
        assert_eq!(e.error, "verification_failed");
    }

    #[tokio::test]
    async fn two_warrants_are_two_budgets() {
        // Same grantor and agent, different status idx ⇒ independent counters.
        let issuer_kp = KeyPair::generate();
        let cfg = cfg(&issuer_kp);
        let db = Db::open(":memory:").unwrap();
        let verify = Verify::new(&cfg).unwrap();
        db.with(|c| Db::adjust(c, "dan@x.test", "USD", 10_000, "credit", None)).unwrap();
        let p = presentation(&issuer_kp, "dan@x.test", "agent@x.test", vec![capped("pay:transfer", "1.00", None)]);
        pay(&verify, &db, "USD", req(&p, "1.00", "shop@y.test")).await.unwrap();
        assert_eq!(pay(&verify, &db, "USD", req(&p, "1.00", "shop@y.test")).await.err().unwrap().error, "cap_exceeded");
        assert_eq!(db.warrants_for("dan@x.test").unwrap().len(), 1);
    }

    #[test]
    fn money_formatting() {
        assert_eq!(fmt_minor(0), "0.00");
        assert_eq!(fmt_minor(1999), "19.99");
        assert_eq!(fmt_minor(-5), "-0.05");
    }
}
