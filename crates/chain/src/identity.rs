//! A service's own identity: a browserid agent device credential provisioned
//! once (a human approves at the broker; the service becomes its own delegated
//! identity — grantee `*`, a requested handle — see `provision.rs`), plus the
//! warrants that approval bundled (grantor = the approver, grantee = the
//! service): one for the SBO database (chain writes) and any others the
//! service needs. Chain writes are signed with the ACCESS key the access cert
//! certifies and carry the four-object presentation as `Auth-Cert`, exactly
//! like mingo's `sign_as_logged_in_user`. Without `as:<grantor>` in the chain
//! warrant the service authors its writes as itself.

use anyhow::{anyhow, Context, Result};
use browserid_agent::{DeviceAgent, DeviceCredential, GrantRequest};
use sbo_core::crypto::{ContentHash, Signature, SigningKey};
use sbo_core::message::{Action, Id, Message, ObjectType, Path};
use sbo_core::wire;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredGrant {
    pub audience: String,
    pub warrant: String,
    pub config_cert: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredIdentity {
    pub credential: DeviceCredential,
    pub grants: Vec<StoredGrant>,
}

impl StoredIdentity {
    pub fn load(path: &PathBuf) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&std::fs::read_to_string(path)?)?))
    }
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
    pub fn agent(&self) -> Result<DeviceAgent> {
        let mut agent = DeviceAgent::new(self.credential.clone())?;
        for g in &self.grants {
            agent.add_grant(&g.warrant, &g.config_cert)?;
        }
        Ok(agent)
    }
}

/// Provisioning state, visible at `GET /api/setup` while a human approval is pending.
#[derive(Clone, Serialize, Default)]
pub struct SetupState {
    pub identity: Option<String>,
    pub audiences: Vec<String>,
    pub approval_url: Option<String>,
    pub user_code: Option<String>,
    pub fingerprint: Option<String>,
    pub error: Option<String>,
}

pub struct Identity {
    path: PathBuf,
    pub state: Arc<Mutex<SetupState>>,
    agent: Arc<Mutex<Option<DeviceAgent>>>,
}

impl Identity {
    pub fn new(path: PathBuf) -> Self {
        Self { path, state: Arc::new(Mutex::new(SetupState::default())), agent: Arc::new(Mutex::new(None)) }
    }

    /// Load the stored identity if present.
    pub async fn load(&self) -> Result<bool> {
        let Some(stored) = StoredIdentity::load(&self.path)? else { return Ok(false) };
        let agent = stored.agent()?;
        let mut st = self.state.lock().await;
        st.identity = Some(agent.email().to_string());
        st.audiences = agent.warranted_audiences().iter().map(|s| s.to_string()).collect();
        drop(st);
        *self.agent.lock().await = Some(agent);
        Ok(true)
    }

    /// Does the loaded credential hold a warrant for `audience`?
    ///
    /// A stored credential loads happily even when it was issued for a
    /// DIFFERENT chain, and then every write fails on an audience miss — which
    /// looks like "no grant" rather than "wrong ledger". That is not
    /// hypothetical: moving from app 530 to 506 (browserid-pay-4v59) changes
    /// this string, so a service redeployed with the old credential would come
    /// up healthy and silently write nothing. Startup checks this and
    /// re-provisions instead.
    pub async fn has_audience(&self, audience: &str) -> bool {
        self.state.lock().await.audiences.iter().any(|a| a == audience)
    }

    pub async fn email(&self) -> Option<String> {
        self.state.lock().await.identity.clone()
    }

