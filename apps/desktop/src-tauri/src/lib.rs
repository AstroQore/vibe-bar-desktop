//! Vibe Bar Desktop — Tauri shell.
//!
//! Thin by design: every decision that isn't about windows, trays, or IPC
//! lives in `vibebar-desktop-core`, which knows nothing about Tauri and is
//! testable on all three platforms without a GUI.

mod commands;
mod native_app;
mod state;
mod tray;

use std::time::Duration;

use state::AppState;
use tauri::{Emitter, Manager};

/// Emitted whenever a refresh completes, carrying the full `QuotaView`.
pub const QUOTA_EVENT: &str = "vibebar://quota-updated";

/// Run the Desktop-owned, read-only MCP server without starting Tauri.
pub fn run_mcp_stdio() -> i32 {
    vibebar_desktop_core::mcp::run_stdio()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch focuses the running window instead of starting a
        // rival tray icon and refresh loop against the same data root.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::quota_view,
            commands::presentation_settings,
            commands::status_snapshot,
            commands::refresh_status,
            commands::cost_view,
            commands::refresh_cost,
            commands::refresh_quota,
            commands::session_list,
            commands::session_search,
            commands::session_transcript,
            commands::app_info,
        ])
        .setup(|app| {
            let state = AppState::new();
            tray::install(app.handle(), &state)?;
            app.manage(state);
            spawn_refresh_loop(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Vibe Bar Desktop");
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
