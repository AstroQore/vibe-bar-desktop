//! Read-only view of `service_status.json`.
//!
//! **Wire shape gotcha.** The native app persists a Swift
//! `[ToolType: ServiceStatusSnapshot]`, and because `ToolType` is not
//! `CodingKeyRepresentable`, Foundation encodes it as a *flat alternating
//! array* — `["cursor", {…}, "kimi", {…}]` — not as an object. Decoding it as
//! a map is the obvious mistake, so the parsing lives here once.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::model::ToolType;
use crate::paths::DataRoot;

const MAX_STATUS_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatusSnapshot {
    /// "none" | "minor" | "major" | "critical" | "maintenance".
    #[serde(default)]
    pub indicator: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Apple reference-date seconds as stored; converted on read.
    #[serde(default)]
    pub updated_at: Option<f64>,
}

impl ServiceStatusSnapshot {
    /// True when the provider is reporting anything other than "all good".
    pub fn is_degraded(&self) -> bool {
        !matches!(self.indicator.as_deref(), Some("none") | None)
    }
}

/// Per-tool status, decoded from the alternating-array encoding.
pub fn load(root: &DataRoot) -> HashMap<ToolType, ServiceStatusSnapshot> {
    let Some(value): Option<Value> =
        super::read_json_file(&root.service_status_file(), MAX_STATUS_BYTES)
    else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    match value {
        // The shape the native app actually writes.
        Value::Array(items) => {
            for pair in items.chunks(2) {
                let [key, body] = pair else { continue };
                let Some(tool) = key.as_str().and_then(ToolType::from_raw) else {
                    continue;
                };
                if let Ok(mut snapshot) =
                    serde_json::from_value::<ServiceStatusSnapshot>(body.clone())
                {
                    snapshot.updated_at = snapshot.updated_at.map(super::apple_seconds_to_unix);
                    out.insert(tool, snapshot);
                }
            }
        }
        // Accepted defensively in case a future writer switches to a map.
        Value::Object(map) => {
            for (key, body) in map {
                let Some(tool) = ToolType::from_raw(&key) else {
                    continue;
                };
                if let Ok(mut snapshot) = serde_json::from_value::<ServiceStatusSnapshot>(body) {
                    snapshot.updated_at = snapshot.updated_at.map(super::apple_seconds_to_unix);
                    out.insert(tool, snapshot);
                }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_alternating_array_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(
            root.service_status_file(),
            serde_json::json!([
                "codex",
                {"indicator": "none", "description": "All Systems Operational",
                 "updatedAt": 809_731_205.0},
                "claude",
                {"indicator": "minor", "description": "Elevated error rates"},
                "someFutureProvider",
                {"indicator": "none"}
            ])
            .to_string(),
        )
        .unwrap();

        let status = load(&root);
        assert_eq!(status.len(), 2, "unknown providers are skipped");
        assert!(!status[&ToolType::Codex].is_degraded());
        assert!(status[&ToolType::Claude].is_degraded());
        let updated = status[&ToolType::Codex].updated_at.unwrap();
        assert!((updated - 1_788_038_405.0).abs() < 1.0);
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(load(&root).is_empty());
    }
}
