//! Product-disabled v1 document and three-way patch engine for `settings.json`.
//!
//! This module is deliberately pure: it parses bytes and produces a prospective
//! next document, but never opens a lease, writes a file, or selects a product
//! write route.  `settings.json` remains `legacy_unsafe` in the shared-store
//! manifest until the native writer ships the coordinated contract.

use std::collections::BTreeSet;

use serde::de::{Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::storage_contract::verify_fixture_sha256;

/// Native's shared relative path. This is a contract locator, not permission
/// for the current Desktop product to write there.
pub const SETTINGS_RELATIVE_PATH: &str = "settings.json";
pub const SETTINGS_SCHEMA_VERSION: u64 = 1;
pub const MAX_SETTINGS_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

pub const SETTINGS_V0_LEGACY_FIXTURE: &[u8] =
    include_bytes!("../../../../docs/contracts/settings-document-v0-legacy.json");
pub const SETTINGS_V0_LEGACY_FIXTURE_SHA256: &str =
    include_str!("../../../../docs/contracts/settings-document-v0-legacy.json.sha256");
pub const SETTINGS_V1_UNKNOWN_FIXTURE: &[u8] =
    include_bytes!("../../../../docs/contracts/settings-document-v1-unknown.json");
pub const SETTINGS_V1_UNKNOWN_FIXTURE_SHA256: &str =
    include_str!("../../../../docs/contracts/settings-document-v1-unknown.json.sha256");
pub const SETTINGS_VECTORS_FIXTURE: &[u8] =
    include_bytes!("../../../../docs/contracts/settings-document-vectors.json");
pub const SETTINGS_VECTORS_FIXTURE_SHA256: &str =
    include_str!("../../../../docs/contracts/settings-document-vectors.json.sha256");

const SCHEMA_VERSION_KEY: &str = "schemaVersion";
const REVISION_KEY: &str = "revision";
const RESERVED_KEYS: [&str; 2] = [SCHEMA_VERSION_KEY, REVISION_KEY];

/// The only settings Desktop's eventual first writer slice may change. Unknown
/// top-level and nested values remain raw JSON and are never reconstructed.
pub const DESKTOP_V1_WRITABLE_KEYS: &[&str] = &[
    "displayMode",
    "refreshIntervalSeconds",
    "refreshOnPopoverOpen",
    "popoverOpenRefreshCooldownSeconds",
    "menuBarTextEnabled",
    "menuBarColorBasis",
    "menuBarItems",
    "visibleCoreProviders",
    "coreProviderOrder",
    "providerPlanLabels",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsDocumentVersion {
    /// Existing native documents without an envelope are treated as revision 0.
    LegacyV0,
    V1,
}

/// A validated settings object. `fields` holds every non-envelope value as raw
/// JSON, preserving fields Desktop does not model, including nested objects.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDocument {
    version: SettingsDocumentVersion,
    revision: u64,
    fields: Map<String, Value>,
}

impl SettingsDocument {
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, SettingsPatchError> {
        if bytes.len() > MAX_SETTINGS_DOCUMENT_BYTES {
            return Err(SettingsPatchError::SizeLimit {
                actual: bytes.len(),
                max: MAX_SETTINGS_DOCUMENT_BYTES,
            });
        }
        let object: UniqueTopLevelObject =
            serde_json::from_slice(bytes).map_err(SettingsPatchError::InvalidJson)?;
        Self::from_object(
            object.fields,
            object.schema_token.as_deref(),
            object.revision_token.as_deref(),
        )
    }

    pub fn from_value(value: Value) -> Result<Self, SettingsPatchError> {
        let Value::Object(object) = value else {
            return Err(SettingsPatchError::NotObject);
        };
        let schema_token = object.get(SCHEMA_VERSION_KEY).map(Value::to_string);
        let revision_token = object.get(REVISION_KEY).map(Value::to_string);
        Self::from_object(object, schema_token.as_deref(), revision_token.as_deref())
    }

    fn from_object(
        mut object: Map<String, Value>,
        schema_token: Option<&str>,
        revision_token: Option<&str>,
    ) -> Result<Self, SettingsPatchError> {
        let schema = object.remove(SCHEMA_VERSION_KEY);
        let revision = object.remove(REVISION_KEY);
        match (schema, revision) {
            (None, None) => Ok(Self {
                version: SettingsDocumentVersion::LegacyV0,
                revision: 0,
                fields: object,
            }),
            (Some(Value::Number(_)), Some(Value::Number(_))) => {
                let schema = strict_unsigned_integer(
                    schema_token.ok_or(SettingsPatchError::InvalidSchemaVersion)?,
                    SettingsPatchError::InvalidSchemaVersion,
                )?;
                if schema != SETTINGS_SCHEMA_VERSION {
                    return Err(SettingsPatchError::UnsupportedSchemaVersion(schema));
                }
                let revision = strict_unsigned_integer(
                    revision_token.ok_or(SettingsPatchError::InvalidRevision)?,
                    SettingsPatchError::InvalidRevision,
                )?;
                Ok(Self {
                    version: SettingsDocumentVersion::V1,
                    revision,
                    fields: object,
                })
            }
            (Some(_), Some(_)) => Err(SettingsPatchError::InvalidEnvelope),
            _ => Err(SettingsPatchError::InvalidEnvelope),
        }
    }

    pub fn version(&self) -> SettingsDocumentVersion {
        self.version
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Raw non-envelope top-level values. Callers must make their desired map
    /// from this view so unowned keys are carried forward unchanged.
    pub fn fields(&self) -> &Map<String, Value> {
        &self.fields
    }

    /// Canonical v1 serialization of this semantic document. Legacy v0 is
    /// emitted only when no patch asks to change it; successful changes upgrade
    /// it to v1 and revision 1.
    pub fn to_value(&self) -> Value {
        let mut object = self.fields.clone();
        if self.version == SettingsDocumentVersion::V1 {
            object.insert(
                SCHEMA_VERSION_KEY.to_owned(),
                Value::Number(SETTINGS_SCHEMA_VERSION.into()),
            );
            object.insert(REVISION_KEY.to_owned(), Value::Number(self.revision.into()));
        }
        Value::Object(object)
    }

    fn next_version(&self, fields: Map<String, Value>) -> Result<Self, SettingsPatchError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SettingsPatchError::RevisionOverflow)?;
        Ok(Self {
            version: SettingsDocumentVersion::V1,
            revision,
            fields,
        })
    }

    pub fn legacy_v0_fixture() -> Result<Self, SettingsPatchError> {
        verify_fixture_sha256(
            "settings-document-v0-legacy.json",
            SETTINGS_V0_LEGACY_FIXTURE,
            SETTINGS_V0_LEGACY_FIXTURE_SHA256,
        )
        .map_err(SettingsPatchError::Fixture)?;
        Self::parse_bytes(SETTINGS_V0_LEGACY_FIXTURE)
    }

    pub fn v1_unknown_fixture() -> Result<Self, SettingsPatchError> {
        verify_fixture_sha256(
            "settings-document-v1-unknown.json",
            SETTINGS_V1_UNKNOWN_FIXTURE,
            SETTINGS_V1_UNKNOWN_FIXTURE_SHA256,
        )
        .map_err(SettingsPatchError::Fixture)?;
        Self::parse_bytes(SETTINGS_V1_UNKNOWN_FIXTURE)
    }
}

