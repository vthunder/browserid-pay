//! Genesis / regenesis tooling for the browserid-pay SBO database.
//!
//! Modelled on mingo's `mingo genesis` + `deploy/GENESIS.md`, reduced to what
//! this database needs. The chain is **Mode A** (self-signed `sys`, no domain
//! object): dsp.fyi runs no browserid primary, so every identity here
//! (`escrow@dsp.fyi`, `judge@dsp.fyi`, agents, humans) is a T0 email attributed
//! through the pinned fallback broker (`/sys/trust/brokers`). A `domain.v1`
//! exists to certify T1 (`*@<repo-domain>`) names or to self-certify a
//! repository root of trust, neither of which applies (Identity Spec §Domain
//! Objects, Genesis Spec §Genesis Modes).
//!
//! Subcommands (see `deploy/GENESIS.md` for the runbook):
//!   keygen   — generate the sys key into a mode-600 file (refuses to overwrite)
//!   build    — build `genesis.wire`; prints the genesis hash
//!   submit   — submit a wire batch to TurboDA (AVAIL_TURBODA_KEY from env)
//!   status   — TurboDA submission status (block number once included)
//!   identity — canonical identity + `_sbo` record + daemon repos.json entry
//!   dnssec-refresh — refresh `/sys/dnssec/<domain>` via the daemon's capture
//!   policy-update — re-sign the current root policy with the sys key and
//!                   submit it through the daemon (a governed update; no regenesis)

use anyhow::{bail, Context, Result};
use base64::Engine;
use clap::{Parser, Subcommand};
use sbo_core::crypto::SigningKey;
use sbo_core::presets::{dnssec_self_auth_policy_entries, set_dnssec, set_trust_brokers, signed_object};
use sbo_core::wire;
use std::path::{Path, PathBuf};

const TURBO_STAGING: &str = "https://staging.turbo-api.availproject.org";
const TURBO_PROD: &str = "https://turbo-api.availproject.org";
const DEFAULT_DAEMON: &str = "https://da.sandmill.org";

