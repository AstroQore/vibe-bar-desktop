//! Synthetic-root interoperability probe for the native Swift test suite.
//!
//! Example (two processes):
//! `cargo run -p vibebar-desktop-core --example shared_store_lease_probe -- --root "/tmp/VibeBarLease-fixture" --store quota_cache --mode hold --milliseconds 5000`
//! `cargo run -p vibebar-desktop-core --example shared_store_lease_probe -- --root "/tmp/VibeBarLease-fixture" --store quota_cache --mode try`
//!
//! `--root` must already exist, carry the `VibeBarLease-` prefix, and live
//! below a fixed OS temporary anchor. This is intentionally incapable of
//! targeting a user's real Vibe Bar data root, even if `TMPDIR` is overridden.

use std::str::FromStr;
use std::time::Duration;

use vibebar_desktop_core::storage_contract::{
    ContractError, LeaseError, SharedStoreId, SharedStoreLeaseBatch, SharedStoreLeaseRole,
    SharedStoreManifest,
};

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("shared-store lease probe: {error}");
        std::process::exit(if matches!(error, LeaseError::Busy) {
            3
        } else {
            2
        });
    }
}

fn run(args: Vec<String>) -> Result<(), LeaseError> {
    let root = argument(&args, "--root").ok_or(LeaseError::InvalidRecord("--root is required"))?;
    let store = SharedStoreId::from_str(argument(&args, "--store").unwrap_or("quota_cache"))
        .map_err(LeaseError::Contract)?;
    let mode = argument(&args, "--mode").unwrap_or("try");
    let milliseconds = argument(&args, "--milliseconds")
        .unwrap_or("0")
        .parse::<u64>()
        .map_err(|_| LeaseError::InvalidRecord("--milliseconds must be an integer"))?;
    let maintenance = has_flag(&args, "--maintenance");
    let role = role_for(store, maintenance)?;
    let mut lease = SharedStoreLeaseBatch::acquire_synthetic_probe(
        std::path::Path::new(root),
        &[store],
        role,
        maintenance,
        "rust-contract-probe",
    )?;
    println!("acquired store={}", store.as_raw());
    if mode == "hold" {
        std::thread::sleep(Duration::from_millis(milliseconds.max(1)));
    } else if mode != "try" {
        return Err(LeaseError::InvalidRecord("--mode must be try or hold"));
    }
    lease.release();
    Ok(())
}

fn argument<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == name).then_some(pair[1].as_str()))
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|argument| argument == name)
}

fn role_for(store: SharedStoreId, maintenance: bool) -> Result<SharedStoreLeaseRole, LeaseError> {
    let manifest = SharedStoreManifest::native_fixture().map_err(LeaseError::Contract)?;
    let contract = manifest.contract(store).ok_or_else(|| {
        LeaseError::Contract(ContractError::UnknownStore(store.as_raw().to_owned()))
    })?;
    if maintenance {
        contract
            .writer_roles
            .iter()
            .copied()
            .find(|role| {
                matches!(
                    role,
                    SharedStoreLeaseRole::Migrator | SharedStoreLeaseRole::Pruner
                )
            })
            .ok_or(LeaseError::InvalidRole)
    } else {
        contract
            .writer_roles
            .first()
            .copied()
            .ok_or(LeaseError::InvalidRole)
    }
}

#[cfg(test)]
mod tests {
    use super::{has_flag, role_for};
    use vibebar_desktop_core::storage_contract::{SharedStoreId, SharedStoreLeaseRole};

    #[test]
    fn bare_maintenance_flag_is_detected() {
        let args = vec![
            "--root".into(),
            "/tmp/synthetic".into(),
            "--maintenance".into(),
        ];
        assert!(has_flag(&args, "--maintenance"));
    }

    #[test]
    fn role_is_selected_from_each_store_manifest() {
        assert_eq!(
            role_for(SharedStoreId::Settings, false).unwrap(),
            SharedStoreLeaseRole::SettingsEditor
        );
        assert_eq!(
            role_for(SharedStoreId::UsageEvents, false).unwrap(),
            SharedStoreLeaseRole::UsageScanner
        );
        assert_eq!(
            role_for(SharedStoreId::SessionIndexMaintenance, false).unwrap(),
            SharedStoreLeaseRole::Pruner
        );
        assert_eq!(
            role_for(SharedStoreId::QuotaCache, true).unwrap(),
            SharedStoreLeaseRole::Migrator
        );
        assert_eq!(
            role_for(SharedStoreId::SessionIndexScratch, true).unwrap(),
            SharedStoreLeaseRole::Pruner
        );
    }
}
