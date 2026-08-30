/// Canonical on-disk diagnostic record. Its JSON must remain byte-identical to
/// Swift's `JSONEncoder.outputFormatting = [.sortedKeys]` output.
use std::collections::BTreeMap;

use serde::Deserialize;

use super::{
    verify_fixture_sha256, LeaseError, SharedStoreLeaseRole, LEASE_RECORD_FIXTURE,
    LEASE_RECORD_FIXTURE_SHA256,
};
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedStoreLeaseRecord {
    pub version: u32,
    pub role: SharedStoreLeaseRole,
    pub pid: i32,
    pub started_at: i64,
    #[serde(rename = "clientID")]
    pub client_id: String,
}

impl SharedStoreLeaseRecord {
    pub const VERSION: u32 = 1;
    pub fn new(
        role: SharedStoreLeaseRole,
        pid: i32,
        started_at: i64,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            version: Self::VERSION,
            role,
            pid,
            started_at,
            client_id: client_id.into(),
        }
    }
    pub fn validate(&self) -> Result<(), LeaseError> {
        if self.version != Self::VERSION {
            return Err(LeaseError::InvalidRecord("unsupported version"));
        }
        if self.pid <= 0 {
            return Err(LeaseError::InvalidRecord("pid must be positive"));
        }
        if !valid_client_id(&self.client_id) {
            return Err(LeaseError::InvalidClientId);
        }
        Ok(())
    }
    pub fn canonical_json(&self) -> Result<Vec<u8>, LeaseError> {
        self.validate()?;
        let mut fields = BTreeMap::new();
        fields.insert(
            "clientID",
            serde_json::Value::String(self.client_id.clone()),
        );
        fields.insert("pid", serde_json::Value::Number(self.pid.into()));
        fields.insert(
            "role",
            serde_json::Value::String(self.role.as_raw().to_owned()),
        );
        fields.insert(
            "startedAt",
            serde_json::Value::Number(self.started_at.into()),
        );
        fields.insert("version", serde_json::Value::Number(self.version.into()));
        serde_json::to_vec(&fields).map_err(|error| LeaseError::Json(error.to_string()))
    }
    pub fn from_canonical_json(bytes: &[u8]) -> Result<Self, LeaseError> {
        let record: Self =
            serde_json::from_slice(bytes).map_err(|error| LeaseError::Json(error.to_string()))?;
        record.validate()?;
        if record.canonical_json()? != bytes {
            return Err(LeaseError::InvalidRecord("JSON is not canonical"));
        }
        Ok(record)
    }
    pub fn native_fixture() -> Result<Self, LeaseError> {
        verify_fixture_sha256(
            "shared-store-lease-record-v1.json",
            LEASE_RECORD_FIXTURE,
            LEASE_RECORD_FIXTURE_SHA256,
        )
        .map_err(LeaseError::Contract)?;
        Self::from_canonical_json(LEASE_RECORD_FIXTURE)
    }
}

pub(super) fn valid_client_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}
