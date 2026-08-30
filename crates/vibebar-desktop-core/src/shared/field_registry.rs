//! Read-only view of `quota_field_registry.json` — quota buckets the native
//! app has actually observed that are not in any build's hardcoded catalog.
//!
//! This is how a provider-side new bucket (a new Claude model lane, say)
//! becomes selectable without shipping a release, so Desktop reads it for the
//! same reason: to label fields it has no compiled-in knowledge of.

use std::collections::HashMap;

use serde::Deserialize;

use crate::paths::DataRoot;

const MAX_REGISTRY_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    fields: Vec<DiscoveredField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredField {
    /// Fully qualified field id, e.g. `claude.weekly_fable`.
    pub id: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub group_title: Option<String>,
    #[serde(default)]
    pub short_label: Option<String>,
    #[serde(default)]
    pub first_seen: Option<f64>,
    #[serde(default)]
    pub last_seen: Option<f64>,
}

/// Discovered fields keyed by field id.
pub fn load(root: &DataRoot) -> HashMap<String, DiscoveredField> {
    let file: RegistryFile =
        super::read_json_file(&root.quota_field_registry_file(), MAX_REGISTRY_BYTES)
            .unwrap_or_default();
    file.fields
        .into_iter()
        .map(|f| (f.id.clone(), f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_registry_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(load(&root).is_empty());
    }

    #[test]
    fn reads_discovered_fields() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(
            root.quota_field_registry_file(),
            serde_json::json!({
                "fields": [{
                    "id": "claude.weekly_newmodel",
                    "tool": "claude",
                    "title": "Weekly",
                    "groupTitle": "NewModel",
                    "shortLabel": "NewModel wk",
                    "firstSeen": 809_000_000.0,
                    "lastSeen": 809_731_205.0
                }]
            })
            .to_string(),
        )
        .unwrap();

        let fields = load(&root);
        let field = fields.get("claude.weekly_newmodel").unwrap();
        assert_eq!(field.group_title.as_deref(), Some("NewModel"));
        assert_eq!(field.tool.as_deref(), Some("claude"));
    }
}
