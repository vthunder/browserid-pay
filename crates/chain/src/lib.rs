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
