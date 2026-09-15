//! Process configuration, from the environment (dokku `config:set`).

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    /// Public origin — the audience every presentation must name.
    pub origin: String,
    pub database_path: String,
    /// Fallback IdPs this relay accepts (spec §8.1).
    pub accepted_fallbacks: Vec<String>,
    /// Dev mode: pinned issuers (`RELAY_TRUST_ISSUER=domain=key`), no status
    /// checks, private status hosts. Never in production.
    pub dev: bool,
    pub trust_issuers: Vec<(String, String)>,
}

impl Config {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| env::var(k).unwrap_or_else(|_| d.to_string());
        let port: u16 = get("RELAY_PORT", "3400").parse().expect("RELAY_PORT");
        let dev = matches!(get("RELAY_DEV", "").as_str(), "1" | "true" | "yes");
        let trust_issuers = get("RELAY_TRUST_ISSUER", "")
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| {
                let (d, k) = s.trim().split_once('=')?;
                Some((d.to_string(), k.to_string()))
            })
            .collect();
        Self {
            port,
            origin: get("RELAY_ORIGIN", &format!("http://localhost:{port}")),
            database_path: get("DATABASE_PATH", "relay.db"),
            accepted_fallbacks: get("RELAY_ACCEPTED_FALLBACKS", "browserid.me")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            dev,
            trust_issuers,
        }
    }
}
