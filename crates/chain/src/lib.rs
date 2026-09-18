//! What every browserid-pay service needs to act on the SBO database as its
//! own delegated identity: provisioning (one human approval at the broker),
//! device-model envelope assembly, reads and submits through an sbo-daemon
//! (which holds the repo's TurboDA credentials and refreshes issuer DNSSEC
//! evidence at write time), and the provider-listing convention.

pub mod daemon;
pub mod identity;
pub mod listing;
pub mod provision;

pub use daemon::{ChainReader, ChainWriter, DaemonReader, DaemonWriter, LogWriter, ObjectView};
pub use identity::{assemble_device_wire, Identity, ProvisionSpec, SetupState, StoredGrant, StoredIdentity, WriteSpec};
pub use listing::{ensure_listing, listing_path, LISTING_ID};
pub use provision::{Pending, Provisioned};

/// `+` in a query string decodes to a space; the repo selector carries one.
pub fn encode_repo(repo: &str) -> String {
    repo.replace('+', "%2B")
}

/// Avail's nominal block time in seconds. ONE definition, shared by every
/// service, because a window is only enforceable if both sides derive the same
/// heights from the same terms (browserid-pay-b3gq).
///
/// Nominal, not guaranteed: Substrate skips SLOTS rather than block numbers,
/// and blocks cannot be produced faster than the slot rate, so real elapsed
/// time per block is >= this and never less. Converting a count of blocks into
/// seconds therefore UNDERSTATES the real duration — anything that must not be
/// understated (a cap on how long money may be committed) has to add margin.
/// Mirrors `NOMINAL_BLOCK_SECS` in the JS SDK.
pub const NOMINAL_BLOCK_SECS: i64 = 20;

/// A count of blocks as an estimated number of seconds, rounded up.
pub fn blocks_to_secs(blocks: i64) -> i64 {
    blocks.saturating_mul(NOMINAL_BLOCK_SECS)
}
