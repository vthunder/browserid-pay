//! Presentation verification: the judge is a relying party for its own origin
//! (same shape as the custodian's). Production: DNSSEC-resolved issuers and
//! fail-closed status; dev: pinned issuers, no status checks.

use crate::config::Config;
use anyhow::{anyhow, Result};
use browserid_core::PublicKey;
use browserid_rp::{VerifiedIdentity, Verifier};

pub struct Verify {
    verifier: Verifier,
    dns: Option<browserid_dnssec::DnsFetcher>,
}

impl Verify {
    pub fn new(cfg: &Config) -> Result<Self> {
        let mut v = Verifier::new(cfg.origin.clone());
        for d in &cfg.accepted_fallbacks {
            v = v.accept_fallback(d.clone());
        }
        let dns = if cfg.dev {
            v = v.without_status_checks().allow_private_status_hosts();
            for (domain, key) in &cfg.trust_issuers {
                let pk = PublicKey::from_base64(key).map_err(|e| anyhow!("JUDGE_TRUST_ISSUER {domain}: {e}"))?;
                v = v.trust_issuer(domain.clone(), pk);
            }
            None
        } else {
            anyhow::ensure!(cfg.trust_issuers.is_empty(), "JUDGE_TRUST_ISSUER is dev-only (set JUDGE_DEV=1)");
            Some(browserid_dnssec::DnsFetcher::new().map_err(|e| anyhow!(e))?)
        };
        Ok(Self { verifier: v, dns })
    }

    pub async fn presentation(&self, presentation: &str) -> Result<VerifiedIdentity, String> {
        match &self.dns {
            Some(dns) => self.verifier.verify_dnssec(presentation, dns).await,
            None => self.verifier.verify(presentation),
        }
        .map_err(|e| e.to_string())
    }
}