#[derive(Parser)]
#[command(name = "dsp-genesis", about = "browserid-pay SBO database genesis tooling")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the sys signing key into `--out` (mode 600). Refuses to overwrite.
    Keygen {
        #[arg(long, default_value = "~/secure-backup/dsp-sys.key")]
        out: String,
    },
    /// Build the genesis batch.
    Build {
        /// Sys key file (`{"secret_key":"<hex seed>"}`).
        #[arg(long, default_value = "~/secure-backup/dsp-sys.key")]
        key_file: String,
        /// Fallback broker pinned in /sys/trust/brokers.
        #[arg(long, default_value = "browserid.me")]
        broker: String,
        /// Ecosystem contribution recipient published in /sys/contribution.
        #[arg(long, default_value = "fund@dsp.fyi")]
        contribution_recipient: String,
        /// Suggested contribution percentage published in /sys/contribution.
        #[arg(long, default_value = "0.5")]
        contribution_percentage: f64,
        /// Daemon whose `/v1/dnssec` captures the broker's DNSSEC proof for
        /// `/sys/dnssec/<broker>` at genesis (so broker-attributed writes work
        /// from block one). `--no-dnssec` skips it.
        #[arg(long, default_value = DEFAULT_DAEMON)]
        daemon: String,
        #[arg(long)]
        no_dnssec: bool,
        #[arg(long, default_value = "genesis.wire")]
        out: String,
    },
    /// Submit a wire batch to TurboDA. Reads AVAIL_TURBODA_KEY (and AVAIL_NETWORK
    /// for the endpoint: turing → staging, mainnet → production) from the env.
    Submit {
        #[arg(long, default_value = "genesis.wire")]
        file: String,
    },
    /// Show a TurboDA submission's status (block number once finalized).
    Status { submission_id: String },
    /// Print the canonical identity, the `_sbo.dsp.fyi` TXT record, and the
    /// daemon repos.json entry for a genesis landed at `--block`.
    Identity {
        #[arg(long, default_value = "genesis.wire")]
        file: String,
        #[arg(long)]
        block: u64,
        #[arg(long, default_value = "avail:turing")]
        chain: String,
        #[arg(long, default_value_t = 530)]
        app_id: u32,
        #[arg(long, default_value = "dsp.fyi")]
        domain: String,
        #[arg(long, default_value = DEFAULT_DAEMON)]
        node: String,
    },
    /// Refresh `/sys/dnssec/<domain>` on chain if the daemon reports it stale:
    /// capture via the daemon, sign with a throwaway key (self-authorizing
    /// object), submit to TurboDA. Cron-friendly; exits non-zero on failure.
    /// Re-sign `/sys/policies/root` (the policy this binary builds) with the sys
    /// key and submit it as an update through the daemon's per-repo submit
    /// (`POST /v1/submit?repo=`). Admin holds `govern` on `/**`, so this is an
    /// ordinary governed write — the way to change policy without a regenesis.
    /// Polls until the daemon serves the new payload.
    PolicyUpdate {
        #[arg(long, default_value = "~/secure-backup/dsp-sys.key")]
        key_file: String,
        #[arg(long, default_value = DEFAULT_DAEMON)]
        daemon: String,
        #[arg(long, default_value = "sbo+raw://avail:turing:530/")]
        repo: String,
        /// Print the policy and the wire size without submitting.
        #[arg(long)]
        dry_run: bool,
        /// Submit straight to TurboDA (AVAIL_TURBODA_KEY) instead of the daemon.
        #[arg(long)]
        direct: bool,
    },
    DnssecRefresh {
        #[arg(long, default_value = DEFAULT_DAEMON)]
        daemon: String,
        /// Repo selector passed to the daemon (it follows several databases).
        #[arg(long, default_value = "sbo+raw://avail:turing:530/")]
        repo: String,
        #[arg(default_values_t = vec!["browserid.me".to_string()])]
        domains: Vec<String>,
    },
}

fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn load_key(path: &str) -> Result<SigningKey> {
    let path = expand(path);
    let s = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&s)?;
    let hexs = v["secret_key"].as_str().context("key file missing `secret_key`")?;
    let raw = hex::decode(hexs.trim())?;
    let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| anyhow::anyhow!("secret_key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&arr))
}

fn pubkey_str(k: &SigningKey) -> String {
    format!("ed25519:{}", hex::encode(k.public_key().bytes))
}

fn turbo_endpoint() -> String {
    match std::env::var("AVAIL_NETWORK").as_deref() {
        Ok("mainnet") => TURBO_PROD.to_string(),
        _ => TURBO_STAGING.to_string(),
    }
}

fn turbo_key() -> Result<String> {
    std::env::var("AVAIL_TURBODA_KEY").context("AVAIL_TURBODA_KEY not set (source .env)")
}

