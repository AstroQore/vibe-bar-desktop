//! Vibe Bar Desktop — Tauri shell.
//!
//! Thin by design: every decision that isn't about windows, trays, or IPC
//! lives in `vibebar-desktop-core`, which knows nothing about Tauri and is
//! testable on all three platforms without a GUI.

mod commands;
mod mini_window;
mod native_app;
mod state;
mod tray;

use std::time::Duration;

use state::AppState;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

/// Emitted whenever a refresh completes, carrying the full `QuotaView`.
pub const QUOTA_EVENT: &str = "vibebar://quota-updated";
pub const MINI_SHOWN_EVENT: &str = "vibebar://mini-shown";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // A second launch focuses the running window instead of starting a
        // rival tray icon and refresh loop against the same data root.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::quota_view,
            commands::presentation_settings,
            commands::refresh_quota,
            commands::hide_mini,
            commands::session_list,
            commands::session_search,
            commands::session_transcript,
            commands::app_info,
        ])
        .setup(|app| {
            let state = AppState::new();
            mini_window::install(app.handle(), state.data_root().clone())?;
            tray::install(app.handle(), &state)?;
            app.manage(state);
            spawn_refresh_loop(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vibe Bar Desktop");
    app.run(|app, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            mini_window::persist(app);
        }
    });
}

pub fn toggle_mini<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    mini_window::toggle(app);
}

pub fn persist_mini<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    mini_window::persist(app);
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
            tokio::time::sleep(interval.max(Duration::from_secs(60))).await;
        }
    });
}
