//! IPC surface for the web UI.

use vibebar_desktop_core::sessions::SessionListingQuery;
use vibebar_desktop_core::usage_stats::{UsageStatsQuery, UsageStatsView};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
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
        .apply(&changes)
        // The window has to hear about this: without it the control snaps
        // back to its old value and nothing says why.
        .map_err(|error| error.to_string())?;

    // A save re-reads, so it can be the first to see the native app's change.
    // Reported down the same channel the watch uses, or the watch would find
    // a file that already matches what this save recorded, and say nothing.
    if let Some(replaced) = applied.folded.replaced {
        let _ = app.emit(crate::SETTINGS_EVENT, Some(replaced.replaced_keys));
    }
    // The menu bar renders from these, and nothing else would redraw it until
    // the next quota refresh — which, if the cadence is what just changed, is
    // exactly the wait this save was meant to shorten.
    if applied.written.iter().any(|key| key == "displayMode" || key == "menuBarColorBasis") {
        crate::tray::update(&app, &state.engine().cached_view());
    }
    if applied.written.iter().any(|key| key == "refreshIntervalSeconds") {
        state.cadence_changed().notify_one();
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

/// The Usage Stats page: the retained ledger filtered and folded server-side,
/// so the page never holds 400k events in the webview.
#[tauri::command]
pub async fn usage_stats(
    state: State<'_, AppState>,
    query: UsageStatsQuery,
) -> Result<UsageStatsView, String> {
    let engine = state.cost().clone();
    tauri::async_runtime::spawn_blocking(move || engine.usage_stats(&query))
        .await
        .map_err(|error| error.to_string())?
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
/// The mini window reports the size its content needs.
///
/// Measured in the window rather than computed here: the layouts are React,
/// their sizes depend on how many buckets there are and how the text wraps,
/// and a second implementation of that arithmetic in the shell would drift
/// from the one that draws. The native client computes it instead, and pays
/// for that with a sizing table it has to keep in step with its own views.
#[tauri::command]
pub fn resize_mini(app: AppHandle, width: f64, height: f64) {
    crate::mini_window::resize_to_content(&app, width, height);
}

/// Whether a newer build is waiting, on the channel this machine follows.
///
/// The endpoint is chosen here rather than in the static config: the config
/// carries one list, and the two channels are two documents. `updateChannel`
/// is shared with the native client, so a choice made in either window
/// applies to both.
///
/// Reports rather than installs. An update that arrives without being asked
/// for is the kind of surprise this app has no business springing on someone
/// mid-session, and the native client asks first too.
/// The feed to ask, or the reason not to ask anything.
///
/// Demo mode's own banner promises no network requests, and a check is one.
/// The refusal lives here rather than only in the control that starts it: a
/// hidden button is a hidden button, not a guarantee.
fn update_endpoint(root: &vibebar_desktop_core::paths::DataRoot) -> Result<&'static str, String> {
    if root.is_demo() {
        return Err("update checks are off in demo mode".to_string());
    }
    Ok(SharedSettings::load(root).update_channel().endpoint())
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<PendingUpdate>, String> {
    let endpoint = update_endpoint(state.data_root())?
        .parse()
        .map_err(|_| "the update endpoint is not a URL".to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            // Held under an id so that installing puts on exactly what this
            // check found, rather than whatever the feed says a minute later
            // or what a second check that is still in flight returns.
            let id = state.hold_update(update);
            Ok(Some(PendingUpdate { version, id }))
        }
        Ok(None) => {
            state.drop_update();
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

/// What a check found: the version to show, and which check found it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub version: String,
    pub id: u64,
}

/// Installs the update a particular check found, and restarts into it.
///
/// Separate from the check because this is the irreversible half: it replaces
/// the running application. Nothing calls it without the person asking, and
/// the id says which answer they were looking at when they did.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    let update = state
        .take_update(id)
        .ok_or_else(|| "that check has been overtaken by a newer one".to_string())?;
    match update.download_and_install(|_, _| {}, || {}).await {
        // Nothing after this: the application is being replaced.
        Ok(()) => app.restart(),
        Err(error) => {
            // Put it back, so a network blip does not cost the check.
            state.restore_update(id, update);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub fn hide_mini(app: AppHandle) {
    crate::mini_window::hide(&app);
}

/// The popover's Mini button.
#[tauri::command]
pub fn toggle_mini(app: AppHandle) {
    crate::mini_window::toggle(&app);
}

/// The popover's Workbench and Settings buttons: the main window, on a page.
/// The page travels as an event because the window may already be open on
/// another one.
#[tauri::command]
pub fn show_main_window(app: AppHandle, page: String) {
    crate::popover::hide(&app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("navigate", page);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The popover's page lost focus: native's transient popover closes.
#[tauri::command]
pub fn hide_popover(app: AppHandle) {
    crate::popover::hide(&app);
}

/// The popover's content told the shell how big it is.
#[tauri::command]
pub fn resize_popover(app: AppHandle, width: f64, height: f64) {
    crate::popover::resize_to_content(&app, width, height);
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

/// The Sessions page's listing: search text, provider/harness filters, a
/// time bound, and an offset page, with per-harness counts for the menu.
#[tauri::command]
pub fn session_listing(state: State<'_, AppState>, query: SessionListingQuery) -> SessionListing {
    state.sessions().listing(&query)
}

/// Run a resume command in the user's terminal. Only a line shaped like a
/// resume command is accepted — one CLI name and its arguments, no shell
/// operators — so the webview cannot turn this into a general shell.
#[tauri::command]
pub fn open_in_terminal(command: String, terminal: String) -> Result<(), String> {
    if !is_resume_command(&command) {
        return Err("Only a session resume command can be opened in a terminal.".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
        let script = if terminal == "iterm2" {
            format!(
                "tell application \"iTerm\"\nactivate\ncreate window with default profile\ntell current session of current window to write text \"{escaped}\"\nend tell"
            )
        } else {
            format!("tell application \"Terminal\"\nactivate\ndo script \"{escaped}\"\nend tell")
        };
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("osascript exited with {status}"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        Err("Opening a terminal is only wired up on macOS in this release; copy the command instead.".to_string())
    }
}

/// A resume command is one known CLI followed by plain arguments — no
/// newlines, no shell operators, no substitution — optionally preceded by
/// the `cd '<dir>' && ` the core generates for project sessions, where the
/// directory is a single POSIX-quoted word.
pub(crate) fn is_resume_command(command: &str) -> bool {
    const CLIS: [&str; 6] = ["codex", "claude", "gemini", "grok", "cursor", "cursor-agent"];
    let trimmed = command.trim();
    let rest = match strip_cd_prefix(trimmed) {
        Some(Some(rest)) => rest,
        Some(None) => return false,
        None => trimmed,
    };
    let Some(first) = rest.split_whitespace().next() else {
        return false;
    };
    let cli = first.rsplit('/').next().unwrap_or(first);
    CLIS.contains(&cli) && is_plain_arguments(rest)
}

/// `Some(Some(rest))` when the line starts with a well-formed `cd '…' && `,
/// `Some(None)` when it starts with `cd` but the directory is not one quoted
/// word, `None` when there is no `cd` prefix at all.
fn strip_cd_prefix(line: &str) -> Option<Option<&str>> {
    let after_cd = line.strip_prefix("cd ")?;
    let quoted = after_cd.strip_prefix('\'')?;
    // The quoted word ends at the first `'` that is not part of the `'\''`
    // escape the quoter emits for a literal quote.
    let mut index = 0;
    let bytes = quoted.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            if quoted[index + 1..].starts_with("\\''") {
                index += 4;
                continue;
            }
            let rest = &quoted[index + 1..];
            return Some(rest.strip_prefix(" && "));
        }
        index += 1;
    }
    Some(None)
}

fn is_plain_arguments(rest: &str) -> bool {
    !rest.contains(['\n', '\r', ';', '|', '&', '`', '$', '<', '>'])
}

/// Reveal a skill directory in the file manager. Only a path inside the
/// shared skill library is accepted, so the webview cannot browse the disk.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let library = vibebar_desktop_core::paths::home_directory().join(".agents/skills");
    let candidate = std::path::Path::new(&path);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("{path}: {error}"))?;
    let root = library.canonicalize().map_err(|error| error.to_string())?;
    if !canonical.starts_with(&root) || canonical == root {
        return Err("Only a skill inside ~/.agents/skills can be revealed.".to_string());
    }
    app.opener()
        .reveal_item_in_dir(&canonical)
        .map_err(|error| error.to_string())
}

/// Whether the app is registered to launch at login.
#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|error| error.to_string())
}

/// Register or unregister launch at login; returns what the system reports.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|error| error.to_string())?;
    } else {
        manager.disable().map_err(|error| error.to_string())?;
    }
    manager.is_enabled().map_err(|error| error.to_string())
}

/// The effective per-model price table this build prices with.
#[tauri::command]
pub fn pricing_effective() -> Vec<vibebar_desktop_core::cost::EffectiveModelPricingRow> {
    vibebar_desktop_core::cost::effective_model_prices()
}

/// Open a project link in the browser. Only https links to the hosts the
/// settings pages link to are accepted.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let allowed = ["https://github.com/", "https://www.github.com/"];
    let clean = !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !clean || !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Only https links to github.com can be opened from here.".to_string());
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|error| error.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use vibebar_desktop_core::paths::DataRoot;

    #[test]
    fn demo_mode_refuses_to_reach_the_feed() {
        // `DataRoot::at` is a demo root: the banner over this control says the
        // mode makes no network requests.
        let refusal = update_endpoint(&DataRoot::at(std::env::temp_dir().join("vibebar-demo")))
            .expect_err("a demo root must not produce an endpoint to fetch");
        assert!(refusal.contains("demo"), "{refusal}");
    }
}

#[cfg(test)]
mod resume_guard_tests {
    use super::is_resume_command;

    #[test]
    fn accepts_plain_resume_lines_and_refuses_shell_operators() {
        assert!(is_resume_command("codex resume 019a1b2c-3d4e-7f80-9abc-def012345678"));
        assert!(is_resume_command("claude --resume 'abc-123'"));
        assert!(is_resume_command("/usr/local/bin/gemini --resume 7"));
        assert!(is_resume_command("cd '/Users/example/Coding/app' && codex resume 019a1b2c"));
        assert!(is_resume_command("cd '/Users/example/it'\\''s here' && claude --resume abc"));
        assert!(!is_resume_command("cd /Users/example/app && codex resume x"), "an unquoted directory is not the generated shape");
        assert!(!is_resume_command("cd '/tmp' && rm -rf ~"));
        assert!(!is_resume_command("cd '/tmp'; codex resume x"));
        assert!(!is_resume_command("rm -rf ~"));
        assert!(!is_resume_command("codex resume x; rm -rf ~"));
        assert!(!is_resume_command("codex resume $(whoami)"));
        assert!(!is_resume_command("codex resume x && curl evil"));
        assert!(!is_resume_command(""));
    }
}
