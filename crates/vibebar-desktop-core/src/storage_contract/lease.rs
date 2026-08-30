use std::fmt;
use std::path::Path;
#[cfg(any(test, feature = "contract-probe"))]
use std::path::PathBuf;

#[cfg(any(test, feature = "contract-probe"))]
use super::lease_unix::lease_platform;
#[cfg(any(test, feature = "contract-probe"))]
use super::record::valid_client_id;
use super::{ContractError, SharedStoreId, SharedStoreLeaseRole};
#[cfg(any(test, feature = "contract-probe"))]
use super::{SharedStoreLeaseRecord, SharedStoreManifest, SharedStoreShareEligibility};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseError {
    Busy,
    NotEligible(SharedStoreId),
    InvalidRole,
    InvalidClientId,
    InvalidRecord(&'static str),
    InvalidSyntheticRoot,
    SymlinkDetected,
    UnsupportedPlatform,
    Json(String),
    Contract(ContractError),
    Io { operation: &'static str, code: i32 },
}
impl fmt::Display for LeaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LeaseError {}

/// RAII lock batch for conformance tests. The record is diagnostic only; lock
/// possession never grants a product write authorization.
#[cfg(any(test, feature = "contract-probe"))]
pub struct SharedStoreLeaseBatch {
    inner: lease_platform::LeaseBatch,
}

/// Unconstructable production marker. `acquire_writer` always returns an
/// error, while the diagnostic lock implementation is omitted entirely.
#[cfg(not(any(test, feature = "contract-probe")))]
pub struct SharedStoreLeaseBatch {
    _private: (),
}

impl SharedStoreLeaseBatch {
    /// Product-facing API. Current contracts intentionally reject every store:
    /// `legacy_unsafe`, `not_eligible`, and `endpoint_only` are all non-writable.
    pub fn acquire_writer(
        _root: &Path,
        stores: &[SharedStoreId],
        _role: SharedStoreLeaseRole,
        _client_id: &str,
    ) -> Result<Self, LeaseError> {
        let store = *stores.first().ok_or(LeaseError::InvalidRole)?;
        Err(LeaseError::NotEligible(store))
    }

    /// Public diagnostic seam for cross-client tests. On Unix the root must
    /// already exist, have the `VibeBarLease-` scratch prefix, and canonicalize
    /// below a fixed OS temporary anchor. Environment variables such as
    /// `TMPDIR` are never authority for this boundary.
    #[cfg(any(test, feature = "contract-probe"))]
    pub fn acquire_synthetic_probe(
        root: &Path,
        stores: &[SharedStoreId],
        role: SharedStoreLeaseRole,
        maintenance: bool,
        client_id: &str,
    ) -> Result<Self, LeaseError> {
        Self::acquire_diagnostic_probe(
            &canonical_synthetic_child(root)?,
            stores,
            role,
            maintenance,
            client_id,
        )
    }

    /// Raw lock compatibility seam for crate-private unit tests. It is never
    /// exposed to callers and does not grant a shared-store write authority.
    #[cfg(any(test, feature = "contract-probe"))]
    pub(crate) fn acquire_diagnostic_probe(
        root: &Path,
        stores: &[SharedStoreId],
        role: SharedStoreLeaseRole,
        maintenance: bool,
        client_id: &str,
    ) -> Result<Self, LeaseError> {
        if stores.is_empty() {
            return Err(LeaseError::InvalidRole);
        }
        if !valid_client_id(client_id) {
            return Err(LeaseError::InvalidClientId);
        }
        let manifest = SharedStoreManifest::native_fixture().map_err(LeaseError::Contract)?;
        for store in stores {
            let contract = manifest.contract(*store).ok_or_else(|| {
                LeaseError::Contract(ContractError::UnknownStore(store.as_raw().to_owned()))
            })?;
            if contract.share_eligibility == SharedStoreShareEligibility::EndpointOnly {
                return Err(LeaseError::NotEligible(*store));
            }
            if maintenance {
                if !matches!(
                    role,
                    SharedStoreLeaseRole::Migrator | SharedStoreLeaseRole::Pruner
                ) || !contract.writer_roles.contains(&role)
                {
                    return Err(LeaseError::InvalidRole);
                }
            } else if !contract.writer_roles.contains(&role) {
                return Err(LeaseError::InvalidRole);
            }
        }
        let mut stores = stores.to_vec();
        stores.sort_by_key(|store| store.as_raw());
        stores.dedup();
        let record = SharedStoreLeaseRecord::new(
            role,
            std::process::id() as i32,
            unix_epoch_millis(),
            client_id,
        );
        Ok(Self {
            inner: lease_platform::LeaseBatch::acquire(root, &stores, maintenance, &record)?,
        })
    }

    #[cfg(any(test, feature = "contract-probe"))]
    pub fn release(&mut self) {
        self.inner.release();
    }
    #[cfg(not(any(test, feature = "contract-probe")))]
    pub fn release(&mut self) {}

    #[cfg(any(test, feature = "contract-probe"))]
    pub fn stores(&self) -> &[SharedStoreId] {
        self.inner.stores()
    }
    #[cfg(not(any(test, feature = "contract-probe")))]
    pub fn stores(&self) -> &[SharedStoreId] {
        &[]
    }
}

#[cfg(all(any(test, feature = "contract-probe"), unix))]
fn canonical_synthetic_child(root: &Path) -> Result<PathBuf, LeaseError> {
    let root = root
        .canonicalize()
        .map_err(|_| LeaseError::InvalidSyntheticRoot)?;
    if !is_trusted_synthetic_path(&root, &trusted_temp_roots()) {
        return Err(LeaseError::InvalidSyntheticRoot);
    }
    Ok(root)
}

#[cfg(all(any(test, feature = "contract-probe"), unix))]
pub(super) fn is_trusted_synthetic_path(root: &Path, trusted_roots: &[PathBuf]) -> bool {
    let has_probe_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("VibeBarLease-") && name.len() > 13);
    has_probe_name && trusted_roots.iter().any(|temp| root.starts_with(temp))
}

#[cfg(all(any(test, feature = "contract-probe"), unix))]
fn trusted_temp_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    const CANDIDATES: &[&str] = &["/private/tmp", "/private/var/folders"];
    #[cfg(not(target_os = "macos"))]
    const CANDIDATES: &[&str] = &["/tmp", "/var/tmp"];

    CANDIDATES
        .iter()
        .filter_map(|candidate| Path::new(candidate).canonicalize().ok())
        .collect()
}

#[cfg(all(any(test, feature = "contract-probe"), not(unix)))]
fn canonical_synthetic_child(_root: &Path) -> Result<PathBuf, LeaseError> {
    Err(LeaseError::UnsupportedPlatform)
}
impl Drop for SharedStoreLeaseBatch {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(any(test, feature = "contract-probe"))]
pub(super) fn unix_epoch_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
