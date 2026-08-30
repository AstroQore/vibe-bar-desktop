//! Cross-client storage contract and diagnostic-only lease probe.
//!
//! The native `docs/contracts/` directory is the source of truth. These
//! modules only verify and exercise the protocol; they do not authorize a
//! shared-store write.

mod lease;
#[cfg(any(test, feature = "contract-probe"))]
mod lease_unix;
mod manifest;
mod record;

pub use lease::{LeaseError, SharedStoreLeaseBatch};
pub use manifest::*;
pub use record::SharedStoreLeaseRecord;

#[cfg(test)]
mod tests;
