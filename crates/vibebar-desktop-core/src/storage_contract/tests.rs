use super::*;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

#[test]
fn native_contract_fixture_decodes_and_is_hashed_exactly() {
    let manifest = SharedStoreManifest::native_fixture().unwrap();
    assert_eq!(manifest.protocol_version, 1);
    assert_eq!(SharedStoreId::ALL.len(), 27);
    assert_eq!(manifest.stores.len(), SharedStoreId::ALL.len());
    assert!(manifest.contract(SharedStoreId::QuotaCache).is_some());
}

#[test]
fn checksum_sidecars_accept_windows_crlf() {
    let hash = sha256_hex(b"fixture");
    let sidecar = format!("{hash}  fixture.json\r\n");
    verify_fixture_sha256("fixture.json", b"fixture", &sidecar).unwrap();
    assert!(verify_fixture_sha256(
        "fixture.json",
        b"fixture",
        &format!("{hash}  fixture.json\r\n\r\n")
    )
    .is_err());
}

#[test]
fn endpoint_and_legacy_contracts_are_classified_fail_closed() {
    let manifest = SharedStoreManifest::native_fixture().unwrap();
    assert_eq!(
        manifest
            .contract(SharedStoreId::CredentialVault)
            .unwrap()
            .locator_kind,
        SharedStoreLocatorKind::KeychainItem
    );
    assert_eq!(
        manifest
            .contract(SharedStoreId::McpSocket)
            .unwrap()
            .share_eligibility,
        SharedStoreShareEligibility::EndpointOnly
    );
    assert_eq!(
        manifest
            .contract(SharedStoreId::McpSocket)
            .unwrap()
            .endpoint_version
            .as_deref(),
        Some("2025-06-18")
    );
    assert_eq!(
        manifest
            .contract(SharedStoreId::McpSocket)
            .unwrap()
            .relative_locator,
        "mcp.sock"
    );
    let remote_usage = manifest.contract(SharedStoreId::RemoteUsage).unwrap();
    assert_eq!(
        remote_usage.schema_kind,
        SharedStoreSchemaKind::SqliteUnversioned
    );
    assert_eq!(remote_usage.current_schema_version, None);
    let maintenance = manifest
        .contract(SharedStoreId::SessionIndexMaintenance)
        .unwrap();
    assert_eq!(
        maintenance.schema_kind,
        SharedStoreSchemaKind::JsonUnversioned
    );
    assert_eq!(maintenance.current_schema_version, None);
    assert!(manifest
        .stores
        .iter()
        .filter(|store| store.locator_kind == SharedStoreLocatorKind::FilesystemRelative)
        .all(|store| store.share_eligibility == SharedStoreShareEligibility::LegacyUnsafe));
}

#[test]
fn lease_record_is_native_byte_equal_in_both_directions() {
    let record = SharedStoreLeaseRecord::native_fixture().unwrap();
    assert_eq!(record.canonical_json().unwrap(), LEASE_RECORD_FIXTURE);
}

#[test]
fn lease_record_escapes_slashes_like_swift_json_encoder() {
    let record = SharedStoreLeaseRecord::new(
        SharedStoreLeaseRole::QuotaCollector,
        42,
        1_700_000_000_000,
        "fixture/client",
    );
    let bytes = record.canonical_json().unwrap();
    assert_eq!(
        bytes,
        br#"{"clientID":"fixture\/client","pid":42,"role":"quota_collector","startedAt":1700000000000,"version":1}"#
    );
    assert_eq!(
        SharedStoreLeaseRecord::from_canonical_json(&bytes)
            .unwrap()
            .client_id,
        "fixture/client"
    );
}

#[test]
fn production_writer_rejects_every_current_store() {
    for store in SharedStoreManifest::native_fixture().unwrap().stores {
        let error = SharedStoreLeaseBatch::acquire_writer(
            Path::new("/synthetic"),
            &[store.store_id],
            SharedStoreLeaseRole::Migrator,
            "desktop",
        )
        .err()
        .expect("production writer must reject every current store");
        assert_eq!(error, LeaseError::NotEligible(store.store_id));
    }
}

