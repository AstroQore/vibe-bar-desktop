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

use crate::state::AppState;

const TRAY_ID: &str = "vibebar-desktop-tray";

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Vibe Bar Desktop", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Toggle Mini", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &mini, &separator, &quit])?;

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
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
