//! The judge's sealing key: parties seal the private terms to it so the
//! custodian can hold a copy nobody but the adjudicator can read.
//!
//! Scheme **`sealed-x25519-v1`** (docs/specs/agreement-v1.md, "Sealed copy";
//! sealer reference: `sdk/agreements/src/seal.mjs`):
//!   epk    = fresh ephemeral X25519 public key (32 bytes)
//!   shared = X25519(esk, rpk)
//!   key    = HKDF-SHA256(ikm = shared, salt = epk ‖ rpk, info = "agreement.v1 seal", 32 bytes)
//!   ct     = ChaCha20-Poly1305(key, nonce = 12 zero bytes, aad = epk, plaintext) ‖ tag
//!   sealed = base64url(epk ‖ ct)
//! The listing publishes `sealing_key` = base64url of the raw X25519 public key.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use std::path::Path;
use x25519_dalek::{PublicKey, StaticSecret};

const INFO: &[u8] = b"agreement.v1 seal";

pub struct SealKey {
    secret: StaticSecret,
}

fn derive_key(shared: &[u8; 32], epk: &[u8; 32], rpk: &[u8; 32]) -> [u8; 32] {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(epk);
    salt[32..].copy_from_slice(rpk);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut key = [0u8; 32];
    hk.expand(INFO, &mut key).expect("32 bytes is a valid HKDF length");
    key
}

impl SealKey {
    /// Load from `path`, generating (mode 600) on first use.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            let raw = B64.decode(v["secret"].as_str().context("seal key file: no `secret`")?)?;
            let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| anyhow!("seal key must be 32 bytes"))?;
            return Ok(Self::from_secret(arr));
        }
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({ "scheme": "sealed-x25519-v1", "secret": B64.encode(seed) }))?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self::from_secret(seed))
    }

    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self { secret: StaticSecret::from(secret) }
    }

    /// base64url public key, as published in `sealing_key`.
    pub fn public_b64(&self) -> String {
        B64.encode(PublicKey::from(&self.secret).as_bytes())
    }

    pub fn unseal(&self, sealed_b64: &str) -> Result<Vec<u8>> {
        let bytes = B64.decode(sealed_b64.trim()).context("sealed_terms: bad base64url")?;
        if bytes.len() < 32 + 16 {
            anyhow::bail!("sealed_terms: too short");
        }
        let epk: [u8; 32] = bytes[..32].try_into().unwrap();
        let ct = &bytes[32..];
        let shared = self.secret.diffie_hellman(&PublicKey::from(epk));
        let rpk = *PublicKey::from(&self.secret).as_bytes();
        let key = derive_key(shared.as_bytes(), &epk, &rpk);
        let cipher = ChaCha20Poly1305::new((&key).into());
        cipher
            .decrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: ct, aad: &epk })
            .map_err(|_| anyhow!("sealed_terms: cannot unseal with the judge's key"))
    }
}

/// Seal `plaintext` to a published `sealing_key` (what an SDK does).
pub fn seal_to(public_b64: &str, plaintext: &[u8]) -> Result<String> {
    let raw = B64.decode(public_b64.trim()).context("sealing_key: bad base64url")?;
    let rpk: [u8; 32] = raw.as_slice().try_into().map_err(|_| anyhow!("sealing_key must be 32 bytes"))?;
    let esk = StaticSecret::random_from_rng(rand::thread_rng());
    let epk = *PublicKey::from(&esk).as_bytes();
    let shared = esk.diffie_hellman(&PublicKey::from(rpk));
    let key = derive_key(shared.as_bytes(), &epk, &rpk);
    let cipher = ChaCha20Poly1305::new((&key).into());
    let ct = cipher
        .encrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: plaintext, aad: &epk })
        .map_err(|_| anyhow!("seal failed"))?;
    let mut out = epk.to_vec();
    out.extend_from_slice(&ct);
    Ok(B64.encode(out))
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
        // Tampered aad/ciphertext fails.
        let mut raw = B64.decode(&sealed).unwrap();
        raw[40] ^= 1;
        assert!(k.unseal(&B64.encode(raw)).is_err());
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