#[test]
fn malformed_manifest_is_rejected() {
    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest.stores.push(manifest.stores[0].clone());
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::DuplicateStore(_))
    ));
    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest.protocol_version += 1;
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::ProtocolVersion(_))
    ));
    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest.stores.pop();
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::MissingStore(_))
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::CredentialVault)
        .unwrap()
        .endpoint_protocol = Some("mcp-jsonrpc".to_string());
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::McpSocket)
        .unwrap()
        .keychain_service = Some("synthetic".to_string());
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::UsageEvents)
        .unwrap()
        .current_schema_version = None;
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::Settings)
        .unwrap()
        .current_schema_version = Some(1);
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    let settings = manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::Settings)
        .unwrap();
    settings.schema_kind = SharedStoreSchemaKind::UnixSocket;
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::CredentialVault)
        .unwrap()
        .schema_kind = SharedStoreSchemaKind::JsonSchemaVersion;
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));

    let mut manifest = SharedStoreManifest::native_fixture().unwrap();
    manifest
        .stores
        .iter_mut()
        .find(|store| store.store_id == SharedStoreId::McpSocket)
        .unwrap()
        .schema_kind = SharedStoreSchemaKind::JsonUnversioned;
    assert!(matches!(
        manifest.validate(),
        Err(ContractError::InvalidStore { .. })
    ));
}

#[cfg(unix)]
fn temp_root() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("VibeBarLease-")
        .tempdir()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_same_store_busy_and_release_allows_takeover() {
    let root = temp_root();
    let mut first = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "test",
    )
    .unwrap();
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            root.path(),
            &[SharedStoreId::QuotaCache],
            SharedStoreLeaseRole::QuotaCollector,
            false,
            "test"
        )
        .err(),
        Some(LeaseError::Busy)
    );
    first.release();
    SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "test",
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn public_synthetic_probe_allows_only_a_real_temp_child() {
    let root = temp_root();
    let mut lease = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "test",
    )
    .unwrap();
    lease.release();

    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            &std::env::temp_dir(),
            &[SharedStoreId::QuotaCache],
            SharedStoreLeaseRole::QuotaCollector,
            false,
            "test",
        )
        .err(),
        Some(LeaseError::InvalidSyntheticRoot)
    );

    let non_temp = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = non_temp.join("run");
    assert!(
        !run.exists(),
        "test fixture must not contain a run directory"
    );
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            non_temp,
            &[SharedStoreId::QuotaCache],
            SharedStoreLeaseRole::QuotaCollector,
            false,
            "test",
        )
        .err(),
        Some(LeaseError::InvalidSyntheticRoot)
    );
    assert!(
        !run.exists(),
        "rejected non-temp root must remain untouched"
    );

    // A caller-controlled TMPDIR must not make a home path trusted. The
    // decision helper receives only fixed anchors; even the required basename
    // prefix is insufficient outside them.
    assert!(!super::lease::is_trusted_synthetic_path(
        Path::new("/Users/example/VibeBarLease-env-override"),
        &[PathBuf::from("/private/tmp")]
    ));
}

