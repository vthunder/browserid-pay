//! Bundle verification: the relay is a relying party for its own origin
//! (DNSSEC path in production, pinned issuers in dev). Same shape as the
//! custodian's verifier.

use crate::config::Config;
use anyhow::{anyhow, Result};
use browserid_core::PublicKey;
use browserid_rp::{VerifiedIdentity, Verifier};

pub struct Verify {
    verifier: Verifier,
    dns: Option<browserid_dnssec::DnsFetcher>,
}

/// The acting identity (grantee) and the warrant's scope strings.
pub struct Verified {
    pub grantee: String,
    #[allow(dead_code)]
    pub grantor: String,
    pub scopes: Vec<String>,
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
                let pk = PublicKey::from_base64(key).map_err(|e| anyhow!("RELAY_TRUST_ISSUER {domain}: {e}"))?;
                v = v.trust_issuer(domain.clone(), pk);
            }
            None
        } else {
            anyhow::ensure!(cfg.trust_issuers.is_empty(), "RELAY_TRUST_ISSUER is dev-only (set RELAY_DEV=1)");
            Some(browserid_dnssec::DnsFetcher::new().map_err(|e| anyhow!(e))?)
        };
        Ok(Self { verifier: v, dns })
    }

    pub async fn presentation(&self, presentation: &str) -> Result<Verified, String> {
        let id: VerifiedIdentity = match &self.dns {
            Some(dns) => self.verifier.verify_dnssec(presentation, dns).await,
            None => self.verifier.verify(presentation),
        }
        .map_err(|e| e.to_string())?;
        Ok(Verified { grantee: id.grantee, grantor: id.email, scopes: id.scopes })
    }
}
