//! The tray popover: native's `NSPopover` as a borderless window.
//!
//! Native shows a transient popover anchored to the status item. Tauri has no
//! popover, so this is a decorationless always-on-top window placed just
//! under the tray icon, hidden the moment it loses focus — which is what
//! `.transient` means. The arrow native draws on the popover's edge is the
//! one thing this cannot reproduce without private APIs; the content inside
//! is the same shell at the same width.
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, Rect, Runtime, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

pub const LABEL: &str = "popover";
/// Native's regular-density overview width. The content reports its real
/// size once mounted and the window follows it.
const INITIAL: (f64, f64) = (960.0, 640.0);
const MIN_SIZE: (f64, f64) = (360.0, 200.0);
const SCREEN_MARGIN: f64 = 8.0;
/// Points between the menu bar's bottom edge and the popover's top edge.
const ANCHOR_GAP: f64 = 6.0;

struct PopoverState {
    /// Set while a programmatic show is in flight, so the blur that a show
    /// can briefly cause on some platforms does not hide it again.
    showing: AtomicBool,
}

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    app.manage(PopoverState { showing: AtomicBool::new(false) });
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?popover=1".into()))
        .title("Vibe Bar")
        .inner_size(INITIAL.0, INITIAL.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        // Native's popover is drawn on the system's popover material; the
        // page paints nothing behind its cards, and the window is transparent
        // so the material shows through. Requires `macOSPrivateApi` for the
        // webview to stop painting its own white.
        .transparent(true)
        .build()?;
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        let _ = apply_vibrancy(&window, NSVisualEffectMaterial::Popover, None, Some(12.0));
    }
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        // Transient: gone when the user looks elsewhere.
        WindowEvent::Focused(false) => {
            let state = handle.state::<PopoverState>();
            if !state.showing.load(Ordering::Acquire) {
                hide(&handle);
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle);
        }
        _ => {}
    });
    Ok(())
}

/// Show under the tray icon, or hide if it is already up — a click on the
/// status item toggles, as native's does.
pub fn toggle_at<R: Runtime>(app: &AppHandle<R>, anchor: Rect) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide(app);
    } else {
        show_at(app, anchor);
    }
}

pub fn show_at<R: Runtime>(app: &AppHandle<R>, anchor: Rect) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let state = app.state::<PopoverState>();
    state.showing.store(true, Ordering::Release);
    place(&window, anchor);
    let _ = window.show();
    let _ = window.set_focus();
    // Any focus churn from the show itself has settled by now.
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        handle.state::<PopoverState>().showing.store(false, Ordering::Release);
    });
}

/// Native's `VIBEBAR_DEMO_SURFACE=popover`: present the popover with no tray
/// click to anchor it, centred near the top of the primary display. Demo mode
/// and screenshot scripts use it; nothing else does.
pub fn show_centered<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let state = app.state::<PopoverState>();
    state.showing.store(true, Ordering::Release);
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let origin = area.position.to_logical::<f64>(scale);
        let bounds = area.size.to_logical::<f64>(scale);
        let size = window
            .outer_size()
            .map(|s| s.to_logical::<f64>(scale))
            .unwrap_or_else(|_| LogicalSize::new(INITIAL.0, INITIAL.1));
        let x = origin.x + (bounds.width - size.width) / 2.0;
        let y = origin.y + 40.0;
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        handle.state::<PopoverState>().showing.store(false, Ordering::Release);
    });
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// Centre the popover on the tray icon, just below the menu bar, and keep
/// every edge inside the monitor's work area.
fn place<R: Runtime>(window: &tauri::WebviewWindow<R>, anchor: Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let anchor_pos = anchor.position.to_logical::<f64>(scale);
    let anchor_size = anchor.size.to_logical::<f64>(scale);
    let size = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale))
        .unwrap_or_else(|_| LogicalSize::new(INITIAL.0, INITIAL.1));
    let mut x = anchor_pos.x + anchor_size.width / 2.0 - size.width / 2.0;
    let mut y = anchor_pos.y + anchor_size.height + ANCHOR_GAP;
    if let Ok(Some(monitor)) = window.current_monitor().or_else(|_| window.primary_monitor()) {
        let area = monitor.work_area();
        let origin = area.position.to_logical::<f64>(monitor.scale_factor());
        let bounds = area.size.to_logical::<f64>(monitor.scale_factor());
        let max_x = origin.x + bounds.width - size.width - SCREEN_MARGIN;
        let max_y = origin.y + bounds.height - size.height - SCREEN_MARGIN;
        x = x.min(max_x).max(origin.x + SCREEN_MARGIN);
        y = y.min(max_y).max(origin.y + SCREEN_MARGIN);
    }
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// The content's measured size, from the page. Same clamp as the mini window.
pub fn resize_to_content<R: Runtime>(app: &AppHandle<R>, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !width.is_finite() || !height.is_finite() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
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
    let before = window.outer_position().ok();
    let _ = window.set_size(LogicalSize::new(
        width.clamp(MIN_SIZE.0, max_width),
        height.clamp(MIN_SIZE.1, max_height),
    ));
    // Growing must not push the bottom edge off the screen: keep the top
    // anchored and pull the window up only if it no longer fits.
    if let (Some(before), Ok(Some(monitor)), Ok(size)) = (before, window.current_monitor(), window.outer_size()) {
        let area = monitor.work_area();
        let margin = (SCREEN_MARGIN * monitor.scale_factor()).round() as i32;
        let max_y = area.position.y + area.size.height as i32 - size.height as i32 - margin;
        let y = before.y.min(max_y).max(area.position.y + margin);
        if y != before.y {
            let _ = window.set_position(PhysicalPosition::new(before.x, y));
        }
    }
}