#[cfg(unix)]
#[test]
fn public_synthetic_probe_rejects_temp_link_to_outside() {
    use std::os::unix::fs::symlink;

    let root = temp_root();
    let link = root.path().join("outside");
    let outside = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = outside.join("run");
    assert!(
        !run.exists(),
        "test fixture must not contain a run directory"
    );
    symlink(outside, &link).unwrap();
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            &link,
            &[SharedStoreId::QuotaCache],
            SharedStoreLeaseRole::QuotaCollector,
            false,
            "test",
        )
        .err(),
        Some(LeaseError::InvalidSyntheticRoot)
    );
    assert!(!run.exists(), "outside link target must remain untouched");
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_different_stores_parallel_and_maintenance_fences() {
    let root = temp_root();
    let mut quota = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "test",
    )
    .unwrap();
    let mut status = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::ServiceStatus],
        SharedStoreLeaseRole::StatusCollector,
        false,
        "test",
    )
    .unwrap();
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            root.path(),
            &[SharedStoreId::QuotaCache],
            SharedStoreLeaseRole::Migrator,
            true,
            "test"
        )
        .err(),
        Some(LeaseError::Busy)
    );
    quota.release();
    status.release();
    let mut maintenance = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::Migrator,
        true,
        "test",
    )
    .unwrap();
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            root.path(),
            &[SharedStoreId::ServiceStatus],
            SharedStoreLeaseRole::StatusCollector,
            false,
            "test"
        )
        .err(),
        Some(LeaseError::Busy)
    );
    maintenance.release();
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_maintenance_role_must_belong_to_the_store() {
    let root = temp_root();
    assert_eq!(
        SharedStoreLeaseBatch::acquire_synthetic_probe(
            root.path(),
            &[SharedStoreId::SessionIndexScratch],
            SharedStoreLeaseRole::Migrator,
            true,
            "test",
        )
        .err(),
        Some(LeaseError::InvalidRole)
    );
    SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::SessionIndexScratch],
        SharedStoreLeaseRole::Pruner,
        true,
        "test",
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_writes_exact_record_and_cleans_it_up() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp_root();
    let mut lease = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "fixture-client",
    )
    .unwrap();
    let record = std::fs::read(root.path().join("run/quota_cache.record")).unwrap();
    assert_eq!(
        SharedStoreLeaseRecord::from_canonical_json(&record)
            .unwrap()
            .role,
        SharedStoreLeaseRole::QuotaCollector
    );
    assert_eq!(
        std::fs::metadata(root.path().join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(root.path().join("run/quota_cache.lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(root.path().join("run/quota_cache.record"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    lease.release();
    assert!(!root.path().join("run/quota_cache.record").exists());
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_rejects_final_and_intermediate_root_symlinks() {
    use std::os::unix::fs::symlink;

    let root = temp_root();
    let root_path = root.path().canonicalize().unwrap();
    let target = root_path.join("target");
    let child = target.join("child");
    std::fs::create_dir_all(&child).unwrap();
    let final_link = root_path.join("final-link");
    symlink(&target, &final_link).unwrap();
    let intermediate_link = root_path.join("intermediate-link");
    symlink(&target, &intermediate_link).unwrap();

    for candidate in [final_link, intermediate_link.join("child")] {
        assert_eq!(
            SharedStoreLeaseBatch::acquire_diagnostic_probe(
                &candidate,
                &[SharedStoreId::QuotaCache],
                SharedStoreLeaseRole::QuotaCollector,
                false,
                "test"
            )
            .err(),
            Some(LeaseError::SymlinkDetected)
        );
    }
}

#[cfg(unix)]
#[test]
fn diagnostic_probe_rejects_hard_linked_lock_without_mutating_target() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root();
    let run = root.path().join("run");
    std::fs::create_dir(&run).unwrap();
    let external = root.path().join("external.lock");
    let original = b"external lock content";
    std::fs::write(&external, original).unwrap();
    std::fs::set_permissions(&external, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::hard_link(&external, run.join("quota_cache.lock")).unwrap();

    let result = SharedStoreLeaseBatch::acquire_synthetic_probe(
        root.path(),
        &[SharedStoreId::QuotaCache],
        SharedStoreLeaseRole::QuotaCollector,
        false,
        "hard-link-test",
    );
    let error = match result {
        Ok(_) => panic!("hard-linked lock must be rejected"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        LeaseError::Io {
            operation: "reject_hard_link_lock",
            code: libc::EMLINK,
        }
    );
    assert_eq!(std::fs::read(&external).unwrap(), original);
    assert_eq!(
        std::fs::metadata(&external).unwrap().permissions().mode() & 0o777,
        0o644
    );
}