fn strict_unsigned_integer(
    token: &str,
    error: SettingsPatchError,
) -> Result<u64, SettingsPatchError> {
    let token = token.trim();
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error);
    }
    token.parse::<u64>().map_err(|_| error)
}

struct UniqueTopLevelObject {
    fields: Map<String, Value>,
    schema_token: Option<String>,
    revision_token: Option<String>,
}

impl<'de> Deserialize<'de> for UniqueTopLevelObject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UniqueObjectVisitor;
        impl<'de> Visitor<'de> for UniqueObjectVisitor {
            type Value = UniqueTopLevelObject;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a settings JSON object with unique top-level keys")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut fields = Map::new();
                let mut schema_token = None;
                let mut revision_token = None;
                while let Some(key) = access.next_key::<String>()? {
                    let raw = access.next_value::<Box<RawValue>>()?;
                    if fields.contains_key(&key) {
                        return Err(A::Error::custom(format!(
                            "duplicate top-level settings key {key}"
                        )));
                    }
                    if key == SCHEMA_VERSION_KEY {
                        schema_token = Some(raw.get().to_string());
                    } else if key == REVISION_KEY {
                        revision_token = Some(raw.get().to_string());
                    }
                    let value: Value = serde_json::from_str(raw.get()).map_err(A::Error::custom)?;
                    fields.insert(key, value);
                }
                Ok(UniqueTopLevelObject {
                    fields,
                    schema_token,
                    revision_token,
                })
            }
        }
        deserializer.deserialize_map(UniqueObjectVisitor)
    }
}

