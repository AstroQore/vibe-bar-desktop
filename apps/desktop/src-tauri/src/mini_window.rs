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

pub fn install<R: Runtime>(app: &AppHandle<R>, root: DataRoot) -> tauri::Result<()> {
    let store = ClientStore::new(root.clone());
    let geometry = store.load_mini_window_geometry();
    app.manage(MiniState {
        root,
        geometry: Mutex::new(geometry.clone()),
    });
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?mini=1".into()))
        .title("Vibe Bar Mini")
        .inner_size(272.0, 190.0)
        .min_inner_size(272.0, 150.0)
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

/// Bounds for what the window will resize itself to.
///
/// The content decides the size — the layouts are different shapes, and native
/// sizes its window per layout too — but a render bug must not be able to
/// produce a window that covers the screen or one too small to see.
const MIN_SIZE: (f64, f64) = (200.0, 120.0);
const MAX_SIZE: (f64, f64) = (720.0, 640.0);

/// Resize to fit the layout the mini window is currently drawing.
///
/// A fixed 272x190 fitted the ring layout and nothing else: the ledger is 284
/// wide, four columns of tiles are wider still, and focus is 210 tall. All
/// three were being cropped by the window rather than by any decision.
pub fn resize_to_content<R: Runtime>(app: &AppHandle<R>, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !width.is_finite() || !height.is_finite() {
        return;
    }
    let _ = window.set_size(tauri::LogicalSize::new(
        width.clamp(MIN_SIZE.0, MAX_SIZE.0),
        height.clamp(MIN_SIZE.1, MAX_SIZE.1),
    ));
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
