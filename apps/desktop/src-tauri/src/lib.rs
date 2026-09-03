//! Vibe Bar Desktop — Tauri shell.
//!
//! Thin by design: every decision that isn't about windows, trays, or IPC
//! lives in `vibebar-desktop-core`, which knows nothing about Tauri and is
//! testable on all three platforms without a GUI.

mod commands;
mod menu_bar_health;
mod mini_window;
mod native_app;
mod popover;
mod state;
mod tray;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use state::AppState;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use vibebar_desktop_core::client_store::{startup_action, ClientStore, StartupAction};

/// Emitted whenever a refresh completes, carrying the full `QuotaView`.
pub const QUOTA_EVENT: &str = "vibebar://quota-updated";
/// The shared settings file changed underneath this process.
pub const SETTINGS_EVENT: &str = "vibebar://settings-changed";
pub const MINI_SHOWN_EVENT: &str = "vibebar://mini-shown";

/// Run the Desktop-owned, read-only MCP server without starting Tauri.
pub fn run_mcp_stdio() -> i32 {
    vibebar_desktop_core::mcp::run_stdio()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tray_available = Arc::new(AtomicBool::new(false));
    let close_tray_available = Arc::clone(&tray_available);
    let setup_tray_available = Arc::clone(&tray_available);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        // A second launch focuses the running window instead of starting a
        // rival tray icon and refresh loop against the same data root.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // Closing the one user-facing window leaves the tray refresh loop
        // alive. Explicit tray Quit still calls `app.exit(0)` and terminates
        // the process rather than requesting this window close.
        .on_window_event(move |window, event| {
            if window.label() == "main" && close_tray_available.load(Ordering::Acquire) {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::quota_view,
            commands::presentation_settings,
            commands::save_shared_settings,
            commands::shared_settings_raw,
            commands::status_snapshot,
            commands::refresh_status,
            commands::cost_view,
            commands::refresh_cost,
            commands::usage_stats,
            commands::session_listing,
            commands::open_in_terminal,
            commands::reveal_path,
            commands::autostart_enabled,
            commands::set_autostart,
            commands::pricing_effective,
            commands::open_url,
            commands::frontend_log,
            commands::frontend_ready,
            commands::menu_bar_health,
            commands::menu_bar_check_now,
            commands::menu_bar_repair,
            commands::refresh_quota,
            commands::hide_mini,
            commands::toggle_mini,
            commands::show_main_window,
            commands::resize_popover,
            commands::hide_popover,
            commands::resize_mini,
            commands::check_for_update,
            commands::pending_update,
            commands::install_update,
            commands::session_list,
            commands::session_search,
            commands::session_transcript,
            commands::quota_cycles,
            commands::app_info,
            commands::skills_inventory,
        ])
        // Every window's page learns whether it sits on a vibrant material,
        // so the sheets can lower their fills and let the desktop through.
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished
                && cfg!(target_os = "macos")
            {
                let _ = webview.eval("document.documentElement.classList.add('vibrant')");
            }
        })
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_glass(&window, GlassSurface::Window);
            }
            let state = AppState::new();
            // A forecast needs history and a fresh install has none. On a Mac
            // with the native app, adopt its observations once so the first
            // launch can already say something instead of waiting two weeks.
            // Read-only, optional, and skipped once this client has its own.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            vibebar_desktop_core::forecast::seed_from_native_once(state.data_root(), now);
            mini_window::install(app.handle(), state.data_root().clone())?;
            // Tray failure deliberately does not abort setup: without a tray,
            // hiding the only window would leave the user no way back in.
            let tray_installed = tray::install(app.handle(), &state).is_ok();
            // The popover exists only where a tray exists to anchor it.
            if tray_installed {
                let _ = popover::install(app.handle());
                // Native's demo surface switch, for the screenshot scripts: the
                // popover cannot be clicked open by a script, so demo mode
                // presents it on request.
                if state.data_root().is_demo()
                    && std::env::var("VIBEBAR_DEMO_SURFACE")
                        .map(|s| s.starts_with("popover"))
                        .unwrap_or(false)
                {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                        popover::show_centered(&handle);
                    });
                }
            }
            setup_tray_available.store(tray_installed, Ordering::Release);
            let store = ClientStore::new(state.data_root().clone());
            let action = startup_action(
                state.data_root().is_demo(),
                tray_installed,
                store.first_run_state(),
            );
            app.manage(state);
            apply_startup_action(app.handle(), action);
            spawn_refresh_loop(app.handle().clone());
            spawn_load_watchdog(app.handle().clone());
            spawn_update_check_loop(app.handle().clone());
            spawn_settings_watch(app.handle().clone());
            app.manage(menu_bar_health::Watchdog::default());
            menu_bar_health::spawn(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vibe Bar Desktop");
    app.run(|app, event| match event {
        RunEvent::ExitRequested { .. } => mini_window::persist(app),
        // Opening an app that is already running is how anyone gets back to a
        // window they cannot otherwise reach, and it is the only way back when
        // the tray item is not clickable — macOS hides an item that outgrows
        // the menu bar, and without this the app is running, has no window,
        // and cannot be reached by any means at all.
        //
        // Not gated on `has_visible_windows`: the mini window is a window and
        // is restored on launch whenever it was open, so that flag is true in
        // exactly the state this exists to rescue — mini floating, main
        // hidden, tray unreachable. Showing an already-visible main window
        // just focuses it.
        //
        // macOS-only: `Reopen` is the Dock/`open -a` event and the enum
        // variant does not exist elsewhere, so this arm has to be compiled
        // away rather than merely unreachable.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => tray::show_main_window(app),
        _ => {}
    });
}

