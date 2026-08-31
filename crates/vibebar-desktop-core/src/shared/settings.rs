//! Reading the shared `settings.json`.
//!
//! Only the fields this client actually renders are typed. Everything else is
//! kept verbatim in `unknown` so nothing is ever lost in a round trip, and
//! this struct must never be used to re-serialize a partial view: that is what
//! would delete the keys it does not know.
//!
//! Writing goes through [`super::settings_writer`], which never builds a
//! document from this type — it puts the changed keys onto the raw object the
//! file holds at that moment.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths::DataRoot;

const MAX_SETTINGS_BYTES: u64 = 8 * 1024 * 1024;

/// The shared mini-window preferences this client reads.
///
/// Only `displayMode` so far. The rest of the pane — per-window geometry,
/// label overrides, the cycle order — belongs to windows this client does not
/// have yet, and is kept in `unknown` rather than typed and ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MiniWindowSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

/// The native app's Settings → Cost Data pane.
///
/// One setting here is not a preference about presentation: privacy mode
/// stops that client scanning local sessions for spend at all. It is a
/// statement about the machine, so this client honours it too — otherwise
/// turning it on hides spend in one window and leaves it on show in the
/// other, which is not privacy.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostDataSettings {
    #[serde(default)]
    pub privacy_mode_enabled: bool,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_data: Option<CostDataSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mini_window: Option<MiniWindowSettings>,

    /// Every key this build does not model, preserved byte-for-byte.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSettings {
    pub display_mode: String,
    pub refresh_interval_seconds: u64,
    pub menu_bar_color_basis: String,
    pub selected_field_ids: Vec<String>,
    pub custom_labels: BTreeMap<String, String>,
    pub visible_core_providers: Option<Vec<String>>,
    pub core_provider_order: Vec<String>,
    pub visible_misc_providers: Option<Vec<String>>,
    pub provider_plan_labels: BTreeMap<String, String>,
    /// The mini-window layout, among the ones this client draws.
    pub mini_display_mode: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
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
    /// Whether the user has asked, in either client, that local spend not be
    /// read. Absent settings mean off, which is the native default.
    /// Which mini-window layout the shared settings ask for, among the ones
    /// this client draws. A mode it has not ported yet falls back to
    /// `regular`: showing the arrangement in a shape the user did not pick is
    /// better than showing nothing, and the parity table says which exist.
    pub fn mini_display_mode(&self) -> &'static str {
        match self
            .mini_window
            .as_ref()
            .and_then(|mini| mini.display_mode.as_deref())
        {
            Some("compact") => "compact",
            Some("ledger") => "ledger",
            Some("tile") => "tile",
            _ => "regular",
        }
    }

    pub fn cost_privacy_mode(&self) -> bool {
        self.cost_data
            .as_ref()
            .is_some_and(|cost| cost.privacy_mode_enabled)
    }

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
        let Some(item) = self.menu_bar_items.as_ref().and_then(|items| items.first()) else {
            return (Vec::new(), BTreeMap::new());
        };
        (
            item.selected_field_ids.clone().unwrap_or_default(),
            item.custom_labels.clone().unwrap_or_default(),
        )
    }

    pub fn presentation(&self) -> PresentationSettings {
        let (selected_field_ids, custom_labels) = self.menu_bar_fields();
        let refresh = self.refresh_interval_seconds.unwrap_or(600.0);
        let refresh_interval_seconds = if refresh.is_finite() {
            refresh.max(60.0).round() as u64
        } else {
            600
        };
        let visible_misc_providers = self
            .misc_provider_instances
            .as_ref()
            .map(|instances| {
                instances
                    .iter()
                    .filter(|instance| instance.is_visible != Some(false))
                    .filter_map(|instance| {
                        instance.tool.clone().or_else(|| {
                            crate::model::ToolType::from_raw(&instance.id)
                                .map(|tool| tool.raw_value().to_string())
                        })
                    })
                    .collect()
            })
            .or_else(|| self.visible_misc_providers.clone());
        PresentationSettings {
            display_mode: if self.display_mode.as_deref() == Some("used") {
                "used".into()
            } else {
                "remaining".into()
            },
            refresh_interval_seconds,
            menu_bar_color_basis: if self.menu_bar_color_basis.as_deref() == Some("actual") {
                "actual".into()
            } else {
                "forecast".into()
            },
            selected_field_ids,
            custom_labels,
            visible_core_providers: self.visible_core_providers.clone(),
            core_provider_order: self.core_provider_order.clone().unwrap_or_else(|| {
                ["codex", "claude", "gemini", "grok"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            }),
            visible_misc_providers,
            provider_plan_labels: self.provider_plan_labels.clone().unwrap_or_default(),
            mini_display_mode: self.mini_display_mode().to_string(),
        }
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

    /// A layout this client has not ported falls back to the one it draws.
    /// Silently: a mini window has no room to explain itself, and the parity
    /// table is where "which layouts exist" belongs.
    #[test]
    fn an_unported_mini_layout_falls_back_to_the_one_that_exists() {
        let with_mode = |mode: &str| {
            serde_json::from_str::<SharedSettings>(&format!(
                r#"{{"miniWindow":{{"displayMode":"{mode}"}}}}"#
            ))
            .expect("settings parse")
            .mini_display_mode()
        };

        assert_eq!(with_mode("compact"), "compact");
        assert_eq!(with_mode("ledger"), "ledger");
        assert_eq!(with_mode("tile"), "tile");
        assert_eq!(with_mode("regular"), "regular");
        for unported in ["strip", "focus", "rail", "somethingNewer"] {
            assert_eq!(with_mode(unported), "regular", "{unported}");
        }
        assert_eq!(
            serde_json::from_str::<SharedSettings>("{}")
                .expect("empty settings parse")
                .mini_display_mode(),
            "regular",
            "no mini settings at all"
        );
    }

    /// The rest of the pane belongs to windows this client does not have, and
    /// must survive a round trip rather than being typed and dropped.
    #[test]
    fn keeps_the_mini_settings_it_does_not_understand() {
        let settings: SharedSettings = serde_json::from_str(
            r#"{"miniWindow":{"displayMode":"compact","cycleModes":["tile"],"size":{"w":1}}}"#,
        )
        .expect("settings parse");
        let mini = settings.mini_window.as_ref().expect("miniWindow");
        assert!(mini.unknown.contains_key("cycleModes"));
        assert!(mini.unknown.contains_key("size"));
    }

    #[test]
    fn presentation_uses_effective_values_and_instance_visibility() {
        let settings = SharedSettings {
            display_mode: Some("used".into()),
            refresh_interval_seconds: Some(5.2),
            menu_bar_color_basis: Some("actual".into()),
            menu_bar_items: Some(vec![MenuBarItem {
                selected_field_ids: Some(vec!["codex.weekly".into()]),
                custom_labels: Some(BTreeMap::from([("codex.weekly".into(), "ChatGPT".into())])),
                ..Default::default()
            }]),
            visible_core_providers: Some(vec!["codex".into()]),
            misc_provider_instances: Some(vec![
                MiscProviderInstance {
                    id: "kilo".into(),
                    tool: None,
                    name: None,
                    is_visible: Some(true),
                    unknown: BTreeMap::new(),
                },
                MiscProviderInstance {
                    id: "two".into(),
                    tool: Some("zai".into()),
                    name: None,
                    is_visible: Some(false),
                    unknown: BTreeMap::new(),
                },
            ]),
            visible_misc_providers: Some(vec!["legacy".into()]),
            provider_plan_labels: Some(BTreeMap::from([("kilo".into(), "Pro".into())])),
            ..Default::default()
        };
        let view = settings.presentation();
        assert_eq!(view.display_mode, "used");
        assert_eq!(view.refresh_interval_seconds, 60);
        assert_eq!(view.menu_bar_color_basis, "actual");
        assert_eq!(view.selected_field_ids, vec!["codex.weekly"]);
        assert_eq!(view.visible_misc_providers, Some(vec!["kilo".into()]));
        assert_eq!(
            view.core_provider_order,
            vec!["codex", "claude", "gemini", "grok"]
        );
        assert_eq!(view.provider_plan_labels["kilo"], "Pro");
    }
}
