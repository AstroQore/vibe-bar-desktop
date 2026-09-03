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

/// The bundle id Control Center tracks this app by.
pub const BUNDLE_ID: &str = "com.astroqore.VibeBarDesktop";

/// Drop the status item and build a fresh one — the native
/// `reregisterMenuBarItem`, run after an allow-list repair.
pub fn reregister(app: &AppHandle) -> tauri::Result<()> {
    let _ = app.remove_tray_by_id(TRAY_ID);
    let state = app.state::<AppState>();
    install(app, &state)?;
    let view = state.engine().cached_view();
    update(app, &view);
    Ok(())
}

pub fn install(app: &AppHandle, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, state.pending_update_summary())?;

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
            id if id.starts_with("update:") => {
                // Installing replaces the running app; the Settings page
                // offers the same with a confirmation step, the menu is the
                // shortcut for someone who already decided.
                if let Ok(id) = id["update:".len()..].parse::<u64>() {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::install_pending_update(&app, id).await;
                    });
                }
            }
            "quit" => {
                crate::persist_mini(app);
                app.exit(0)
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // A left click toggles the popover under the icon, as native's
            // status item does. The menu stays on the right button.
            if let TrayIconEvent::Click {
                rect,
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                crate::popover::toggle_at(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

/// Refresh the tray title after a quota pass.
/// The tray menu. With an update found, an item to install it sits at the
/// top — the way Sparkle's "Update to X…" does in the native app's menu.
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    update: Option<crate::commands::PendingUpdate>,
) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", "Open Vibe Bar Desktop", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Toggle Mini", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    match update {
        Some(pending) => {
            // The item carries the id of the find it names, so a click
            // installs that version and not whatever a later check holds.
            let update = MenuItem::with_id(
                app,
                format!("update:{}", pending.id),
                format!("Update to {}…", pending.version),
                true,
                None::<&str>,
            )?;
            let after = PredefinedMenuItem::separator(app)?;
            Menu::with_items(
                app,
                &[&update, &after, &show, &refresh, &mini, &separator, &quit],
            )
        }
        None => Menu::with_items(app, &[&show, &refresh, &mini, &separator, &quit]),
    }
}

/// Rebuild the menu after a check: the update item appears or goes away.
pub fn refresh_menu(app: &AppHandle) {
    let update = app.state::<AppState>().pending_update_summary();
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, update)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn update<R: Runtime>(app: &AppHandle<R>, view: &QuotaView) {
    // "Show in menu bar" is the shared item's own switch; the tray follows it.
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let state = app.state::<AppState>();
        let visible = SharedSettings::load(state.data_root()).menu_bar_item_visible();
        let _ = tray.set_visible(visible);
    }
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
    // Native draws a logo per field instead of its name when this is off. A
    // tray title is one plain string with no room for six logos, so the
    // honest equivalent is to drop the words and keep the numbers — which is
    // also what keeps the item narrow enough to stay on the menu bar.
    let names_fields = settings.menu_bar_shows_title();

    let selected: Vec<String> = if field_ids.is_empty() {
        // No shared configuration: show what this build can actually fetch.
        let mut defaults = vec![
            "codex.weekly".to_string(),
            "claude.weekly".to_string(),
            "claude.five_hour".to_string(),
        ];
        for account in &view.accounts {
            let Some(bucket) = account.buckets.first() else {
                continue;
            };
            let field = format!("{}.{}", account.tool.raw_value(), bucket.id);
            if !defaults.contains(&field) {
                defaults.push(field);
            }
        }
        defaults
    } else {
        field_ids
    };

    let mut parts: Vec<String> = Vec::new();
    for field_id in &selected {
        if parts.len() >= 6 {
            break;
        }
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
            .filter(|account| account.tool.raw_value() == tool_raw)
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
        let percent = format!("{}%", percent.round() as i64);
        parts.push(if names_fields {
            let label = labels
                .get(field_id)
                .cloned()
                .or_else(|| bucket.group_title.clone())
                .unwrap_or_else(|| default_label(tool_raw));
            format!("{label} {percent}")
        } else {
            percent
        });
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

/// Show the main window because the user asked for it. Before the page has
/// mounted this is a blank vibrancy sheet, which is the honest answer to a
/// click: the shell is up, the page is still coming. (On a Mac whose
/// CoreAudio HAL stalls, WebKit's first paint can take fifteen seconds.)
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.show().is_err() {
        return;
    }
    let _ = window.set_focus();
    // A first launch counts as seen only now that the window is on screen.
    let state = app.state::<AppState>();
    if state.take_first_run_mark() {
        let store = vibebar_desktop_core::client_store::ClientStore::new(state.data_root().clone());
        let _ = store.mark_first_run_complete();
    }
}

/// Show the main window at startup — once its page has mounted, so it never
/// opens white. A request before that is parked and honoured by
/// `frontend_ready`; if the page never reports, the load watchdog steps in.
pub(crate) fn show_main_window_when_ready<R: Runtime>(app: &AppHandle<R>) {
    if app.state::<AppState>().park_show_unless_ready() {
        show_main_window(app);
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

    fn unnamed_fields_settings(fields: &[&str], labels: &[(&str, &str)]) -> SharedSettings {
        let mut settings = settings_with(fields, labels);
        let item = settings
            .menu_bar_items
            .as_mut()
            .unwrap()
            .first_mut()
            .unwrap();
        item.show_title = Some(false);
        settings
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
        let future = account(
            "bogus-claude",
            ToolType::Claude,
            NOW + 86_400.0 * 150.0,
            1.0,
        );
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
        assert_eq!(
            render_title_at(&settings, &view(vec![future]), NOW),
            "Vibe Bar"
        );
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
        assert_eq!(
            render_title_at(&settings, &view(vec![failed]), NOW),
            "Vibe Bar · sign in"
        );
    }

    #[test]
    fn fallback_uses_available_adapter_buckets_and_keeps_cached_auth_values() {
        let mut zai = account("zai", ToolType::Zai, NOW - 10.0, 31.0);
        zai.buckets[0].id = "zai.tokens".to_string();
        assert_eq!(
            render_title_at(&SharedSettings::default(), &view(vec![zai]), NOW),
            "GLM 69%"
        );

        let mut cached = account("codex", ToolType::Codex, NOW - 10.0, 24.0);
        cached.origin = QuotaOrigin::DesktopCache;
        cached.error = Some(vibebar_desktop_core::error::QuotaError::NeedsLogin);
        assert_eq!(
            render_title_at(&SharedSettings::default(), &view(vec![cached]), NOW),
            "ChatGPT Agentic 76%"
        );

        let accounts = [
            (ToolType::Zai, "zai.tokens"),
            (ToolType::Minimax, "minimax.coding"),
            (ToolType::OpenRouter, "openrouter.credits"),
            (ToolType::Warp, "warp.credits"),
        ]
        .into_iter()
        .map(|(tool, bucket_id)| {
            let mut account = account(tool.raw_value(), tool, NOW - 10.0, 25.0);
            account.buckets[0].id = bucket_id.to_string();
            account
        })
        .collect();
        let title = render_title_at(&SharedSettings::default(), &view(accounts), NOW);
        assert_eq!(title.split(" · ").count(), 4, "{title}");
    }

    #[test]
    fn an_item_without_titles_drops_the_words_and_keeps_the_numbers() {
        // The configuration on the machine this was found on: six fields with
        // `showTitle` off, where native draws a logo per field and measures
        // 126 points. Rendering the labels anyway made it 518 points wide —
        // wide enough that macOS pushed it behind the app menus and it could
        // not be clicked at all.
        let fields = [
            "codex.weekly",
            "claude.weekly",
            "grok.weekly",
            "cursor.models",
            "gemini.weekly",
            "antigravity.gemini_weekly",
        ];
        let labels = [
            ("codex.weekly", "ChatGPT"),
            ("claude.weekly", "Claude"),
            ("grok.weekly", "Grok"),
            ("cursor.models", "Cursor"),
            ("gemini.weekly", "Gemini"),
            ("antigravity.gemini_weekly", "A W"),
        ];
        let view = view(vec![
            account("a", ToolType::Codex, 100.0, 80.0),
            account("b", ToolType::Claude, 100.0, 34.0),
        ]);
        let compact = render_title_at(&unnamed_fields_settings(&fields, &labels), &view, 100.0);
        assert!(!compact.contains("ChatGPT"), "{compact}");
        assert!(!compact.contains("A W"), "{compact}");
        assert!(
            compact.contains("20%") && compact.contains("66%"),
            "{compact}"
        );

        // And the labelled form is still the labelled form when nothing asked
        // for compact.
        let labelled = render_title_at(&settings_with(&fields, &labels), &view, 100.0);
        assert!(labelled.contains("ChatGPT 20%"), "{labelled}");
    }

    #[test]
    fn an_empty_selection_still_says_something_clickable() {
        let view = view(vec![]);
        let title = render_title_at(&settings_with(&[], &[]), &view, 100.0);
        assert!(
            !title.is_empty(),
            "an empty tray title is an invisible item"
        );
    }
}
