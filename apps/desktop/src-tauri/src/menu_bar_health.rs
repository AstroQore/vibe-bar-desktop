//! The menu bar health watchdog — the native `MenuBarBlockWatchdog` and
//! `MenuBarAllowListRepair`, for the part of them that does not need AppKit.
//!
//! macOS 26 keeps a Control Center allow-list of menu bar apps. A hidden app
//! can retain this app's bundle id in its own `menuItemLocations`, and
//! Control Center then applies that app's `isAllowed=false` to us: the tray
//! icon vanishes with no error anywhere. The bundled `fix_menu_bar_allowlist.py`
//! (the native app's script, with this bundle id) audits and, on request,
//! removes only those stale cross-app references. This module runs it on a
//! cadence, remembers the last report, and re-registers the tray after a
//! repair.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub const HEALTH_EVENT: &str = "vibebar://menu-bar-health";
const CADENCE: Duration = Duration::from_secs(300);
/// Consecutive polluted audits before an automatic repair runs — the native
/// `confirmationsRequired`.
const CONFIRMATIONS_REQUIRED: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthState {
    #[default]
    Checking,
    Healthy,
    Blocked,
    Unavailable,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub state: HealthState,
    pub message: String,
    pub checked_at: f64,
    pub needs_full_disk_access: bool,
    pub alerts_enabled: bool,
    pub auto_repair_enabled: bool,
    /// The command a person can run themselves, for the Copy button.
    pub repair_command: Option<String>,
}

#[derive(Default)]
pub struct Watchdog {
    report: Mutex<HealthReport>,
    consecutive_blocked: Mutex<u32>,
}

impl Watchdog {
    pub fn report(&self) -> HealthReport {
        self.report.lock().map(|r| r.clone()).unwrap_or_default()
    }

    fn store(&self, report: HealthReport) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = report;
        }
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn script_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    let bundled = app
        .path()
        .resolve("resources/fix_menu_bar_allowlist.py", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|path| path.exists());
    bundled.or_else(|| {
        // `cargo run` / `tauri dev`: the script sits beside the crate.
        let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fix_menu_bar_allowlist.py");
        dev.exists().then_some(dev)
    })
}

fn settings_flags(app: &AppHandle) -> (bool, bool) {
    let state = app.state::<crate::state::AppState>();
    let settings = vibebar_desktop_core::shared::settings::SharedSettings::load(state.data_root());
    let flag = |key: &str| settings.unknown.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    (!flag("menuBarBlockAlertSuppressed"), flag("menuBarAutoRepairEnabled"))
}

/// What the audit could tell: one bounded run of the script without `--apply`.
fn run_script(app: &AppHandle, apply: bool) -> Result<(bool, String), String> {
    let script = script_path(app).ok_or_else(|| "The bundled repair script is missing.".to_string())?;
    let mut command = std::process::Command::new("/usr/bin/python3");
    command.arg(&script).arg("--bundle-id").arg(crate::tray::BUNDLE_ID);
    if apply {
        command.arg("--apply");
    }
    let output = command.output().map_err(|error| error.to_string())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.success(), text))
}

fn last_useful_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// What one script run means, read from its output alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Interpretation {
    pub state: HealthState,
    pub message: String,
    pub needs_full_disk_access: bool,
}

pub(crate) fn interpret(succeeded: bool, text: &str) -> Interpretation {
    if text.contains("Orphaned references to") {
        let owner = text.lines().find_map(|line| {
            line.trim()
                .strip_prefix('[')
                .and_then(|rest| rest.split_once("] "))
                .map(|(_, tail)| tail.split(" (").next().unwrap_or("").to_string())
        });
        return Interpretation {
            state: HealthState::Blocked,
            message: owner.map(|o| format!("Stale mapping: {o}")).unwrap_or_else(|| "Stale cross-app mapping found".into()),
            needs_full_disk_access: false,
        };
    }
    if succeeded {
        return Interpretation { state: HealthState::Healthy, message: "Control Center allow-list is clean".into(), needs_full_disk_access: false };
    }
    let denied = text.contains("Permission denied");
    Interpretation {
        state: HealthState::Unavailable,
        message: if denied { "Full Disk Access is required to inspect the allow-list".into() } else { last_useful_line(text) },
        needs_full_disk_access: denied,
    }
}