pub fn toggle_mini<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    mini_window::toggle(app);
}

pub fn persist_mini<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    mini_window::persist(app);
}

fn apply_startup_action<R: tauri::Runtime>(app: &tauri::AppHandle<R>, action: StartupAction) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match action {
        StartupAction::HideToTray => {
            if window.hide().is_err() {
                tray::show_main_window_when_ready(app);
            }
        }
        StartupAction::Show => {
            tray::show_main_window_when_ready(app);
        }
        StartupAction::ShowAndMarkFirstRunComplete => {
            // Recorded by the show itself, once the window is actually up.
            app.state::<AppState>().defer_first_run_mark();
            tray::show_main_window_when_ready(app);
        }
    }
}

/// Watch the shared settings for the other client's changes.
///
/// `settings.json` is written by the native app as well, and every surface
/// here reads it fresh — so the only thing this has to do is notice, and say
/// when a setting the user chose *here* now holds someone else's value.
///
/// Polled rather than watched: the file is a few tens of kilobytes, the
/// cadence is slower than a person can retype a preference, and a poll is the
/// same three lines on every platform Desktop targets.
fn spawn_settings_watch(app: tauri::AppHandle) {
    const CADENCE: Duration = Duration::from_secs(2);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(CADENCE).await;
            let changed = {
                let state = app.state::<AppState>();
                let Ok(mut writer) = state.settings().lock() else {
                    continue;
                };
                writer.poll()
            };
            if let Some(change) = changed {
                let _ = app.emit(
                    SETTINGS_EVENT,
                    change.replaced.map(|replaced| replaced.replaced_keys),
                );
            }
        }
    });
}

/// Background refresh: one immediate pass, then on the cadence the shared
/// settings define. Each pass updates the tray and pushes the new view to any
/// open window.
fn spawn_refresh_loop(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let interval = {
                let state = app.state::<AppState>();
                let view = state.engine().refresh().await;
                tray::update(&app, &view);
                let _ = app.emit(QUOTA_EVENT, &view);
                state.engine().refresh_interval()
            };
            // Not a plain sleep: a cadence saved in Settings is meant to take
            // effect now, not after the wait it was meant to replace. Going
            // from an hour to a minute would otherwise leave this refreshing
            // hourly for the rest of the hour.
            let cadence_changed = app.state::<AppState>().cadence_changed();
            tokio::select! {
                _ = tokio::time::sleep(interval.max(Duration::from_secs(60))) => {}
                _ = cadence_changed.notified() => {}
            }
        }
    });
}

/// The material under a window: the glass design language, with the
/// system's own materials. A no-op off macOS, where the windows stay opaque.
#[derive(Clone, Copy)]
pub enum GlassSurface {
    /// The main window: the sidebar material under the whole surface.
    Window,
    /// A floating panel: the popover material, rounded like one.
    Panel,
}

pub fn apply_glass<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, surface: GlassSurface) {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        let (material, radius) = match surface {
            GlassSurface::Window => (NSVisualEffectMaterial::Sidebar, None),
            GlassSurface::Panel => (NSVisualEffectMaterial::Popover, Some(14.0)),
        };
        let _ = apply_vibrancy(window, material, None, radius);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, surface);
    }
}