    /// Start provisioning at `broker`: a merged one-approval flow covering the
    /// identity and the bundled grants. The approval URL is logged and exposed
    /// in `state` (serve it at `/api/setup`); this waits (in the caller's
    /// task) for the pickup.
    pub async fn provision(&self, broker: &str, spec: &ProvisionSpec) -> Result<()> {
        let pending = crate::provision::request(broker, &spec.handle, &spec.grants, &spec.label, &spec.message)
            .await
            .map_err(|e| anyhow!("provisioning request at {broker}: {e}"))?;
        {
            let mut st = self.state.lock().await;
            st.approval_url = Some(pending.verification_uri_complete.clone());
            st.user_code = Some(pending.user_code.clone());
            st.fingerprint = Some(pending.fingerprint.clone());
        }
        tracing::warn!(
            "IDENTITY NOT PROVISIONED — open {}  (code {}, fingerprint {})",
            pending.verification_uri_complete,
            pending.user_code,
            pending.fingerprint
        );
        let provisioned = pending.wait().await.map_err(|e| anyhow!("provisioning: {e}"))?;
        if let Some(why) = &provisioned.grants_denied {
            tracing::warn!("identity approved but grants denied: {why}");
        }
        let grants = provisioned
            .grants
            .iter()
            .filter_map(|(aud, tail)| {
                let (w, c) = tail.split_once('~')?;
                Some(StoredGrant { audience: aud.clone(), warrant: w.into(), config_cert: c.into() })
            })
            .collect();
        let stored = StoredIdentity { credential: provisioned.credential, grants };
        stored.save(&self.path)?;
        let agent = stored.agent()?;
        {
            let mut st = self.state.lock().await;
            st.identity = Some(agent.email().to_string());
            st.audiences = agent.warranted_audiences().iter().map(|s| s.to_string()).collect();
            st.approval_url = None;
            st.user_code = None;
        }
        *self.agent.lock().await = Some(agent);
        Ok(())
    }

    /// A presentation for `audience` (a custodian origin), for delivering a ruling.
    pub async fn presentation_for(&self, audience: &str) -> Result<String> {
        let mut g = self.agent.lock().await;
        let agent = g.as_mut().context("identity not provisioned")?;
        agent.assertion_for(audience).await.map_err(|e| anyhow!("presentation for {audience}: {e}"))
    }

    /// Sign one SBO write as this identity for `chain_audience` (owner = self).
    pub async fn sign_write(&self, chain_audience: &str, spec: &WriteSpec) -> Result<Vec<u8>> {
        let mut g = self.agent.lock().await;
        let agent = g.as_mut().context("identity not provisioned")?;
        let owner = agent.email().to_string();
        let (presentation, seed) = agent
            .assertion_with_access_seed(chain_audience)
            .await
            .map_err(|e| anyhow!("minting access cert for {chain_audience}: {e}"))?;
        assemble_device_wire(&seed, &presentation, spec, &owner)
    }
}

/// What a service asks the approver for: its handle, the human-facing label
/// and message on the approval card, and the warrants to bundle.
pub struct ProvisionSpec {
    pub handle: String,
    pub label: String,
    pub message: String,
    pub grants: Vec<GrantRequest>,
}

pub struct WriteSpec {
    pub path: String,
    pub id: String,
    pub schema: String,
    pub payload: Vec<u8>,
}

/// Pure envelope assembly: sign with the ACCESS key and attach the
/// presentation as `Auth-Cert` (the daemon's device-attribution path requires
/// the envelope signer to equal the access cert's key).
pub fn assemble_device_wire(access_seed: &[u8; 32], presentation: &str, spec: &WriteSpec, owner: &str) -> Result<Vec<u8>> {
    let key = SigningKey::from_bytes(access_seed);
    let mut msg = Message {
        action: Action::Post,
        path: Path::parse(&spec.path).map_err(|e| anyhow!("bad path '{}': {e}", spec.path))?,
        id: Id::new(&spec.id).map_err(|e| anyhow!("bad id '{}': {e}", spec.id))?,
        object_type: ObjectType::Object,
        signing_key: key.public_key(),
        signature: Signature([0u8; 64]),
        content_type: Some("application/json".into()),
        content_hash: Some(ContentHash::sha256(&spec.payload)),
        payload: Some(spec.payload.clone()),
        owner: Some(Id::new(owner).map_err(|e| anyhow!("bad owner '{owner}': {e}"))?),
        creator: None,
        content_encoding: None,
        content_schema: Some(spec.schema.clone()),
        policy_ref: None,
        related: None,
        hlc: None,
        prev: None,
        auth_cert: Some(presentation.to_string()),
        auth_evidence: None,
        auth_warrant: None,
    };
    msg.sign(&key);
    Ok(wire::serialize(&msg))
}
