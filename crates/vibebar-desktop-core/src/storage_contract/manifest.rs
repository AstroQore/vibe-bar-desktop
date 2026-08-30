//! Cross-client shared-store contract and **diagnostic-only** lease probe.
//!
//! The JSON fixtures in `docs/contracts/` are exported by the native Swift
//! implementation and are the sole source of truth.  Keeping the Rust types
//! here deliberately does not authorize a shared write: every current store
//! is legacy-unsafe and [`SharedStoreLeaseBatch::acquire_writer`] refuses it.

use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SHARED_STORE_PROTOCOL_VERSION: u32 = 1;
pub const CONTRACT_FIXTURE: &[u8] =
    include_bytes!("../../../../docs/contracts/shared-store-contract-v1.json");
pub const CONTRACT_FIXTURE_SHA256: &str =
    include_str!("../../../../docs/contracts/shared-store-contract-v1.json.sha256");
pub const LEASE_RECORD_FIXTURE: &[u8] =
    include_bytes!("../../../../docs/contracts/shared-store-lease-record-v1.json");
pub const LEASE_RECORD_FIXTURE_SHA256: &str =
    include_str!("../../../../docs/contracts/shared-store-lease-record-v1.json.sha256");

macro_rules! protocol_enum {
    ($name:ident { $($variant:ident = $raw:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $raw)] $variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn as_raw(self) -> &'static str { match self { $(Self::$variant => $raw),+ } }
        }
        impl fmt::Display for $name { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_raw()) } }
    };
}

protocol_enum!(SharedStoreDurability { Durable = "durable", Reconstructible = "reconstructible", Ephemeral = "ephemeral" });
protocol_enum!(SharedStoreSchemaKind {
    JsonUnversioned = "json_unversioned", JsonSchemaVersion = "json_schema_version",
    SqliteUserVersion = "sqlite_user_version", SqliteMetadataVersion = "sqlite_metadata_version",
    SqliteUnversioned = "sqlite_unversioned",
    KeychainEnvelope = "keychain_envelope", UnixSocket = "unix_socket", Directory = "directory"
});
protocol_enum!(SharedStoreLocatorKind { FilesystemRelative = "filesystem_relative", KeychainItem = "keychain_item", Endpoint = "endpoint" });
protocol_enum!(SharedStoreImplementationStatus {
    LegacyUnsafe = "legacy_unsafe", NativeOnlyCredentialEndpoint = "native_only_credential_endpoint",
    EndpointOwned = "endpoint_owned"
});
protocol_enum!(SharedStoreShareEligibility { NotEligible = "not_eligible", LegacyUnsafe = "legacy_unsafe", EndpointOnly = "endpoint_only" });
protocol_enum!(SharedStoreUnknownVersionPolicy { FailClosed = "fail_closed" });
protocol_enum!(SharedStoreRecoveryPolicy {
    RequireExplicitMigration = "require_explicit_migration", RebuildFromAuthoritativeSource = "rebuild_from_authoritative_source",
    RecreateEphemeralOwnerState = "recreate_ephemeral_owner_state"
});
protocol_enum!(SharedStoreFlushPolicy { Immediate = "immediate", FlushOnShutdown = "flush_on_shutdown", CheckpointWalOnShutdown = "checkpoint_wal_on_shutdown", RemoveOnOwnerShutdown = "remove_on_owner_shutdown" });
protocol_enum!(SharedStoreLeaseRole {
    SettingsEditor = "settings_editor", QuotaCollector = "quota_collector", StatusCollector = "status_collector",
    UsageScanner = "usage_scanner", PricingRefresher = "pricing_refresher", SessionIndexer = "session_indexer",
    LayoutEditor = "layout_editor", MiniWindowManager = "mini_window_manager", CredentialManager = "credential_manager",
    Migrator = "migrator", Pruner = "pruner", SkillsManager = "skills_manager", RemoteSync = "remote_sync", McpOwner = "mcp_owner"
});
protocol_enum!(SharedStoreId {
    Settings = "settings", QuotaCache = "quota_cache", QuotaFieldRegistry = "quota_field_registry", ServiceStatus = "service_status",
    ScanCache = "scan_cache", CostSnapshots = "cost_snapshots", CostHistory = "cost_history", SubscriptionHistory = "subscription_history",
    FillTimeline = "fill_timeline", ForecastTimeline = "forecast_timeline", UsageEvents = "usage_events", SessionIndex = "session_index",
    SessionIndexMaintenance = "session_index_maintenance", SessionIndexScratch = "session_index_scratch", PageLayout = "page_layout",
    MiniWindowGeometry = "mini_window_geometry", AntigravityModelLabels = "antigravity_model_labels", GeminiWebUsageRecipe = "gemini_web_usage_recipe",
    PricingCache = "pricing_cache", PricingSources = "pricing_sources", PricingRefreshStatus = "pricing_refresh_status",
    SkillsRegistry = "skills_registry", SkillBackups = "skill_backups", RemoteCoreConfig = "remote_core_config", RemoteUsage = "remote_usage",
    CredentialVault = "credential_vault", McpSocket = "mcp_socket"
});

