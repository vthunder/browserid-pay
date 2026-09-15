//! Process configuration, from the environment (dokku `config:set`).

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    /// Listen port.
    pub port: u16,
    /// Public origin — the audience every warrant and assertion must name.
    pub origin: String,
    /// SQLite path.
    pub database_path: String,
    /// Bearer token for the admin API (test credit). Empty disables it.
    pub admin_token: String,
    /// Broker origin serving `include.js` for the ledger UI's sign-in.
    pub broker: String,
    /// Fallback IdPs this custodian accepts (spec §8.1).
    pub accepted_fallbacks: Vec<String>,
    /// HMAC key for the session cookie.
    pub session_secret: String,
    /// Dev mode: pinned-issuer verification (`PAY_TRUST_ISSUER=domain=key`)
    /// with no status checks, and private status hosts allowed. Never in
    /// production.
    pub dev: bool,
    /// `domain=base64url-ed25519-key` pairs pinned as accepted fallbacks for
    /// the dev path (comma separated).
    pub trust_issuers: Vec<(String, String)>,
    /// Account currency (ISO 4217). One currency per deployment in v1.
    pub currency: String,
    /// Where the custodian's identity credential lives (dokku: /data).
    pub data_dir: String,
    /// The SBO database the custodian writes to (audience of its chain warrant).
    pub chain_audience: String,
    /// The handle the custodian asks the approver for.
    pub handle: String,
    /// The custodian's escrow fee (decimal string), published in its listing.
    pub fee: String,
    /// The sbo-daemon that reads the database and submits writes for it.
    pub daemon: String,
    /// Repo selector passed to the daemon (defaults to the chain audience).
    pub repo: String,
}

impl Config {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| env::var(k).unwrap_or_else(|_| d.to_string());
        let port: u16 = get("PAY_PORT", "3300").parse().expect("PAY_PORT");
        let dev = matches!(get("PAY_DEV", "").as_str(), "1" | "true" | "yes");
        let trust_issuers = get("PAY_TRUST_ISSUER", "")
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| {
                let (d, k) = s.trim().split_once('=')?;
                Some((d.to_string(), k.to_string()))
            })
            .collect();
        let session_secret = env::var("PAY_SESSION_SECRET").unwrap_or_else(|_| {
            if !dev {
                tracing::warn!("PAY_SESSION_SECRET unset: sessions will not survive a restart");
            }
            let mut buf = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut buf);
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, buf)
        });
        Self {
            port,
            origin: get("PAY_ORIGIN", &format!("http://localhost:{port}")),
            database_path: get("DATABASE_PATH", "custodian.db"),
            admin_token: get("ADMIN_TOKEN", ""),
            broker: get("BROKER", "https://browserid.me"),
            accepted_fallbacks: get("PAY_ACCEPTED_FALLBACKS", "browserid.me")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            session_secret,
            dev,
            trust_issuers,
            currency: get("PAY_CURRENCY", "USD"),
            data_dir: get("PAY_DATA_DIR", "./pay-data"),
            chain_audience: get("PAY_CHAIN_AUDIENCE", "sbo+raw://avail:turing:530/"),
            handle: get("PAY_HANDLE", "escrow"),
            fee: get("PAY_FEE", "0.50"),
            daemon: get("DAEMON", "https://da.sandmill.org"),
            repo: env::var("REPO").unwrap_or_else(|_| get("PAY_CHAIN_AUDIENCE", "sbo+raw://avail:turing:530/")),
        }
    }
}