/// Audit the allow-list and remember the report; returns it.
pub fn audit(app: &AppHandle) -> HealthReport {
    let state = app.state::<crate::state::AppState>();
    let (alerts_enabled, auto_repair_enabled) = settings_flags(app);
    let repair_command = script_path(app).map(|p| format!("python3 \"{}\" --bundle-id {} --apply", p.display(), crate::tray::BUNDLE_ID));
    let base = HealthReport {
        checked_at: now(),
        alerts_enabled,
        auto_repair_enabled,
        repair_command,
        ..Default::default()
    };
    let report = if state.data_root().is_demo() {
        HealthReport { state: HealthState::Unavailable, message: "Demo mode does not inspect the live allow-list".into(), ..base }
    } else if !cfg!(target_os = "macos") {
        HealthReport { state: HealthState::Unavailable, message: "Menu bar allow-lists are a macOS 26 feature".into(), ..base }
    } else {
        match run_script(app, false) {
            Ok((succeeded, text)) => {
                let read = interpret(succeeded, &text);
                HealthReport { state: read.state, message: read.message, needs_full_disk_access: read.needs_full_disk_access, ..base }
            }
            Err(error) => HealthReport { state: HealthState::Unavailable, message: error, ..base },
        }
    };
    let watchdog = app.state::<Watchdog>();
    watchdog.store(report.clone());
    report
}

/// Run the narrow repair, restart Control Center (the script does), and
/// re-register the tray so a fresh status item lands in a clean list.
pub fn repair(app: &AppHandle) -> Result<HealthReport, String> {
    let state = app.state::<crate::state::AppState>();
    if state.data_root().is_demo() {
        return Err("Repair is unavailable in demo mode.".to_string());
    }
    let (ok, text) = run_script(app, true)?;
    if !ok {
        return Err(if text.contains("Permission denied") {
            "Vibe Bar Desktop needs Full Disk Access to repair Control Center.".to_string()
        } else {
            last_useful_line(&text)
        });
    }
    crate::tray::reregister(app).map_err(|error| error.to_string())?;
    if let Ok(mut count) = app.state::<Watchdog>().consecutive_blocked.lock() {
        *count = 0;
    }
    let mut report = audit(app);
    if report.state == HealthState::Healthy {
        report.message = if text.contains("Repair completed") || text.contains("Backup:") {
            "Repair completed and Control Center restarted.".into()
        } else {
            "Control Center allow-list was already clean.".into()
        };
        app.state::<Watchdog>().store(report.clone());
    }
    Ok(report)
}

/// The cadence: audit, count confirmations, auto-repair when enabled, and
/// tell the window when something changed.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let handle = app.clone();
            let report = tauri::async_runtime::spawn_blocking(move || audit(&handle)).await.unwrap_or_default();
            if report.state == HealthState::Blocked {
                let confirmed = app
                    .state::<Watchdog>()
                    .consecutive_blocked
                    .lock()
                    .map(|mut count| {
                        *count += 1;
                        *count
                    })
                    .unwrap_or(0);
                if report.auto_repair_enabled && confirmed >= CONFIRMATIONS_REQUIRED {
                    let handle = app.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || repair(&handle)).await;
                }
            } else if let Ok(mut count) = app.state::<Watchdog>().consecutive_blocked.lock() {
                *count = 0;
            }
            let _ = app.emit(HEALTH_EVENT, app.state::<Watchdog>().report());
            tokio::time::sleep(CADENCE).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{interpret, last_useful_line, HealthState};

    #[test]
    fn reads_the_script_output_the_way_the_native_controller_does() {
        let polluted = "Orphaned references to com.astroqore.VibeBarDesktop found in:\n  [3] com.example.hidden (isAllowed=False, left unchanged)\n\nDry run — nothing written. Re-run with --apply to fix.\n";
        let read = interpret(true, polluted);
        assert_eq!(read.state, HealthState::Blocked);
        assert_eq!(read.message, "Stale mapping: com.example.hidden");
        assert!(!read.needs_full_disk_access);

        let clean = interpret(true, "No orphaned references to com.astroqore.VibeBarDesktop — allow-list is clean.\n");
        assert_eq!(clean.state, HealthState::Healthy);

        let denied = interpret(false, "Permission denied reading Control Center's preferences.\nGrant Full Disk Access to this terminal, then run this again:\n");
        assert_eq!(denied.state, HealthState::Unavailable);
        assert!(denied.needs_full_disk_access);

        let missing = interpret(false, "not found: /Users/example/Library/Group Containers/x.plist\nNothing to repair — this macOS version may not use an allow-list.\n");
        assert_eq!(missing.state, HealthState::Unavailable);
        assert_eq!(missing.message, "Nothing to repair — this macOS version may not use an allow-list.");
        assert!(!missing.needs_full_disk_access);
    }

    #[test]
    fn last_useful_line_skips_blank_tails() {
        assert_eq!(last_useful_line("a\n  b  \n\n"), "b");
        assert_eq!(last_useful_line(""), "");
    }
}
