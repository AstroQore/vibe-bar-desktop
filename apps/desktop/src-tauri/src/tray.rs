//! The tray item.
//!
//! The native app renders an attributed string with per-field colouring and
//! an optional two-row layout; a Tauri tray takes a plain title, so Desktop
//! renders the same fields as one line. Field selection and custom labels
//! come from the shared settings, so a user who configured the menu bar once
//! sees the same fields here.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use vibebar_desktop_core::refresh::QuotaView;
use vibebar_desktop_core::shared::settings::SharedSettings;

use crate::state::AppState;

const TRAY_ID: &str = "vibebar-desktop-tray";

pub fn install<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Vibe Bar Desktop", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Toggle Mini", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &mini, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .title(initial_title(state))
        .tooltip("Vibe Bar Desktop")
        // Left-click opens the window; the menu stays on right-click, so the
        // tray behaves the way the native item does.
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "refresh" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let view = app.state::<AppState>().engine().refresh().await;
                    update(&app, &view);
                    use tauri::Emitter;
                    let _ = app.emit(crate::QUOTA_EVENT, &view);
                });
            }
            "mini" => crate::toggle_mini(app),
            "quit" => {
                crate::persist_mini(app);
                app.exit(0)
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Refresh the tray title after a quota pass.
pub fn update<R: Runtime>(app: &AppHandle<R>, view: &QuotaView) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(title_for(app, view)));
    }
}

fn initial_title(state: &AppState) -> String {
    let view = state.engine().cached_view();
    render_title(&SharedSettings::load(state.data_root()), &view)
}

fn title_for<R: Runtime>(app: &AppHandle<R>, view: &QuotaView) -> String {
    let settings = SharedSettings::load(app.state::<AppState>().data_root());
    render_title(&settings, view)
}

/// `ChatGPT 45% · Claude 88%`, using the user's configured field order and
/// labels, and their remaining-vs-used preference.
fn render_title(settings: &SharedSettings, view: &QuotaView) -> String {
    render_title_at(settings, view, now_unix())
}

fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn render_title_at(settings: &SharedSettings, view: &QuotaView, now: f64) -> String {
    let (field_ids, labels) = settings.menu_bar_fields();
    let shows_remaining = settings.shows_remaining();

    let selected: Vec<String> = if field_ids.is_empty() {
        // No shared configuration: show what this build can actually fetch.
        vec![
            "codex.weekly".to_string(),
            "claude.weekly".to_string(),
            "claude.five_hour".to_string(),
        ]
    } else {
        field_ids
    };

    let mut parts: Vec<String> = Vec::new();
    for field_id in selected.iter().take(6) {
        let Some((tool_raw, bucket_id)) = field_id.split_once('.') else {
            continue;
        };
        // `QuotaEngine` already consolidates each provider to one card of
        // newest-believable windows, so in practice one candidate matches.
        // The rule is restated here because this function is the tray's whole
        // contract and is tested directly against raw views: a provider with
        // several cached accounts must resolve to the newest *believable*
        // reading, never the first match and never a future-stamped entry.
        let Some(bucket) = view
            .accounts
            .iter()
            .filter(|account| account.tool.raw_value() == tool_raw && account.error.is_none())
            .filter(|account| account.has_plausible_timestamp(now))
            .filter_map(|account| {
                account
                    .buckets
                    .iter()
                    .find(|bucket| bucket.id == bucket_id)
                    .map(|bucket| (account.queried_at, bucket))
            })
            .max_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, bucket)| bucket)
        else {
            continue;
        };
        let percent = if shows_remaining {
            bucket.remaining_percent()
        } else {
            bucket.used_percent
        };
        let label = labels
            .get(field_id)
            .cloned()
            .or_else(|| bucket.group_title.clone())
            .unwrap_or_else(|| default_label(tool_raw));
        parts.push(format!("{label} {}%", percent.round() as i64));
    }

    if parts.is_empty() {
        // Never render an empty tray: say why there is nothing to show.
        return if view.accounts.iter().all(|a| a.error.is_some()) && !view.accounts.is_empty() {
            "Vibe Bar · sign in".to_string()
        } else {
            "Vibe Bar".to_string()
        };
    }
    parts.join(" · ")
}

