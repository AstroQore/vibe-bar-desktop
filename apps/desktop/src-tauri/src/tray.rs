//! The tray item.
//!
//! An icon and a menu, and nothing else. The readout that names quota fields
//! in a status item is the macOS menu bar, which is the native app's surface:
//! Windows and Linux have no customisable menu bar at all, and a Tauri tray
//! title is drawn on none of them the way native draws it. So this client
//! reads no menu-bar setting and renders no title — the two clients cannot
//! disagree about a strip only one of them has.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

use crate::state::AppState;

const TRAY_ID: &str = "vibebar-desktop-tray";

pub fn install(app: &AppHandle, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, state.pending_update_summary())?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
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