async fn turbo_submit(data: &[u8]) -> Result<String> {
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/submit_raw_data", turbo_endpoint()))
        .header("x-api-key", turbo_key()?)
        .header("Content-Type", "application/octet-stream")
        .body(data.to_vec())
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("TurboDA submit failed ({}): {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await?;
    v["submission_id"].as_str().map(String::from).context("no submission_id in response")
}

fn encode_repo(repo: &str) -> String {
    // `+` in a query string decodes to a space; the repo selector carries one.
    repo.replace('+', "%2B")
}

/// Submit a wire batch through the daemon (`POST /v1/submit?repo=`): the
/// daemon holds this repo's TurboDA credentials and refreshes any issuer
/// DNSSEC evidence the write needs before it. Returns the submission id.
async fn daemon_submit(daemon: &str, repo: &str, data: &[u8]) -> Result<serde_json::Value> {
    let url = format!("{}/v1/submit?repo={}", daemon.trim_end_matches('/'), encode_repo(repo));
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/octet-stream")
        .body(data.to_vec())
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("daemon submit failed ({}): {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

async fn daemon_get_object(daemon: &str, repo: &str, path: &str, id: &str) -> Result<Option<serde_json::Value>> {
    let url = format!(
        "{}/v1/object?repo={}&path={}&id={}",
        daemon.trim_end_matches('/'),
        encode_repo(repo),
        path,
        id
    );
    let resp = reqwest::Client::new().get(&url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    Ok(Some(resp.error_for_status()?.json().await?))
}

async fn turbo_status(id: &str) -> Result<serde_json::Value> {
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/get_submission_info?submission_id={id}", turbo_endpoint()))
        .header("x-api-key", turbo_key()?)
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("TurboDA status failed ({}): {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Capture a fresh RFC 9102 proof for `_browserid.<domain>` through the
/// daemon's `/v1/dnssec` (asking for validity far enough ahead that it always
/// captures rather than reporting the on-chain copy fresh).
async fn capture_proof(daemon: &str, domain: &str, repo: Option<&str>, force: bool) -> Result<Option<Vec<u8>>> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let needed_by = if force { now + 365 * 24 * 3600 } else { now };
    let mut url = format!("{}/v1/dnssec?domain={domain}&needed_by={needed_by}&margin=3600", daemon.trim_end_matches('/'));
    if let Some(r) = repo {
        // `+` in a query string decodes to a space; the repo selector carries one.
        url.push_str(&format!("&repo={}", r.replace('+', "%2B")));
    }
    let v: serde_json::Value = reqwest::Client::new().get(&url).send().await?.error_for_status()?.json().await?;
    if !v["needs_refresh"].as_bool().unwrap_or(true) {
        return Ok(None);
    }
    let b64 = v["proof_b64"].as_str().context("daemon reported needs_refresh but returned no proof")?;
    Ok(Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64)?))
}

/// The root policy (policy.v2) for this database. See deploy/GENESIS.md
/// ("Policy") for what each grant is for and what it cannot express.
/// The chain-enforced contribution floor: 1 basis point, or at least one cent,
/// whichever is larger. Deliberately tiny — it exists so that "contributes
/// nothing" is not expressible, not to price the ecosystem. The *suggested*
/// contribution (published in `/sys/contribution`, 0.5% by default) is what an
/// agent should actually offer.
const MIN_CONTRIBUTION_PERCENTAGE: f64 = 0.01;
const MIN_CONTRIBUTION_AMOUNT_MINOR: i64 = 1;
/// The same floor as a decimal, for the policy's numeric comparison.
const MIN_CONTRIBUTION_AMOUNT: f64 = 0.01;

/// Minor units as a decimal string (money is carried as decimal strings).
fn fmt_minor_str(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, minor % 100)
}

fn root_policy(sys: &SigningKey) -> serde_json::Value {
    let (dnssec_grant, dnssec_restriction) = dnssec_self_auth_policy_entries();
    let sys_pubkey = pubkey_str(sys);
    serde_json::json!({
        // `admin` is the sys KEY (never a name — a name could be IdP-issued).
        "roles": { "admin": [{ "key": sys_pubkey }] },
        "grants": [
            // Name claims: first-come create, owner-only update.
            { "to": "*", "can": ["create"], "on": "/sys/names/*" },
            { "to": "owner", "can": ["update", "delete"], "on": "/sys/names/*" },
            // Every signer owns its own top-level namespace: attestations live
            // at /<issuer>/attestations/<subject>/<type> (Attestation spec).
            { "to": "*", "can": ["post", "delete"], "on": "/$user/**" },
            // Provider listings: an identity may publish exactly its own entry,
            // at /providers/<kind>/<identity>/ (ID `listing`). Grant patterns
            // match an object's PATH, never path+id, so the identity is a path
            // segment (sbo-core policy/path.rs; bean browserid-pay-iovq: listing
            // layout fix, 2026-09-15).
            { "to": "*", "can": ["post"], "on": "/providers/custodians/$user/**" },
            { "to": "*", "can": ["post"], "on": "/providers/adjudicators/$user/**" },
            // Agreements: append-only. Any attributed identity may CREATE any
            // object under an agreement; nothing is ever updated (a second
            // proposal/acceptance/ruling at the same slot is a no-op under
            // first-valid-write-wins). Who may accept/rule/lock is decided by
            // consumers against the parties the proposal names — the policy
            // language cannot read another object's payload (GENESIS.md).
            { "to": "*", "can": ["create"], "on": "/agreements/**" },
            // Self-authorizing DNSSEC proof refresh (shared sbo fragment).
            dnssec_grant,
            // Governance: only the sys key may install/replace policy.
            { "to": { "role": "admin" }, "can": ["post", "transfer", "delete", "govern"], "on": "/**" }
        ],
        "restrictions": [
            { "on": "/providers/**", "require": { "schema": "provider.v1", "max_size": 16384 } },
            { "on": "/agreements/**", "require": { "max_size": 65536 } },
            // The ecosystem contribution floor. Scoped by `id` to the proposal:
            // an agreement's other objects (acceptance, lock, ruling, evidence)
            // share the container and carry no contribution, so without the id
            // this would reject all of them. Requires the object-id + payload-
            // field policy features (sbo-pu34) — a daemon older than those
            // IGNORES both fields, silently not enforcing this, so never publish
            // this policy to a chain whose validators have not been upgraded.
            //
            // `fn` is pinned because without it a proposer picks `min` and pays
            // the smaller operand. Flooring both operands is conservative for a
            // floor on max() — stricter than strictly necessary, and sound. The
            // custodian does the exact arithmetic at lock (it has the price,
            // which is sealed on chain in commitment mode).
            {
                "on": "/agreements/*/",
                "id": "proposal",
                "require": { "fields": [
                    { "pointer": "/contribution/fn", "eq": "max" },
                    { "pointer": "/contribution/amount", "min": MIN_CONTRIBUTION_AMOUNT },
                    { "pointer": "/contribution/percentage", "min": MIN_CONTRIBUTION_PERCENTAGE }
                ]}
            },
            dnssec_restriction
        ]
    })
}

/// Build the genesis batch, in dependency order (everything before the root
/// policy is admitted in genesis mode):
///   0. /sys/dnssec/<broker>     (optional; sys-signed; self-authenticating)
///   1. /sys/names/sys           (self-signed identity.v1 — Mode A)
///   2. /sys/trust/brokers       (the attribution anchor)
///   3. /sys/contribution        (suggested ecosystem contribution)
///   4. /sys/policies/root       (LAST)
fn build_genesis(sys: &SigningKey, broker: &str, dnssec_proof: Option<&[u8]>, recipient: &str, pct: f64) -> Vec<u8> {
    let mut batch = Vec::new();
    if let Some(proof) = dnssec_proof {
        batch.extend(set_dnssec(sys, broker, proof));
    }
    // 1. sys identity (self-signed). Reuse the core preset's exact shape by
    // taking the first message of the Mode A preset batch.
    let preset = sbo_core::presets::genesis(sys);
    let msgs = wire::parse_batch(&preset).expect("preset genesis parses");
    batch.extend(wire::serialize(&msgs[0]));
    // 2. brokers
    batch.extend(set_trust_brokers(sys, &[broker]));
    // 3. contribution suggestion
    let contribution = serde_json::to_vec(&serde_json::json!({
        "recipient": recipient,
        "fn": "max",
        "suggested_amount": fmt_minor_str(MIN_CONTRIBUTION_AMOUNT_MINOR),
        "suggested_percentage": pct,
        "minimum": {
            "fn": "max",
            "amount": fmt_minor_str(MIN_CONTRIBUTION_AMOUNT_MINOR),
            "percentage": MIN_CONTRIBUTION_PERCENTAGE
        },
        "note": "The suggestion is what an agent should offer; `minimum` is the floor the root policy enforces on every proposal."
    }))
    .unwrap();
    batch.extend(signed_object(sys, "/sys/", "contribution", "contribution.v1", "application/json", contribution, None, None, None));
    // 4. root policy
    let policy = serde_json::to_vec(&root_policy(sys)).unwrap();
    batch.extend(signed_object(sys, "/sys/policies/", "root", "policy.v2", "application/json", policy, None, None, None));
    batch
}

fn repo_id(uri: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(uri.as_bytes())[..8])
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Keygen { out } => {
            let path = expand(&out);
            if path.exists() {
                bail!("{} exists — refusing to overwrite a sys key (regenesis with a new key is a deliberate step; move the old file aside first)", path.display());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let key = SigningKey::generate();
            let body = serde_json::to_vec_pretty(&serde_json::json!({ "secret_key": hex::encode(key.to_bytes()) }))?;
            std::fs::write(&path, body)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            println!("✓ wrote sys key to {} (mode 600). pubkey {}", path.display(), pubkey_str(&key));
            println!("  BACK THIS FILE UP OUT OF BAND. Losing it means a regenesis (see deploy/GENESIS.md).");
        }
        Commands::Build { key_file, broker, contribution_recipient, contribution_percentage, daemon, no_dnssec, out } => {
            let sys = load_key(&key_file)?;
            let proof = if no_dnssec {
                None
            } else {
                let p = capture_proof(&daemon, &broker, None, true)
                    .await
                    .with_context(|| format!("capturing DNSSEC proof for {broker} via {daemon}"))?
                    .context("daemon returned no proof")?;
                println!("✓ captured RFC 9102 proof for _browserid.{broker}: {} bytes", p.len());
                Some(p)
            };
            let batch = build_genesis(&sys, &broker, proof.as_deref(), &contribution_recipient, contribution_percentage);
            std::fs::write(&out, &batch)?;
            let hash = sbo_core::genesis_hash_from_wire(&batch)?;
            let n = wire::parse_batch(&batch)?.len();
            println!("✓ wrote {out}: {} messages, {} bytes", n, batch.len());
            println!("  sys pubkey     {}", pubkey_str(&sys));
            println!("  genesis hash   {}", hash.to_string());
        }
        Commands::Submit { file } => {
            let data = std::fs::read(&file).with_context(|| format!("reading {file}"))?;
            let hash = sbo_core::genesis_hash_from_wire(&data).ok();
            println!("submitting {file} ({} bytes) to {} …", data.len(), turbo_endpoint());
            let id = turbo_submit(&data).await?;
            println!("✓ submission_id {id}");
            if let Some(h) = hash {
                println!("  batch hash     {}", h.to_string());
            }
            println!("  poll: dsp-genesis status {id}");
        }
        Commands::Status { submission_id } => {
            let v = turbo_status(&submission_id).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Identity { file, block, chain, app_id, domain, node } => {
            let data = std::fs::read(&file).with_context(|| format!("reading {file}"))?;
            let hash = sbo_core::genesis_hash_from_wire(&data)?.to_string();
            let bare = format!("sbo+raw://{chain}:{app_id}/");
            let (ns, reference) = chain.split_once(':').context("chain must be namespace:reference")?;
            println!("canonical identity  {chain}:{app_id}:{block}:{hash}");
            println!("bare repo URI       {bare}");
            println!("pinned URI          sbo+raw://{chain}:{app_id}@{block}/?genesis={hash}");
            println!();
            println!("_sbo.{domain}.  IN  TXT  \"v=sbo1 repo=sbo+raw://{chain}:{app_id}@{block}/ genesis={hash} node={node}\"");
            println!();
            let entry = serde_json::json!([{
                "id": repo_id(&bare),
                "uri": { "chain": { "namespace": ns, "reference": reference }, "app_id": app_id, "first_block": block, "path": null,
                          "query": { "genesis": null, "as_of": null, "content_hash": null, "content_type": null, "content_schema": null, "encoding": null, "size": null, "extra": {} } },
                "display_uri": bare,
                "path": "/data/repos/dsp",
                "head": block - 1,
                "created_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                "expected_genesis": hash.to_string()
            }]);
            println!("daemon repos.json entry (append to the array):");
            println!("{}", serde_json::to_string(&entry[0])?);
        }
        Commands::PolicyUpdate { key_file, daemon, repo, dry_run, direct } => {
            let sys = load_key(&key_file)?;
            let policy = root_policy(&sys);
            let payload = serde_json::to_vec(&policy)?;
            let wire_bytes = signed_object(&sys, "/sys/policies/", "root", "policy.v2", "application/json", payload.clone(), None, None, None);
            println!("{}", serde_json::to_string_pretty(&policy)?);
            println!("policy.v2 for /sys/policies/root: {} bytes payload, {} bytes wire, sys {}", payload.len(), wire_bytes.len(), pubkey_str(&sys));
            if dry_run {
                return Ok(());
            }
            let sub = if direct {
                let id = turbo_submit(&wire_bytes).await?;
                println!("✓ TurboDA submission_id {id}");
                id
            } else {
                let v = daemon_submit(&daemon, &repo, &wire_bytes).await?;
                println!("✓ daemon accepted: {}", serde_json::to_string(&v)?);
                v["submission_id"].as_str().unwrap_or("").to_string()
            };
            // Poll until the daemon serves the new payload (confirmed on chain).
            let want: serde_json::Value = serde_json::from_slice(&payload)?;
            let started = std::time::Instant::now();
            loop {
                if let Some(o) = daemon_get_object(&daemon, &repo, "/sys/policies/", "root").await? {
                    let got = o.get("payload").or_else(|| o.get("content")).or_else(|| o.get("value")).cloned();
                    let got = match got {
                        Some(serde_json::Value::String(s)) => serde_json::from_str::<serde_json::Value>(&s).ok().or_else(|| {
                            base64::engine::general_purpose::URL_SAFE_NO_PAD
                                .decode(&s)
                                .ok()
                                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                        }),
                        other => other,
                    };
                    if got.as_ref() == Some(&want) {
                        let block = o.get("block").cloned().unwrap_or(serde_json::Value::Null);
                        // The daemon serves an accepted write from its overlay
                        // before the DA batch lands; wait for the confirmed copy.
                        if o.get("confirmed").and_then(|c| c.as_bool()) == Some(false) {
                            println!("  … accepted (overlay, block {block}); waiting for on-chain confirmation");
                        } else {
                            println!("✓ /sys/policies/root now serves the new policy (confirmed, block {block}; submission {sub})");
                            break;
                        }
                    }
                }
                if started.elapsed() > std::time::Duration::from_secs(900) {
                    bail!("timed out waiting for the daemon to serve the new policy (submission {sub})");
                }
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
        }
        Commands::DnssecRefresh { daemon, repo, domains } => {
            let mut failed = false;
            for d in &domains {
                match capture_proof(&daemon, d, Some(&repo), false).await {
                    Ok(None) => println!("✓ /sys/dnssec/{d} is fresh"),
                    Ok(Some(proof)) => {
                        let throwaway = SigningKey::generate();
                        let wire_bytes = set_dnssec(&throwaway, d, &proof);
                        match turbo_submit(&wire_bytes).await {
                            Ok(id) => println!("✓ refreshed /sys/dnssec/{d} (submission {id})"),
                            Err(e) => { eprintln!("✗ {d}: submit failed: {e:#}"); failed = true; }
                        }
                    }
                    Err(e) => { eprintln!("✗ {d}: {e:#}"); failed = true; }
                }
            }
            if failed {
                bail!("some domains failed to refresh");
            }
        }
    }
    Ok(())
}
