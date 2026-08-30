//! Read-only view of the shared `settings.json`.
//!
//! Only the fields this client actually renders are typed. Everything else is
//! kept verbatim in `unknown` so nothing is ever lost in a round trip — a
//! precondition for the day Desktop is allowed to write this file, and the
//! reason this struct must never be used to re-serialize a partial view.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths::DataRoot;

const MAX_SETTINGS_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SharedSettings {
    /// "remaining" | "used".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_interval_seconds: Option<f64>,
    /// "forecast" | "actual".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_bar_color_basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_bar_items: Option<Vec<MenuBarItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_core_providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_provider_order: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misc_provider_instances: Option<Vec<MiscProviderInstance>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_misc_providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_plan_labels: Option<BTreeMap<String, String>>,

    /// Every key this build does not model, preserved byte-for-byte.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MenuBarItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_title: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_field_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_labels: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiscProviderInstance {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

impl SharedSettings {
    /// Load the shared settings, or defaults when absent/unreadable. A
    /// missing file is the normal case on a machine that has only ever run
    /// Desktop.
    pub fn load(root: &DataRoot) -> Self {
        super::read_json_file(&root.settings_file(), MAX_SETTINGS_BYTES).unwrap_or_default()
    }

    /// Refresh cadence, honoring the native app's floor of 60 s. Defaults to
    /// 10 minutes, matching the native default.
    pub fn refresh_interval(&self) -> std::time::Duration {
        let seconds = self.refresh_interval_seconds.unwrap_or(600.0).max(60.0);
        std::time::Duration::from_secs_f64(seconds)
    }

    /// True when percentages should read as "remaining" rather than "used".
    pub fn shows_remaining(&self) -> bool {
        self.display_mode.as_deref() != Some("used")
    }

    /// The ordered menu-bar field ids the user configured, with their custom
    /// labels. Empty when the shared settings are absent — the caller then
    /// picks its own default set.
    pub fn menu_bar_fields(&self) -> (Vec<String>, BTreeMap<String, String>) {
        let Some(item) = self
            .menu_bar_items
            .as_ref()
            .and_then(|items| items.first())
        else {
            return (Vec::new(), BTreeMap::new());
        };
        (
            item.selected_field_ids.clone().unwrap_or_default(),
            item.custom_labels.clone().unwrap_or_default(),
        )
    }

    /// Candidate account ids for the shared quota cache. The cache is keyed
    /// by `sha256(accountId)`, so an id can only be recovered by guessing it
    /// and hashing — these are the stable ids the native `AccountStore`
    /// mints, plus one per configured misc instance.
    pub fn candidate_account_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = [
            "oauth-codex",
            "cli-codex",
            "web-codex",
            "oauth-claude",
            "cli-claude",
            "web-claude",
            "claude",
            "web-gemini",
            "web-antigravity",
            "local-antigravity",
            "oauth-grok",
            "web-grok",
            "cursor",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        if let Some(instances) = &self.misc_provider_instances {
            for instance in instances {
                ids.push(format!("misc-{}", instance.id));
            }
        }
        // A misc provider the user never cloned keeps `id == tool.rawValue`.
        for tool in crate::model::ToolType::ALL {
            ids.push(format!("misc-{}", tool.raw_value()));
        }
        ids.sort();
        ids.dedup();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_usable_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let settings = SharedSettings::load(&root);
        assert_eq!(settings.refresh_interval().as_secs(), 600);
        assert!(settings.shows_remaining());
        assert!(settings.menu_bar_fields().0.is_empty());
    }

    #[test]
    fn reads_real_shape_and_preserves_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(
            root.settings_file(),
            serde_json::json!({
                "displayMode": "used",
                "refreshIntervalSeconds": 300,
                "menuBarColorBasis": "forecast",
                "menuBarItems": [{
                    "kind": "compact",
                    "layout": "twoRows",
                    "isVisible": true,
                    "showTitle": false,
                    "selectedFieldIds": ["codex.weekly", "claude.weekly"],
                    "customLabels": {"codex.weekly": "ChatGPT"}
                }],
                "visibleCoreProviders": ["codex", "claude"],
                "skillsSyncMethod": "auto",
                "someFutureKey": {"nested": true}
            })
            .to_string(),
        )
        .unwrap();

        let settings = SharedSettings::load(&root);
        assert!(!settings.shows_remaining());
        assert_eq!(settings.refresh_interval().as_secs(), 300);
        let (fields, labels) = settings.menu_bar_fields();
        assert_eq!(fields, vec!["codex.weekly", "claude.weekly"]);
        assert_eq!(labels.get("codex.weekly").unwrap(), "ChatGPT");
        // Unmodelled keys survive.
        assert!(settings.unknown.contains_key("someFutureKey"));
        assert!(settings.unknown.contains_key("skillsSyncMethod"));
    }

    #[test]
    fn refresh_interval_honors_native_floor() {
        let settings = SharedSettings {
            refresh_interval_seconds: Some(5.0),
            ..Default::default()
        };
        assert_eq!(settings.refresh_interval().as_secs(), 60);
    }
}