impl FromStr for SharedStoreId {
    type Err = ContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(&format!("\"{value}\""))
            .map_err(|_| ContractError::UnknownStore(value.to_owned()))
    }
}

/// Exact wire shape exported by Swift's `SharedStoreContractRegistry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedStoreContract {
    #[serde(rename = "storeID")]
    pub store_id: SharedStoreId,
    pub locator_kind: SharedStoreLocatorKind,
    pub relative_locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keychain_service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keychain_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_encoding: Option<String>,
    pub sidecars: Vec<String>,
    pub durability: SharedStoreDurability,
    pub schema_kind: SharedStoreSchemaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_schema_version: Option<u32>,
    pub writer_roles: Vec<SharedStoreLeaseRole>,
    pub unknown_version_policy: SharedStoreUnknownVersionPolicy,
    pub recovery_policy: SharedStoreRecoveryPolicy,
    pub flush_policy: SharedStoreFlushPolicy,
    pub share_eligibility: SharedStoreShareEligibility,
    pub implementation_status: SharedStoreImplementationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedStoreManifest {
    pub protocol_version: u32,
    pub stores: Vec<SharedStoreContract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    Json(String),
    ProtocolVersion(u32),
    DuplicateStore(SharedStoreId),
    MissingStore(SharedStoreId),
    UnknownStore(String),
    InvalidStore {
        store: SharedStoreId,
        reason: &'static str,
    },
    FixtureHash {
        fixture: &'static str,
        expected: String,
        actual: String,
    },
    FixtureChecksumFormat(&'static str),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid shared-store fixture JSON: {error}"),
            Self::ProtocolVersion(version) => {
                write!(f, "unsupported shared-store protocol version {version}")
            }
            Self::DuplicateStore(store) => write!(f, "duplicate shared-store contract {store}"),
            Self::MissingStore(store) => write!(f, "missing shared-store contract {store}"),
            Self::UnknownStore(store) => write!(f, "unknown shared-store id {store}"),
            Self::InvalidStore { store, reason } => {
                write!(f, "invalid contract for {store}: {reason}")
            }
            Self::FixtureHash {
                fixture,
                expected,
                actual,
            } => write!(
                f,
                "{fixture} SHA-256 mismatch: expected {expected}, got {actual}"
            ),
            Self::FixtureChecksumFormat(fixture) => {
                write!(f, "invalid checksum sidecar for {fixture}")
            }
        }
    }
}
impl std::error::Error for ContractError {}

impl SharedStoreManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ContractError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| ContractError::Json(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn native_fixture() -> Result<Self, ContractError> {
        verify_fixture_sha256(
            "shared-store-contract-v1.json",
            CONTRACT_FIXTURE,
            CONTRACT_FIXTURE_SHA256,
        )?;
        Self::from_json(CONTRACT_FIXTURE)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.protocol_version != SHARED_STORE_PROTOCOL_VERSION {
            return Err(ContractError::ProtocolVersion(self.protocol_version));
        }
        let mut ids = HashSet::new();
        for store in &self.stores {
            if !ids.insert(store.store_id) {
                return Err(ContractError::DuplicateStore(store.store_id));
            }
            validate_store(store)?;
        }
        for expected in SharedStoreId::ALL {
            if !ids.contains(expected) {
                return Err(ContractError::MissingStore(*expected));
            }
        }
        Ok(())
    }

    pub fn contract(&self, store: SharedStoreId) -> Option<&SharedStoreContract> {
        self.stores
            .iter()
            .find(|candidate| candidate.store_id == store)
    }
}