/// A desired full top-level view against the snapshot it was based on.
///
/// Omitting a key from `desired` requests its removal. The caller should clone
/// `base` and edit only a whitelisted key for normal use.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsThreeWayPatch {
    base: Map<String, Value>,
    desired: Map<String, Value>,
}

impl SettingsThreeWayPatch {
    pub fn new(
        base: Map<String, Value>,
        desired: Map<String, Value>,
    ) -> Result<Self, SettingsPatchError> {
        reject_reserved_keys(&base)?;
        reject_reserved_keys(&desired)?;
        Ok(Self { base, desired })
    }

    pub fn from_document_and_desired(
        base: &SettingsDocument,
        desired: Map<String, Value>,
    ) -> Result<Self, SettingsPatchError> {
        Self::new(base.fields.clone(), desired)
    }

    /// Applies no partial changes. A conflict in one key leaves every key
    /// untouched. If current already equals desired, the result is idempotent
    /// and retains current's revision rather than causing a needless write.
    pub fn apply(
        &self,
        current: &SettingsDocument,
    ) -> Result<SettingsPatchResult, SettingsPatchError> {
        let current_fields = current.fields();
        let keys: BTreeSet<&String> = self
            .base
            .keys()
            .chain(self.desired.keys())
            .chain(current_fields.keys())
            .collect();

        let mut conflicts = Vec::new();
        let mut changes = Vec::new();
        for key in keys {
            let base = self.base.get(key);
            let desired = self.desired.get(key);
            let current_value = current_fields.get(key);
            if desired == base {
                continue;
            }
            if !is_desktop_v1_writable(key) {
                return Err(SettingsPatchError::KeyNotWhitelisted(key.clone()));
            }
            if current_value == base {
                changes.push((key.clone(), desired.cloned()));
            } else if current_value == desired {
                // This key was already written by a prior equivalent patch.
            } else {
                conflicts.push(SettingsPatchConflict {
                    key: key.clone(),
                    base: base.cloned(),
                    current: current_value.cloned(),
                    desired: desired.cloned(),
                });
            }
        }
        if !conflicts.is_empty() {
            return Err(SettingsPatchError::Conflict(conflicts));
        }
        if changes.is_empty() {
            return Ok(SettingsPatchResult {
                document: current.clone(),
                changed_keys: Vec::new(),
                write_required: false,
            });
        }

        let mut fields = current.fields.clone();
        let mut changed_keys = Vec::with_capacity(changes.len());
        for (key, value) in changes {
            changed_keys.push(key.clone());
            match value {
                Some(value) => {
                    fields.insert(key, value);
                }
                None => {
                    fields.remove(&key);
                }
            }
        }
        changed_keys.sort();
        Ok(SettingsPatchResult {
            document: current.next_version(fields)?,
            changed_keys,
            write_required: true,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsPatchResult {
    pub document: SettingsDocument,
    pub changed_keys: Vec<String>,
    /// False for an already-applied patch. The future product writer must then
    /// skip touching the shared file and keep its revision unchanged.
    pub write_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatchConflict {
    pub key: String,
    pub base: Option<Value>,
    pub current: Option<Value>,
    pub desired: Option<Value>,
}

#[derive(Debug, Error)]
pub enum SettingsPatchError {
    #[error("settings document is larger than {max} bytes ({actual} bytes)")]
    SizeLimit { actual: usize, max: usize },
    #[error("settings JSON is invalid: {0}")]
    InvalidJson(serde_json::Error),
    #[error("settings document must be a top-level JSON object")]
    NotObject,
    #[error("settings envelope must contain both schemaVersion and revision, or neither")]
    InvalidEnvelope,
    #[error("settings schemaVersion must be an unsigned integer")]
    InvalidSchemaVersion,
    #[error("settings revision must be an unsigned integer")]
    InvalidRevision,
    #[error("settings schema version {0} is unsupported")]
    UnsupportedSchemaVersion(u64),
    #[error("settings revision overflow")]
    RevisionOverflow,
    #[error("settings patch may not include reserved key {0}")]
    ReservedKey(String),
    #[error("settings key {0} is not in Desktop's v1 writer whitelist")]
    KeyNotWhitelisted(String),
    #[error("settings patch conflicts with current document")]
    Conflict(Vec<SettingsPatchConflict>),
    #[error("settings fixture verification failed: {0}")]
    Fixture(crate::storage_contract::ContractError),
}

fn reject_reserved_keys(fields: &Map<String, Value>) -> Result<(), SettingsPatchError> {
    for key in RESERVED_KEYS {
        if fields.contains_key(key) {
            return Err(SettingsPatchError::ReservedKey(key.to_owned()));
        }
    }
    Ok(())
}

fn is_desktop_v1_writable(key: &str) -> bool {
    DESKTOP_V1_WRITABLE_KEYS.contains(&key)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn v0(value: Value) -> SettingsDocument {
        SettingsDocument::from_value(value).unwrap()
    }

    #[test]
    fn v0_and_v1_keep_unknown_top_level_and_nested_values() {
        assert_eq!(SETTINGS_RELATIVE_PATH, "settings.json");
        let legacy = SettingsDocument::legacy_v0_fixture().unwrap();
        assert_eq!(legacy.version(), SettingsDocumentVersion::LegacyV0);
        assert_eq!(legacy.revision(), 0);
        assert_eq!(legacy.fields()["future"]["nested"], true);
        let v1 = SettingsDocument::v1_unknown_fixture().unwrap();
        assert_eq!(v1.version(), SettingsDocumentVersion::V1);
        assert_eq!(v1.revision(), 7);
        assert_eq!(
            v1.fields()["menuBarItems"][0]["futureNested"]["schemaVersion"],
            "opaque"
        );
        assert_eq!(v1.fields()["topLevelUnknown"], "keep-me");
    }

    #[test]
    fn rejects_non_object_unknown_schema_and_invalid_revision() {
        assert!(matches!(
            SettingsDocument::from_value(json!([])),
            Err(SettingsPatchError::NotObject)
        ));
        assert!(matches!(
            SettingsDocument::from_value(json!({"schemaVersion": 2, "revision": 0})),
            Err(SettingsPatchError::UnsupportedSchemaVersion(2))
        ));
        assert!(matches!(
            SettingsDocument::from_value(json!({"schemaVersion": 1, "revision": -1})),
            Err(SettingsPatchError::InvalidRevision)
        ));
        assert!(matches!(
            SettingsDocument::from_value(json!({"schemaVersion": 1})),
            Err(SettingsPatchError::InvalidEnvelope)
        ));
        let signed_zero = SettingsDocument::parse_bytes(br#"{"schemaVersion":1,"revision":-0}"#);
        assert!(
            matches!(signed_zero, Err(SettingsPatchError::InvalidRevision)),
            "got {signed_zero:?}"
        );
        assert!(
            SettingsDocument::parse_bytes(br#"{"schemaVersion":1,"revision":1,"revision":2}"#)
                .is_err()
        );
    }

    #[test]
    fn applies_three_way_patch_losslessly_and_bumps_revision() {
        let base = v0(json!({"displayMode":"remaining","future":{"deep":true}}));
        let desired = fields(json!({"displayMode":"used","future":{"deep":true}}));
        let patch = SettingsThreeWayPatch::from_document_and_desired(&base, desired).unwrap();
        let result = patch.apply(&base).unwrap();
        assert!(result.write_required);
        assert_eq!(result.changed_keys, vec!["displayMode"]);
        assert_eq!(result.document.version(), SettingsDocumentVersion::V1);
        assert_eq!(result.document.revision(), 1);
        assert_eq!(result.document.fields()["future"]["deep"], true);
        let serialized = result.document.to_value();
        assert_eq!(serialized["schemaVersion"], 1);
        assert_eq!(serialized["revision"], 1);
        assert_eq!(serialized["future"]["deep"], true);
    }

    #[test]
    fn applies_v1_patch_from_the_current_revision() {
        let base = SettingsDocument::from_value(
            json!({"schemaVersion":1,"revision":41,"displayMode":"remaining"}),
        )
        .unwrap();
        let patch = SettingsThreeWayPatch::from_document_and_desired(
            &base,
            fields(json!({"displayMode":"used"})),
        )
        .unwrap();
        let result = patch.apply(&base).unwrap();
        assert!(result.write_required);
        assert_eq!(result.document.version(), SettingsDocumentVersion::V1);
        assert_eq!(result.document.revision(), 42);
    }

    #[test]
    fn treats_current_desired_as_idempotent_without_bumping_revision() {
        let base = v0(json!({"displayMode":"remaining"}));
        let desired = fields(json!({"displayMode":"used"}));
        let patch = SettingsThreeWayPatch::from_document_and_desired(&base, desired).unwrap();
        let current = SettingsDocument::from_value(
            json!({"schemaVersion":1,"revision":9,"displayMode":"used"}),
        )
        .unwrap();
        let result = patch.apply(&current).unwrap();
        assert!(!result.write_required);
        assert_eq!(result.document.revision(), 9);
    }

    #[test]
    fn reports_structured_conflicts_without_partial_apply() {
        let base = v0(json!({"displayMode":"remaining","refreshIntervalSeconds":600}));
        let patch = SettingsThreeWayPatch::from_document_and_desired(
            &base,
            fields(json!({"displayMode":"used","refreshIntervalSeconds":300})),
        )
        .unwrap();
        let current = SettingsDocument::from_value(json!({"schemaVersion":1,"revision":4,"displayMode":"remaining","refreshIntervalSeconds":120})).unwrap();
        let error = patch.apply(&current).unwrap_err();
        let SettingsPatchError::Conflict(conflicts) = error else {
            panic!("expected conflict")
        };
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].key, "refreshIntervalSeconds");
        assert_eq!(conflicts[0].base, Some(json!(600)));
        assert_eq!(conflicts[0].current, Some(json!(120)));
        assert_eq!(conflicts[0].desired, Some(json!(300)));
    }

    #[test]
    fn native_fixture_vectors_drive_the_same_merge_results() {
        verify_fixture_sha256(
            "settings-document-vectors.json",
            SETTINGS_VECTORS_FIXTURE,
            SETTINGS_VECTORS_FIXTURE_SHA256,
        )
        .unwrap();
        let vectors: Value = serde_json::from_slice(SETTINGS_VECTORS_FIXTURE).unwrap();

        let non_conflict = &vectors["nonConflict"];
        let base = SettingsDocument::from_value(non_conflict["base"].clone()).unwrap();
        let current = SettingsDocument::from_value(non_conflict["current"].clone()).unwrap();
        let desired = non_conflict["desired"].as_object().unwrap().clone();
        let desired = SettingsDocument::from_value(Value::Object(desired)).unwrap();
        let patch =
            SettingsThreeWayPatch::from_document_and_desired(&base, desired.fields().clone())
                .unwrap();
        let result = patch.apply(&current).unwrap();
        assert_eq!(result.document.revision(), 4);
        assert_eq!(result.document.fields()["displayMode"], "used");
        assert_eq!(result.document.fields()["refreshIntervalSeconds"], 900);
        assert_eq!(result.document.fields()["future"]["keep"], true);

        let conflict = &vectors["conflict"];
        let base = SettingsDocument::from_value(conflict["base"].clone()).unwrap();
        let current = SettingsDocument::from_value(conflict["current"].clone()).unwrap();
        let desired = SettingsDocument::from_value(conflict["desired"].clone()).unwrap();
        let patch =
            SettingsThreeWayPatch::from_document_and_desired(&base, desired.fields().clone())
                .unwrap();
        let error = patch.apply(&current).unwrap_err();
        let SettingsPatchError::Conflict(conflicts) = error else {
            panic!("expected conflict")
        };
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].key, "refreshIntervalSeconds");
    }

    #[test]
    fn forbids_envelope_keys_and_non_whitelisted_changes() {
        assert!(matches!(
            SettingsThreeWayPatch::new(fields(json!({"schemaVersion":1})), Map::new()),
            Err(SettingsPatchError::ReservedKey(_))
        ));
        let base = v0(json!({"futureKey":false}));
        let patch = SettingsThreeWayPatch::from_document_and_desired(
            &base,
            fields(json!({"futureKey":true})),
        )
        .unwrap();
        assert!(
            matches!(patch.apply(&base), Err(SettingsPatchError::KeyNotWhitelisted(key)) if key == "futureKey")
        );

        let base = v0(json!({"visibleMiscProviders":["kimi"]}));
        let patch = SettingsThreeWayPatch::from_document_and_desired(
            &base,
            fields(json!({"visibleMiscProviders":[]})),
        )
        .unwrap();
        assert!(
            matches!(patch.apply(&base), Err(SettingsPatchError::KeyNotWhitelisted(key)) if key == "visibleMiscProviders")
        );
    }

    #[test]
    fn enforces_byte_size_cap_before_json_parsing() {
        let bytes = vec![b'x'; MAX_SETTINGS_DOCUMENT_BYTES + 1];
        assert!(matches!(
            SettingsDocument::parse_bytes(&bytes),
            Err(SettingsPatchError::SizeLimit { .. })
        ));
    }

    #[test]
    fn arbitrary_precision_unknown_numbers_survive_an_unrelated_patch() {
        const INTEGER: &str = "12345678901234567890123456789012345678901234567890";
        const DECIMAL: &str = "0.12345678901234567890123456789012345678901234567890";
        let base = SettingsDocument::v1_unknown_fixture().unwrap();
        assert_eq!(base.fields()["unknownPreciseInteger"].to_string(), INTEGER);
        assert_eq!(base.fields()["unknownPreciseDecimal"].to_string(), DECIMAL);
        let mut desired = base.fields().clone();
        desired.insert("displayMode".to_string(), json!("used"));
        let patch = SettingsThreeWayPatch::from_document_and_desired(&base, desired).unwrap();
        let result = patch.apply(&base).unwrap();
        assert_eq!(result.document.revision(), 8);
        let encoded = serde_json::to_string(&result.document.to_value()).unwrap();
        assert!(encoded.contains(&format!("\"unknownPreciseInteger\":{INTEGER}")));
        assert!(encoded.contains(&format!("\"unknownPreciseDecimal\":{DECIMAL}")));
    }
}
