//! IPC surface for the web UI.

use serde::Serialize;
use tauri::State;
use vibebar_desktop_core::cost::CostView;
use vibebar_desktop_core::refresh::QuotaView;
use vibebar_desktop_core::sessions::SessionListing;
use vibebar_desktop_core::shared::settings::{PresentationSettings, SharedSettings};
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
) -> Result<serde_json::Value, String> {
    let page = state
        .sessions()
        .transcript(
            &session_ref,
            offset.unwrap_or(0),
            limit.unwrap_or(50).clamp(1, 200),
        )
        .map_err(|e| e.to_string())?;
    serde_json::to_value(page).map_err(|e| e.to_string())
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
