//! Bundle verification: the custodian is a relying party for its own origin.
//!
//! Production verifies the conformant way (DNSSEC-resolved issuer keys,
//! fail-closed status checks). Dev mode pins issuer keys and skips status —
//! never in production.

use crate::config::Config;
use anyhow::{anyhow, Result};
use browserid_core::device::{AccessPresentation, ScopeEntry};
use browserid_core::PublicKey;
use browserid_rp::{VerifiedIdentity, Verifier};

pub struct Verify {
    verifier: Verifier,
    dns: Option<browserid_dnssec::DnsFetcher>,
    cfg: Config,
}

/// What a verified payment presentation tells the custodian.
pub struct Verified {
    pub identity: VerifiedIdentity,
    /// The warrant's scope entries, parameters included.
    pub scopes: Vec<ScopeEntry>,
    pub iat: i64,
    pub exp: i64,
}

impl Verify {
    pub fn new(cfg: &Config) -> Result<Self> {
        Self::for_audience(cfg, &cfg.origin)
    }

    /// A verifier for one audience (the custodian's origin, or a contract).
    pub fn for_audience(cfg: &Config, audience: &str) -> Result<Self> {
        let mut v = Verifier::new(audience.to_string());
        for d in &cfg.accepted_fallbacks {
            v = v.accept_fallback(d.clone());
        }
        let dns = if cfg.dev {
            v = v.without_status_checks().allow_private_status_hosts();
            for (domain, key) in &cfg.trust_issuers {
                let pk = PublicKey::from_base64(key)
                    .map_err(|e| anyhow!("PAY_TRUST_ISSUER {domain}: {e}"))?;
                v = v.trust_issuer(domain.clone(), pk);
            }
            None
        } else {
            anyhow::ensure!(
                cfg.trust_issuers.is_empty(),
                "PAY_TRUST_ISSUER is dev-only (set PAY_DEV=1)"
            );
            Some(browserid_dnssec::DnsFetcher::new().map_err(|e| anyhow!(e))?)
        };
        Ok(Self { verifier: v, dns, cfg: cfg.clone() })
    }

    /// Verify a presentation for a different audience than this verifier's
    /// (contract audiences are per agreement).
    pub async fn presentation_for(&self, audience: &str, presentation: &str) -> Result<Verified, String> {
        if audience == self.verifier.audience() {
            return self.presentation(presentation).await;
        }
        let v = Self::for_audience(&self.cfg, audience).map_err(|e| e.to_string())?;
        v.presentation(presentation).await
    }

    pub async fn presentation(&self, presentation: &str) -> Result<Verified, String> {
        let identity = match &self.dns {
            Some(dns) => self.verifier.verify_dnssec(presentation, dns).await,
            None => self.verifier.verify(presentation),
        }
        .map_err(|e| e.to_string())?;
        // The RP view projects scopes to strings; the parameters live on the
        // warrant itself. Re-parse (already verified above) to read them.
        let pres = AccessPresentation::parse(presentation).map_err(|e| e.to_string())?;
        let claims = pres.warrant.claims();
        Ok(Verified {
            identity,
            scopes: claims.scopes.clone(),
            iat: claims.iat,
            exp: claims.exp,
        })
    }
}
