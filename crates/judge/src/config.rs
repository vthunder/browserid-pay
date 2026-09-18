//! Process configuration, from the environment (dokku `config:set`).

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    /// Public origin — the audience a party's presentation must name.
    pub origin: String,
    /// Persistent state dir: cases.db, judge-credential.json, judge-seal-key.json.
    pub data_dir: String,
    /// Broker that provisions the judge's identity and serves include.js.
    pub broker: String,
    /// Handle requested at provisioning (the approver may rename it).
    pub handle: String,
    /// The SBO database this judge rules on (warrant audience for chain writes).
    pub chain_audience: String,
    /// Daemon for reads (`/v1/*`).
    pub daemon: String,
    /// Custodians the judge may deliver rulings to (warrant audiences requested at provisioning).
    pub custodians: Vec<String>,
    pub accepted_fallbacks: Vec<String>,
    /// Published price per ruling (decimal, ISO 4217).
    pub price_amount: String,
    pub price_currency: String,
    pub anthropic_api_key: String,
    pub anthropic_model: String,
    /// Dev mode: pinned issuers, no status checks, no chain writes unless AVAIL_* is set.
    pub dev: bool,
    pub trust_issuers: Vec<(String, String)>,
    /// How long the counterparty has to respond to a dispute before the judge
    /// rules on the record as it stands (seconds). Default a day; 60s in dev.
    /// The disputed party's response window, in BLOCKS (~20s each). Blocks,
    /// not seconds, because this window decides whether a party forfeited
    /// their chance to answer (browserid-pay-e1ou). The env var is renamed
    /// with it: an operator's old `JUDGE_RESPONSE_WINDOW=3600` read as blocks
    /// would be a 20-hour window, silently.
    pub response_window_blocks: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| env::var(k).unwrap_or_else(|_| d.to_string());
        let port: u16 = get("JUDGE_PORT", "3400").parse().expect("JUDGE_PORT");
        let dev = matches!(get("JUDGE_DEV", "").as_str(), "1" | "true" | "yes");
        let list = |k: &str, d: &str| -> Vec<String> {
            get(k, d).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        Self {
            port,
            origin: get("JUDGE_ORIGIN", &format!("http://localhost:{port}")),
            data_dir: get("JUDGE_DATA_DIR", "./judge-data"),
            broker: get("BROKER", "https://browserid.me"),
            handle: get("JUDGE_HANDLE", "judge"),
            chain_audience: get("JUDGE_CHAIN_AUDIENCE", chain::DEFAULT_REPO),
            daemon: get("SBO_DAEMON", "https://da.sandmill.org"),
            custodians: list("JUDGE_CUSTODIANS", "https://pay.dsp.fyi"),
            accepted_fallbacks: list("JUDGE_ACCEPTED_FALLBACKS", "browserid.me"),
            price_amount: get("JUDGE_PRICE", "5.00"),
            price_currency: get("JUDGE_CURRENCY", "USD"),
            anthropic_api_key: get("ANTHROPIC_API_KEY", ""),
            anthropic_model: get("JUDGE_MODEL", "claude-fable-5-1"),
            dev,
            response_window_blocks: get("JUDGE_RESPONSE_WINDOW_BLOCKS", if dev { "3" } else { "180" }).parse().expect("JUDGE_RESPONSE_WINDOW_BLOCKS"),
            trust_issuers: get("JUDGE_TRUST_ISSUER", "")
                .split(',')
                .filter_map(|s| s.trim().split_once('=').map(|(a, b)| (a.to_string(), b.to_string())))
                .collect(),
        }
    }
}
