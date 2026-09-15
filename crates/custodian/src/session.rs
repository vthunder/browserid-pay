//! Signed-cookie sessions for the ledger UI (HMAC-SHA256, no server state).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const COOKIE: &str = "pay_session";
const TTL_SECS: i64 = 12 * 3600;

pub fn issue(secret: &str, identity: &str) -> String {
    let exp = chrono::Utc::now().timestamp() + TTL_SECS;
    let payload = B64.encode(format!("{identity}|{exp}"));
    let sig = sign(secret, &payload);
    format!("{payload}.{sig}")
}

pub fn verify(secret: &str, token: &str) -> Option<String> {
    let (payload, sig) = token.split_once('.')?;
    let want = sign(secret, payload);
    if want.len() != sig.len() || !constant_eq(want.as_bytes(), sig.as_bytes()) {
        return None;
    }
    let raw = String::from_utf8(B64.decode(payload).ok()?).ok()?;
    let (identity, exp) = raw.rsplit_once('|')?;
    let exp: i64 = exp.parse().ok()?;
    if exp < chrono::Utc::now().timestamp() {
        return None;
    }
    Some(identity.to_string())
}

fn sign(secret: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Pull our cookie out of a `Cookie:` header value.
pub fn from_header(header: &str) -> Option<&str> {
    header
        .split(';')
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(COOKIE).and_then(|r| r.strip_prefix('=')))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_tamper() {
        let t = issue("k", "a@b");
        assert_eq!(verify("k", &t).as_deref(), Some("a@b"));
        assert!(verify("other", &t).is_none());
        let mut bad = t.clone();
        bad.push('x');
        assert!(verify("k", &bad).is_none());
        assert_eq!(from_header("x=1; pay_session=abc.def; y=2"), Some("abc.def"));
    }
}
