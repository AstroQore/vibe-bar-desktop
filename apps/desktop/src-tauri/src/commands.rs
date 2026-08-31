//! IPC surface for the web UI.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use vibebar_desktop_core::cost::CostView;
use vibebar_desktop_core::forecast::cycles::CycleSummary;
use vibebar_desktop_core::refresh::QuotaView;
use vibebar_desktop_core::sessions::{SessionListing, TranscriptCursor};
use vibebar_desktop_core::shared::settings::{PresentationSettings, SharedSettings};
use vibebar_desktop_core::shared::settings_writer::WRITABLE_KEYS as SETTINGS_WRITABLE_KEYS;
use vibebar_desktop_core::skills::SkillsInventoryView;
use vibebar_desktop_core::status::ServiceStatusView;

use crate::native_app::{self, NativeAppPresence};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    /// The data root in use, so the UI can show which Vibe Bar data it reads.
    pub data_root: String,
    pub is_demo: bool,
    pub native_app: NativeAppPresence,
}

/// One bucket's reset cycles: the finished ones oldest first, then the one
/// still open. Separate fields because the chart draws the open one
/// differently — outlined rather than filled.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetHistory {
    pub completed: Vec<CycleSummary>,
    pub current: Option<CycleSummary>,
}

/// The current view without hitting the network.
#[tauri::command]
pub fn quota_view(state: State<'_, AppState>) -> QuotaView {
    state.engine().cached_view()
}

/// The current presentation projection from the shared settings file. This is
/// deliberately a fresh read on every IPC call: Desktop neither caches nor
/// writes the shared settings document.
#[tauri::command]
pub fn presentation_settings(state: State<'_, AppState>) -> PresentationSettings {
    SharedSettings::load(state.data_root()).presentation()
}

/// Save one or more shared settings.
///
/// Returns the settings as they read afterwards, which is not necessarily
/// what was asked for: the file is shared, and a value the native app changed
/// in between wins over a stale idea of it here.
#[tauri::command]
pub fn save_shared_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    changes: serde_json::Map<String, serde_json::Value>,
) -> Result<PresentationSettings, String> {
    let refused: Vec<&str> = changes
        .keys()
        .map(String::as_str)
        .filter(|key| !SETTINGS_WRITABLE_KEYS.contains(key))
        .collect();
    if !refused.is_empty() {
        // Not a permission failure to report to the user: Desktop's own UI is
        // the only caller, so this is a bug in it.
        return Err(format!("not a setting Vibe Bar Desktop presents: {refused:?}"));
    }
    let applied = state
        .settings()
        .lock()
        .map_err(|_| "the settings writer is unavailable".to_string())?
        .apply(&changes);
    // A save re-reads, so it can be the first to see the native app's change.
    // Reported down the same channel the watch uses, or the watch would find
    // a file that already matches what this save recorded, and say nothing.
    if let Some(replaced) = applied.folded.replaced {
        let _ = app.emit(crate::SETTINGS_EVENT, Some(replaced.replaced_keys));
    }
    Ok(SharedSettings::load(state.data_root()).presentation())
}

#[tauri::command]
pub fn status_snapshot(state: State<'_, AppState>) -> ServiceStatusView {
    state.status().cached()
}

#[tauri::command]
pub fn cost_view(state: State<'_, AppState>) -> CostView {
    state.cost().cached()
}

#[tauri::command]
pub async fn refresh_cost(state: State<'_, AppState>) -> Result<CostView, String> {
    let engine = state.cost().clone();
    tauri::async_runtime::spawn_blocking(move || engine.refresh())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_status(state: State<'_, AppState>) -> Result<ServiceStatusView, String> {
    state
        .status()
        .refresh()
        .await
        .map_err(|error| error.to_string())
}

/// Fetch live quota for every provider with an adapter, then return the
/// merged view.
#[tauri::command]
pub async fn refresh_quota(state: State<'_, AppState>) -> Result<QuotaView, String> {
    Ok(state.engine().refresh().await)
}

/// Hide the borderless Mini through the same state/persistence path as the
/// tray toggle. The Mini's own close button is the user-reachable close path.
#[tauri::command]
pub fn hide_mini(app: AppHandle) {
    crate::mini_window::hide(&app);
}

#[tauri::command]
pub fn session_list(state: State<'_, AppState>, limit: Option<usize>) -> SessionListing {
    state.sessions().list(limit.unwrap_or(100).clamp(1, 500))
}

#[tauri::command]
pub fn session_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> SessionListing {
    state
        .sessions()
        .search(&query, limit.unwrap_or(50).clamp(1, 200))
}

#[tauri::command]
pub fn session_transcript(
    state: State<'_, AppState>,
    session_ref: String,
    offset: Option<usize>,
    limit: Option<usize>,
    cursor: Option<TranscriptCursor>,
) -> Result<serde_json::Value, String> {
    let page = state
        .sessions()
        .transcript_with_cursor(
            &session_ref,
            offset.unwrap_or(0),
            limit.unwrap_or(50).clamp(1, 200),
            cursor,
        )
        .map_err(|e| e.to_string())?;
    serde_json::to_value(page).map_err(|e| e.to_string())
}

/// The reset cycles behind one bucket, for the reset-history chart.
///
/// Read-only and outside the refresh path: the chart is drawn from what a
/// refresh already recorded, so opening a card can never mutate the store.
#[tauri::command]
pub fn quota_cycles(
    state: State<'_, AppState>,
    account_id: String,
    bucket_id: String,
) -> ResetHistory {
    // The native chart shows at most twelve bars, so a wider window would be
    // read and thrown away. Sized for the longest cycle a provider uses.
    const LOOKBACK_SECONDS: f64 = 120.0 * 86_400.0;
    let (completed, current) = vibebar_desktop_core::forecast::cycles_for(
        state.data_root(),
        &account_id,
        &bucket_id,
        LOOKBACK_SECONDS,
    );
    ResetHistory {
        completed,
        current,
    }
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let root = state.data_root();
    AppInfo {
        version: vibebar_desktop_core::VERSION,
        data_root: root.shared().display().to_string(),
        is_demo: root.is_demo(),
        native_app: native_app::detect(root),
    }
}

#[tauri::command]
pub fn skills_inventory(state: State<'_, AppState>) -> SkillsInventoryView {
    vibebar_desktop_core::skills::scan(state.data_root())
}
