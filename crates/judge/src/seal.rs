//! The judge's sealing key: parties seal the private terms to it so the
//! custodian can hold a copy nobody but the adjudicator can read.
//!
//! Scheme: libsodium **sealed box** (X25519 + XSalsa20-Poly1305, ephemeral
//! sender key; `crypto_box::SealedBox`). Wire form: base64url (no pad) of
//! the sealed bytes. The public key is published in the provider listing as
//! `sealing_key` = base64url of the 32-byte X25519 public key.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use crypto_box::{PublicKey, SecretKey};
use std::path::Path;

pub struct SealKey {
    secret: SecretKey,
}

impl SealKey {
    /// Load from `path`, generating (mode 600) on first use.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            let raw = B64.decode(v["secret"].as_str().context("seal key file: no `secret`")?)?;
            let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| anyhow!("seal key must be 32 bytes"))?;
            return Ok(Self { secret: SecretKey::from(arr) });
        }
        let secret = SecretKey::generate(&mut crypto_box::aead::OsRng);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({ "secret": B64.encode(secret.to_bytes()) }))?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self { secret })
    }

    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self { secret: SecretKey::from(secret) }
    }

    /// base64url public key, as published in `sealing_key`.
    pub fn public_b64(&self) -> String {
        B64.encode(self.secret.public_key().as_bytes())
    }

    pub fn unseal(&self, sealed_b64: &str) -> Result<Vec<u8>> {
        let bytes = B64.decode(sealed_b64.trim()).context("sealed_terms: bad base64url")?;
        self.secret.unseal(&bytes).map_err(|_| anyhow!("sealed_terms: cannot unseal with the judge's key"))
    }
}

/// Seal `plaintext` to a published `sealing_key` (what an SDK does).
pub fn seal_to(public_b64: &str, plaintext: &[u8]) -> Result<String> {
    let raw = B64.decode(public_b64.trim()).context("sealing_key: bad base64url")?;
    let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| anyhow!("sealing_key must be 32 bytes"))?;
    let pk = PublicKey::from(arr);
    let sealed = pk.seal(&mut crypto_box::aead::OsRng, plaintext).map_err(|_| anyhow!("seal failed"))?;
    Ok(B64.encode(sealed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_round_trip_and_wrong_key() {
        let k = SealKey::from_secret([7u8; 32]);
        let sealed = seal_to(&k.public_b64(), b"{\"price\":1}").unwrap();
        assert_eq!(k.unseal(&sealed).unwrap(), b"{\"price\":1}");
        let other = SealKey::from_secret([9u8; 32]);
        assert!(other.unseal(&sealed).is_err());
        assert!(k.unseal("not-base64!!").is_err());
    }

    #[test]
    fn persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("judge-seal-{}", std::process::id()));
        let p = dir.join("seal.json");
        let a = SealKey::load_or_create(&p).unwrap();
        let b = SealKey::load_or_create(&p).unwrap();
        assert_eq!(a.public_b64(), b.public_b64());
        let _ = std::fs::remove_dir_all(dir);
    }
}