fn invalid(store: SharedStoreId, reason: &'static str) -> ContractError {
    ContractError::InvalidStore { store, reason }
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_store(store: &SharedStoreContract) -> Result<(), ContractError> {
    if store.writer_roles.is_empty() {
        return Err(invalid(store.store_id, "writerRoles must not be empty"));
    }
    if store.unknown_version_policy != SharedStoreUnknownVersionPolicy::FailClosed {
        return Err(invalid(
            store.store_id,
            "unknownVersionPolicy must fail closed",
        ));
    }
    if store
        .sidecars
        .iter()
        .any(|name| !name.starts_with('-') || name.contains('/') || name.contains('\\'))
    {
        return Err(invalid(store.store_id, "invalid sidecar name"));
    }
    let schema_requires_version = matches!(
        store.schema_kind,
        SharedStoreSchemaKind::JsonSchemaVersion
            | SharedStoreSchemaKind::SqliteUserVersion
            | SharedStoreSchemaKind::SqliteMetadataVersion
            | SharedStoreSchemaKind::KeychainEnvelope
    );
    if schema_requires_version {
        if store
            .current_schema_version
            .is_none_or(|version| version == 0)
        {
            return Err(invalid(
                store.store_id,
                "versioned schema requires a positive currentSchemaVersion",
            ));
        }
    } else if store.current_schema_version.is_some() {
        return Err(invalid(
            store.store_id,
            "unversioned schema must not declare currentSchemaVersion",
        ));
    }
    match store.locator_kind {
        SharedStoreLocatorKind::FilesystemRelative => {
            if !safe_relative(&store.relative_locator) {
                return Err(invalid(
                    store.store_id,
                    "filesystem locator must be a safe relative path",
                ));
            }
            if matches!(
                store.schema_kind,
                SharedStoreSchemaKind::KeychainEnvelope | SharedStoreSchemaKind::UnixSocket
            ) {
                return Err(invalid(
                    store.store_id,
                    "filesystem locator has an incompatible schema kind",
                ));
            }
            if store.keychain_service.is_some()
                || store.keychain_account.is_some()
                || store.endpoint_protocol.is_some()
                || store.endpoint_version.is_some()
            {
                return Err(invalid(
                    store.store_id,
                    "filesystem locator has endpoint metadata",
                ));
            }
            if !matches!(
                store.share_eligibility,
                SharedStoreShareEligibility::LegacyUnsafe
                    | SharedStoreShareEligibility::NotEligible
            ) {
                return Err(invalid(
                    store.store_id,
                    "filesystem store is not currently share eligible",
                ));
            }
            if store.implementation_status != SharedStoreImplementationStatus::LegacyUnsafe {
                return Err(invalid(
                    store.store_id,
                    "filesystem store has unexpected implementation status",
                ));
            }
        }
        SharedStoreLocatorKind::KeychainItem => {
            if store.schema_kind != SharedStoreSchemaKind::KeychainEnvelope
                || !store.relative_locator.is_empty()
                || store
                    .keychain_service
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .is_none()
                || store.endpoint_protocol.is_some()
                || store.endpoint_version.is_some()
                || store
                    .keychain_account
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .is_none()
            {
                return Err(invalid(store.store_id, "keychain locator is incomplete"));
            }
            if store.share_eligibility != SharedStoreShareEligibility::EndpointOnly
                || store.implementation_status
                    != SharedStoreImplementationStatus::NativeOnlyCredentialEndpoint
            {
                return Err(invalid(
                    store.store_id,
                    "keychain locator must be endpoint-only/native-owned",
                ));
            }
        }
        SharedStoreLocatorKind::Endpoint => {
            if store.schema_kind != SharedStoreSchemaKind::UnixSocket
                || !safe_relative(&store.relative_locator)
                || store
                    .endpoint_protocol
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .is_none()
                || store.keychain_service.is_some()
                || store.keychain_account.is_some()
                || store
                    .endpoint_version
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .is_none()
            {
                return Err(invalid(store.store_id, "endpoint locator is incomplete"));
            }
            if store.share_eligibility != SharedStoreShareEligibility::EndpointOnly
                || store.implementation_status != SharedStoreImplementationStatus::EndpointOwned
            {
                return Err(invalid(
                    store.store_id,
                    "endpoint locator must be endpoint-only/owned",
                ));
            }
        }
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_fixture_sha256(
    filename: &'static str,
    bytes: &[u8],
    sidecar: &str,
) -> Result<(), ContractError> {
    // `include_str!` sees the checkout's line endings. Accept one canonical
    // checksum line under both LF and CRLF so Windows does not reject the
    // same fixture solely because Git materialized `\r\n`.
    let line = sidecar
        .strip_suffix("\r\n")
        .or_else(|| sidecar.strip_suffix('\n'))
        .unwrap_or(sidecar);
    let Some((expected, name)) = line.split_once("  ") else {
        return Err(ContractError::FixtureChecksumFormat(filename));
    };
    if name != filename
        || expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || line.matches("  ").count() != 1
    {
        return Err(ContractError::FixtureChecksumFormat(filename));
    }
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(ContractError::FixtureHash {
            fixture: filename,
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}
