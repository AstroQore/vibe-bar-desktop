//! Privacy mode is set in the native app's Settings → Cost Data, and it stops
//! that client reading local sessions for spend at all. It is a statement
//! about the machine, not a preference about one window: honoured in one
//! client and ignored in the other, it is not privacy.

use std::fs;

use vibebar_desktop_core::cost::CostEngine;
use vibebar_desktop_core::paths::DataRoot;

struct Home {
    _directory: tempfile::TempDir,
    root: DataRoot,
    home: std::path::PathBuf,
}

fn home_with(privacy: bool, sessions: bool) -> Home {
    let directory = tempfile::tempdir().expect("temp dir");
    let home = directory.path().to_path_buf();
    let shared = home.join(".vibebar");
    fs::create_dir_all(&shared).expect("shared root");
    fs::write(
        shared.join("settings.json"),
        format!(r#"{{"costData":{{"privacyModeEnabled":{privacy}}}}}"#),
    )
    .expect("settings");

    if sessions {
        // The shape the Codex scanner actually reads: a turn_context naming
        // the model, then a token_count event stamped inside today's window.
        let sessions_dir = home.join(".codex/sessions/2026");
        fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs() as i64;
        let stamp = chrono::DateTime::from_timestamp(now - 60, 0)
            .expect("timestamp")
            .to_rfc3339();
        let lines = [
            serde_json::json!({"type":"turn_context","payload":{"model":"gpt-5"}}),
            serde_json::json!({
                "type":"event_msg","timestamp":stamp,
                "payload":{"type":"token_count","info":{"total_token_usage":{
                    "input_tokens":1000,"cached_input_tokens":0,"output_tokens":100}}}
            }),
        ];
        let body: String = lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect();
        fs::write(sessions_dir.join("session.jsonl"), body).expect("session file");
    }

    Home { root: DataRoot::at(shared), _directory: directory, home }
}

/// The point of the whole thing.
#[test]
fn does_not_report_spend_when_privacy_mode_is_on() {
    let fixture = home_with(true, true);
    let engine = CostEngine::new(fixture.root.clone(), fixture.home.clone());

    let view = engine.refresh().expect("refresh");

    assert!(view.privacy_suppressed, "the view does not say it was suppressed");
    assert_eq!(view.all_time.requests, 0);
    assert_eq!(view.models.len(), 0);
    assert_eq!(view.providers.len(), 0);
    assert_eq!(
        view.scanned_files, 0,
        "files were read anyway; privacy mode is meant to stop the reading, not hide the result"
    );
}

/// The control: the same session, without the setting, is found. Otherwise
/// the test above would pass on a fixture that simply has no spend in it.
#[test]
fn reports_spend_when_privacy_mode_is_off() {
    let fixture = home_with(false, true);
    let engine = CostEngine::new(fixture.root.clone(), fixture.home.clone());

    let view = engine.refresh().expect("refresh");

    assert!(!view.privacy_suppressed);
    assert!(view.scanned_files > 0, "the fixture session was not read at all");
    assert!(view.all_time.requests > 0, "the fixture session produced no request");
}

/// A snapshot saved before the setting went on is the user's spend sitting on
/// disk. The native client erases its own; leaving a copy here would make
/// turning it on mean less than it says.
///
/// The file is placed directly rather than earned by a scan: `DataRoot::at`
/// is always a demo root, which never persists one, and the claim under test
/// is about a file that exists — not about how it got there.
#[test]
fn erases_a_snapshot_left_from_before_privacy_mode_went_on() {
    let fixture = home_with(true, false);
    let snapshot = fixture.root.client_cost_snapshot_file();
    fs::create_dir_all(snapshot.parent().expect("parent")).expect("client namespace");
    fs::write(&snapshot, r#"{"allTime":{"pricedCostMicros":123}}"#).expect("stale snapshot");

    let engine = CostEngine::new(fixture.root.clone(), fixture.home.clone());

    assert!(!snapshot.is_file(), "the saved cost data was left on disk");
    assert!(engine.cached().privacy_suppressed);
    assert_eq!(engine.cached().all_time.priced_cost_micros, 0);
}

/// And a scan under privacy mode must not write one either.
#[test]
fn does_not_save_a_snapshot_while_privacy_mode_is_on() {
    let fixture = home_with(true, true);
    let engine = CostEngine::new(fixture.root.clone(), fixture.home.clone());
    engine.refresh().expect("refresh");

    assert!(!fixture.root.client_cost_snapshot_file().is_file());
}
