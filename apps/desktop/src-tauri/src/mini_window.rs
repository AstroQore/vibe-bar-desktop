//! One Desktop-owned, regular-bar Mini quota window.

use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use vibebar_desktop_core::client_store::{ClientStore, MiniWindowGeometry};
use vibebar_desktop_core::paths::DataRoot;

const LABEL: &str = "mini";

struct MiniState {
    root: DataRoot,
    geometry: Mutex<MiniWindowGeometry>,
}

/// Smallest the window will be made, so a render that measures nothing cannot
/// produce a window too small to find. There is deliberately no fixed upper
/// bound: the ceiling is the monitor, the way it is in the native client. A
/// permitted twelve-field regular layout is wider than any round number worth
/// picking, and cropping it to keep the window tidy would hide quotas the user
/// asked for.
const MIN_SIZE: (f64, f64) = (200.0, 120.0);
/// Native keeps this much between the window and the screen edge.
const SCREEN_MARGIN: f64 = 8.0;

pub fn install<R: Runtime>(app: &AppHandle<R>, root: DataRoot) -> tauri::Result<()> {
    let store = ClientStore::new(root.clone());
    let geometry = store.load_mini_window_geometry();
    app.manage(MiniState {
        root,
        geometry: Mutex::new(geometry.clone()),
    });
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?mini=1".into()))
        .title("Vibe Bar Mini")
        // A starting size only: the first render measures its layout and the
        // window is fitted to it. The minimum is the one `resize_to_content`
        // clamps to — two constants disagreeing about the same thing meant the
        // platform floor silently won, and the smallest layouts stayed padded
        // to a width nothing had asked for.
        .inner_size(272.0, 190.0)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()?;

    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(position) => update_position(&handle, *position),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle);
        }
        _ => {}
    });
    if geometry.was_open {
        show(app);
    }
    Ok(())
}

pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide(app);
    } else {
        show(app);
    }
}

/// Fit the window to the layout it is drawing.
///
/// A fixed 272x190 fitted the ring layout and nothing else: the ledger is 284
/// wide, four columns of tiles are wider still, and focus is 210 tall, so all
/// three were cropped by the window rather than by any decision.
pub fn resize_to_content<R: Runtime>(app: &AppHandle<R>, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !width.is_finite() || !height.is_finite() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    // The work area, not the display: `size()` is the whole panel, so a
    // ceiling taken from it puts the bottom of the window under the Dock or
    // the taskbar and the top under the menu bar. This is what the native
    // client's `NSScreen.visibleFrame` gives it.
    let (max_width, max_height) = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let area = monitor.work_area().size.to_logical::<f64>(scale);
            (
                (area.width - 2.0 * SCREEN_MARGIN).max(MIN_SIZE.0),
                (area.height - 2.0 * SCREEN_MARGIN).max(MIN_SIZE.1),
            )
        })
        .unwrap_or((f64::MAX, f64::MAX));

    let _ = window.set_size(tauri::LogicalSize::new(
        width.clamp(MIN_SIZE.0, max_width),
        height.clamp(MIN_SIZE.1, max_height),
    ));
    keep_on_screen(&window);
}

/// Pull the window back onto its monitor after it has changed size.
///
/// `set_size` keeps the origin, so a window parked against the right or bottom
/// edge grows off the screen — and what leaves is the area just added, which
/// is the quota the user changed layout to see.
fn keep_on_screen<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    let (Ok(Some(monitor)), Ok(position), Ok(size)) = (
        window.current_monitor(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        return;
    };
    // Same reason as the ceiling above: the usable area, so the window is not
    // tucked under the menu bar or the Dock.
    let area = monitor.work_area();
    let screen = area.position;
    let bounds = area.size;
    let margin = (SCREEN_MARGIN * monitor.scale_factor()).round() as i32;
    // Native's order: clamp to the far edge first, then to the near one, so a
    // window larger than the screen ends up against the top-left rather than
    // pushed off the opposite side.
    let max_x = screen
        .x
        .saturating_add(bounds.width as i32)
        .saturating_sub(size.width as i32)
        .saturating_sub(margin);
    let max_y = screen
        .y
        .saturating_add(bounds.height as i32)
        .saturating_sub(size.height as i32)
        .saturating_sub(margin);
    let x = position.x.min(max_x).max(screen.x.saturating_add(margin));
    let y = position.y.min(max_y).max(screen.y.saturating_add(margin));
    if x != position.x || y != position.y {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn persist<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<MiniState>();
    let Ok(geometry) = state.geometry.lock() else {
        return;
    };
    let _ = ClientStore::new(state.root.clone()).save_mini_window_geometry(&geometry);
}

fn show<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    restore_or_center(&window, app);
    let _ = window.show();
    let _ = app.emit(crate::MINI_SHOWN_EVENT, ());
    let state = app.state::<MiniState>();
    if let Ok(mut geometry) = state.geometry.lock() {
        geometry.was_open = true;
    };
}

pub(crate) fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
    let state = app.state::<MiniState>();
    if let Ok(mut geometry) = state.geometry.lock() {
        geometry.was_open = false;
    };
    persist(app);
}

fn update_position<R: Runtime>(app: &AppHandle<R>, position: PhysicalPosition<i32>) {
    let state = app.state::<MiniState>();
    if let Ok(mut geometry) = state.geometry.lock() {
        geometry.x = position.x;
        geometry.y = position.y;
    };
}

fn restore_or_center<R: Runtime>(window: &tauri::WebviewWindow<R>, app: &AppHandle<R>) {
    let state = app.state::<MiniState>();
    let geometry = state
        .geometry
        .lock()
        .map(|geometry| geometry.clone())
        .unwrap_or_default();
    let visible = window.available_monitors().ok().is_some_and(|monitors| {
        monitors.into_iter().any(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            geometry.x >= position.x
                && geometry.y >= position.y
                && geometry.x < position.x.saturating_add(size.width as i32)
                && geometry.y < position.y.saturating_add(size.height as i32)
        })
    });
    if visible {
        let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
    } else {
        let _ = window.center();
    }
}
