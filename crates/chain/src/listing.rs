//! Provider listings: `/providers/<kind>/<identity>/listing` (provider.v1).
//! The identity is a PATH segment because root-policy grant patterns match
//! an object's path, never its id (`/providers/<kind>/$user/**`).

use crate::daemon::{ChainReader, ChainWriter};
use crate::identity::{Identity, WriteSpec};
use anyhow::Result;
use serde_json::Value;

pub const LISTING_ID: &str = "listing";

pub fn listing_path(kind: &str, identity: &str) -> String {
    format!("/providers/{kind}/{identity}/")
}

/// Ensure this identity's listing under `kind` (`custodians` | `adjudicators`)
/// is on chain and equals `want`. No-op until the identity is provisioned.
pub async fn ensure_listing(
    identity: &Identity,
    reader: &dyn ChainReader,
    writer: &dyn ChainWriter,
    chain_audience: &str,
    kind: &str,
    want: &Value,
) -> Result<()> {
    let Some(me) = identity.email().await else { return Ok(()) };
    let path = listing_path(kind, &me);
    if let Some(existing) = reader.get_object(&path, LISTING_ID).await? {
        if &existing.value == want && existing.creator == me {
            tracing::info!("provider listing current");
            return Ok(());
        }
    }
    let wire = identity
        .sign_write(
            chain_audience,
            &WriteSpec { path, id: LISTING_ID.into(), schema: "provider.v1".into(), payload: serde_json::to_vec(want)? },
        )
        .await?;
    let sub = writer.submit(wire).await?;
    tracing::info!(%sub, "posted provider listing for {me}");
    Ok(())
}