fn default_label(tool_raw: &str) -> String {
    vibebar_desktop_core::model::ToolType::from_raw(tool_raw)
        .map(|tool| tool.hierarchy().product.to_string())
        .unwrap_or_else(|| tool_raw.to_string())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibebar_desktop_core::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};
    use vibebar_desktop_core::shared::settings::MenuBarItem;

    fn account(id: &str, tool: ToolType, queried_at: f64, weekly_used: f64) -> AccountQuota {
        AccountQuota {
            account_id: id.into(),
            tool,
            buckets: vec![QuotaBucket::new(
                "weekly",
                "Weekly",
                "wk",
                weekly_used,
                None,
                Some(604_800),
                None,
            )],
            plan: None,
            queried_at,
            origin: QuotaOrigin::SharedCache,
            error: None,
        }
    }

    fn view(accounts: Vec<AccountQuota>) -> QuotaView {
        QuotaView {
            accounts,
            last_updated: None,
            has_shared_data: true,
            is_demo: false,
        }
    }

    fn settings_with(fields: &[&str], labels: &[(&str, &str)]) -> SharedSettings {
        SharedSettings {
            menu_bar_items: Some(vec![MenuBarItem {
                selected_field_ids: Some(fields.iter().map(|s| s.to_string()).collect()),
                custom_labels: Some(
                    labels
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                ),
                ..Default::default()
            }]),
            ..Default::default()
        }
    }

    const NOW: f64 = 1_788_040_000.0;

    #[test]
    fn newest_observation_wins_over_a_stale_signed_out_account() {
        // A signed-out account keeps its last cache entry forever; picking
        // "the first match" shows that instead of today's real number.
        let stale = account("stale-claude", ToolType::Claude, 1_768_000_000.0, 1.0);
        let fresh = account("web-claude", ToolType::Claude, 1_788_038_405.0, 34.0);

        let settings = settings_with(&["claude.weekly"], &[]);
        // Both orderings must agree; only recency may decide.
        assert_eq!(
            render_title_at(&settings, &view(vec![stale.clone(), fresh.clone()]), NOW),
            "Claude 66%"
        );
        assert_eq!(
            render_title_at(&settings, &view(vec![fresh, stale]), NOW),
            "Claude 66%"
        );
    }

    #[test]
    fn an_observation_from_the_future_never_wins() {
        // Found on a real data root: an entry stamped five months ahead,
        // which a naive "newest wins" rule shows forever.
        let future = account("bogus-claude", ToolType::Claude, NOW + 86_400.0 * 150.0, 1.0);
        let real = account("web-claude", ToolType::Claude, NOW - 600.0, 34.0);

        let settings = settings_with(&["claude.weekly"], &[]);
        assert_eq!(
            render_title_at(&settings, &view(vec![future.clone(), real.clone()]), NOW),
            "Claude 66%"
        );
        assert_eq!(
            render_title_at(&settings, &view(vec![real, future.clone()]), NOW),
            "Claude 66%"
        );
        // With nothing believable left, the field is dropped rather than
        // rendered from a broken timestamp.
        assert_eq!(render_title_at(&settings, &view(vec![future]), NOW), "Vibe Bar");
    }

    #[test]
    fn a_small_clock_skew_is_still_trusted() {
        let skewed = account("web-claude", ToolType::Claude, NOW + 60.0, 34.0);
        let settings = settings_with(&["claude.weekly"], &[]);
        assert_eq!(
            render_title_at(&settings, &view(vec![skewed]), NOW),
            "Claude 66%"
        );
    }

    #[test]
    fn honors_configured_order_labels_and_used_mode() {
        let settings = settings_with(
            &["claude.weekly", "codex.weekly"],
            &[("codex.weekly", "ChatGPT")],
        );
        let accounts = vec![
            account("a", ToolType::Codex, NOW - 10.0, 1.0),
            account("b", ToolType::Claude, NOW - 10.0, 34.0),
        ];
        // Field order comes from settings, not from provider rank.
        assert_eq!(
            render_title_at(&settings, &view(accounts.clone()), NOW),
            "Claude 66% · ChatGPT 99%"
        );

        let used_mode = SharedSettings {
            display_mode: Some("used".into()),
            ..settings
        };
        assert_eq!(
            render_title_at(&used_mode, &view(accounts), NOW),
            "Claude 34% · ChatGPT 1%"
        );
    }

    #[test]
    fn never_renders_an_empty_tray() {
        let settings = settings_with(&["claude.weekly"], &[]);
        assert_eq!(render_title_at(&settings, &view(vec![]), NOW), "Vibe Bar");

        let failed = AccountQuota {
            error: Some(vibebar_desktop_core::error::QuotaError::NoCredential),
            buckets: Vec::new(),
            ..account("x", ToolType::Claude, NOW - 1.0, 0.0)
        };
        assert_eq!(render_title_at(&settings, &view(vec![failed]), NOW), "Vibe Bar · sign in");
    }
}