/// A page that has not mounted thirty seconds after launch is loaded again
/// once — as a new generation, so a report from the old page cannot be
/// taken for the new one — and, if that does not help either, a show that
/// was parked goes through anyway so the request is not lost. Thirty, not
/// six: on a Mac whose CoreAudio HAL stalls, WebKit's GPU process blocks for
/// fifteen seconds before any page can paint, and a reload inside that
/// window only restarts the wait.
fn spawn_load_watchdog(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let Some(generation) = app.state::<state::AppState>().begin_reload_unless_ready() else {
            return;
        };
        eprintln!("[watchdog] main page not mounted after 30 s; loading generation {generation}");
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(mut url) = window.url() {
                url.set_query(Some(&format!("boot={generation}")));
                let _ = window.navigate(url);
            }
        }
        tokio::time::sleep(Duration::from_secs(20)).await;
        if app.state::<state::AppState>().give_up_waiting() {
            eprintln!("[watchdog] main page still not mounted; showing the parked window anyway");
            tray::show_main_window(&app);
        }
    });
}

/// What a page and the tray learn when the scheduled check finds an update.
pub const UPDATE_EVENT: &str = "vibebar://update-available";

/// The scheduled update check, as the native app's Sparkle does daily: a
/// first look shortly after launch — unless one ran within the last twenty
/// hours, so relaunching all day does not hammer the feed — then every day.
/// A find is held for the Settings page and the tray to offer; nothing is
/// downloaded or installed until the person asks. Demo mode never checks.
fn spawn_update_check_loop(app: tauri::AppHandle) {
    const FIRST_LOOK: Duration = Duration::from_secs(90);
    const DAILY: Duration = Duration::from_secs(24 * 60 * 60);
    const RECENT: f64 = 20.0 * 60.0 * 60.0;
    tauri::async_runtime::spawn(async move {
        if app.state::<AppState>().data_root().is_demo() {
            return;
        }
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs_f64())
                .unwrap_or(0.0);
            let store = ClientStore::new(app.state::<AppState>().data_root().clone());
            let recently = store
                .last_update_check_at()
                .is_some_and(|at| at <= now && now - at < RECENT);
            if !recently {
                let found = {
                    let state = app.state::<AppState>();
                    commands::perform_update_check(&app, &state).await
                };
                let _ = store.record_update_check(now);
                match found {
                    Ok(found) => {
                        if let Some(update) = &found {
                            eprintln!("[update] {} is available", update.version);
                        }
                        announce_update(&app, found.as_ref());
                    }
                    Err(error) => eprintln!("[update] scheduled check failed: {error}"),
                }
                tokio::time::sleep(DAILY).await;
            } else {
                // Sleep only until the last check is a day old, so a launch
                // near the end of the window keeps the daily cadence rather
                // than adding a whole day to it.
                let due = store
                    .last_update_check_at()
                    .map(|at| (at + DAILY.as_secs_f64() - now).max(60.0))
                    .unwrap_or(DAILY.as_secs_f64());
                tokio::time::sleep(Duration::from_secs_f64(due.min(DAILY.as_secs_f64()))).await;
            }
        }
    });
}

/// After any check — scheduled or from Settings — the tray menu shows or
/// drops its "Update to X…" item and every open page hears the result,
/// `null` included, so a page offering a withdrawn update stops offering it.
pub(crate) fn announce_update(app: &tauri::AppHandle, found: Option<&commands::PendingUpdate>) {
    tray::refresh_menu(app);
    let _ = app.emit(UPDATE_EVENT, found);
}

/// Install the find with `id` and restart into it; the tray's "Update to
/// X…" item, which names that id. A find a later check has replaced is not
/// installed — the menu is rebuilt instead. A failure keeps the find so the
/// next try can use it.
pub(crate) async fn install_pending_update(app: &tauri::AppHandle, id: u64) {
    let state = app.state::<AppState>();
    let Some(update) = state.take_update(id) else {
        eprintln!("[update] the menu's find was overtaken by a newer check; rebuilding the menu");
        tray::refresh_menu(app);
        return;
    };
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(error) => {
            eprintln!("[update] install failed: {error}");
            state.restore_update(id, update);
        }
    }
}
