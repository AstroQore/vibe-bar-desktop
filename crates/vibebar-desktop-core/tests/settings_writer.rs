//! What `settings.json` looks like after Desktop saves.
//!
//! Desktop holds no in-memory settings model — every surface reads the file
//! when it needs it — so a save is a locked read, the changed keys set on top,
//! and a write. These cover what that must leave behind.

use serde_json::{json, Map, Value};
use vibebar_desktop_core::shared::settings_document;
use vibebar_desktop_core::shared::settings_writer::SettingsWriter;

fn object(value: Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
}

struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new(contents: Value) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("settings.json");
        let fixture = Self { _directory: directory, path };
        fixture.write_externally(contents);
        fixture
    }

    fn writer(&self) -> SettingsWriter {
        SettingsWriter::new(self.path.clone())
    }

    fn on_disk(&self) -> Map<String, Value> {
        settings_document::read(&self.path).expect("settings.json is an object")
    }

    /// How the native app's writes arrive here: a whole file, atomically.
    fn write_externally(&self, contents: Value) {
        std::fs::write(&self.path, serde_json::to_vec_pretty(&contents).expect("bytes"))
            .expect("external write");
    }
}

/// The case the whole thing exists for.
#[test]
fn a_key_this_build_cannot_decode_survives_a_save() {
    let fixture = Fixture::new(json!({
        "displayMode": "remaining",
        "refreshIntervalSeconds": 600,
        "settingFromTheNativeApp": { "keepMe": true }
    }));
    let mut writer = fixture.writer();

    writer.apply(&object(json!({ "refreshIntervalSeconds": 900 })));

    let saved = fixture.on_disk();
    assert_eq!(saved["refreshIntervalSeconds"], json!(900));
    assert!(
        saved.contains_key("settingFromTheNativeApp"),
        "a key Desktop does not know was deleted by a save"
    );
    assert_eq!(saved["displayMode"], json!("remaining"), "an untouched setting was rewritten");
}

/// A save here reads the file first, so an edit the native app made a moment
/// ago is still there afterwards.
#[test]
fn another_writers_edit_is_not_undone() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 600 }));
    let mut writer = fixture.writer();

    fixture.write_externally(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 120 }));
    writer.apply(&object(json!({ "displayMode": "used" })));

    let saved = fixture.on_disk();
    assert_eq!(saved["displayMode"], json!("used"), "our own edit was dropped");
    assert_eq!(
        saved["refreshIntervalSeconds"],
        json!(120),
        "a setting this writer never touched was written back from a stale read"
    );
}

/// Desktop's Settings shows a fraction of what the native app's does. A key
/// outside that is a bug in the caller, not a setting to take over.
#[test]
fn a_setting_desktop_does_not_present_is_refused() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "skillsSyncMethod": "symlink" }));
    let mut writer = fixture.writer();

    // The debug assertion fires in a debug build; the release behaviour is to
    // skip the key, and both leave the file alone.
    let written = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        writer.apply(&object(json!({ "skillsSyncMethod": "copy" })))
    }));
    assert!(written.map(|keys| keys.is_empty()).unwrap_or(true));
    assert_eq!(fixture.on_disk()["skillsSyncMethod"], json!("symlink"));
}

#[test]
fn a_save_of_the_value_already_there_does_not_touch_the_file() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining" }));
    let mut writer = fixture.writer();

    let before = std::fs::read(&fixture.path).expect("read");
    assert!(writer.apply(&object(json!({ "displayMode": "remaining" }))).is_empty());
    assert_eq!(std::fs::read(&fixture.path).expect("read"), before);
}

/// A saved file must be indistinguishable from one the native app wrote, or
/// every handover between the two rewrites the whole document.
#[test]
fn a_saved_file_is_in_the_shape_the_native_app_writes() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 600 }));
    let mut writer = fixture.writer();
    writer.apply(&object(json!({ "displayMode": "used" })));

    let text = std::fs::read_to_string(&fixture.path).expect("read");
    assert!(text.contains("\"displayMode\" : \"used\""), "not the native separator: {text}");
    assert!(!text.ends_with('\n'), "the native writer leaves no trailing newline");
}

#[test]
fn a_change_by_another_writer_is_not_reported_as_a_loss() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 600 }));
    let mut writer = fixture.writer();

    fixture.write_externally(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 120 }));
    let change = writer.poll().expect("something changed");

    assert!(
        change.replaced.is_none(),
        "a setting this writer never chose a value for was reported as a loss"
    );
}

/// Our edit was saved, so the file agrees with it; only the record of having
/// made it survives. A check that compared the file with itself would miss it.
#[test]
fn reports_an_edit_of_ours_being_replaced() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 600 }));
    let mut writer = fixture.writer();

    writer.apply(&object(json!({ "refreshIntervalSeconds": 900 })));
    fixture.write_externally(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 120 }));

    let replaced = writer.poll().expect("something changed").replaced.expect("our edit replaced");
    assert_eq!(replaced.replaced_keys, vec!["refreshIntervalSeconds".to_string()]);
}

#[test]
fn our_own_save_is_not_reported_as_someone_elses_change() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining", "refreshIntervalSeconds": 600 }));
    let mut writer = fixture.writer();

    writer.apply(&object(json!({ "refreshIntervalSeconds": 900 })));
    assert!(writer.poll().is_none(), "our own save came back as news");
}

/// A setting adopted from the other writer is theirs. If it stayed on our
/// books, the next time they changed it we would tell the user their own
/// choice had been replaced — about a value they never picked.
#[test]
fn a_setting_taken_over_is_only_reported_once() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining" }));
    let mut writer = fixture.writer();

    writer.apply(&object(json!({ "displayMode": "used" })));
    fixture.write_externally(json!({ "displayMode": "remaining" }));
    assert!(writer.poll().expect("changed").replaced.is_some(), "the first loss was not reported");

    fixture.write_externally(json!({ "displayMode": "used" }));
    assert!(
        writer.poll().expect("changed").replaced.is_none(),
        "reported the same setting as lost again, after it stopped being ours"
    );
}

/// The user re-making a choice puts it back on our books.
#[test]
fn choosing_again_after_a_loss_is_reported_again() {
    let fixture = Fixture::new(json!({ "displayMode": "remaining" }));
    let mut writer = fixture.writer();

    writer.apply(&object(json!({ "displayMode": "used" })));
    fixture.write_externally(json!({ "displayMode": "remaining" }));
    writer.poll();

    writer.apply(&object(json!({ "displayMode": "used" })));
    fixture.write_externally(json!({ "displayMode": "remaining" }));
    assert!(writer.poll().expect("changed").replaced.is_some());
}
