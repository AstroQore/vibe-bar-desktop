//! Writing the shared `settings.json`.
//!
//! This is the one exception to [`crate::shared`]'s read-only rule, and it
//! exists only because the conditions `docs/SHARED-STORAGE.md` sets for a
//! shared writer are now met for this file: an advisory lock both clients
//! take, lossless patch semantics that preserve unknown fields, and a merge
//! whose cases are verified against the native implementation from one shared
//! file. Every other shared store stays read-only.
//!
//! The rule is `docs/contracts/settings-write-v1.md`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::file_lock;
use super::settings_document::{self, Object};

/// The settings Desktop is allowed to change.
///
/// Not a safety mechanism against a hostile writer — it is this process, and
/// it could write anything. It is a statement of which settings Desktop's own
/// UI actually presents, so a bug that puts an unrelated key into the object
/// it hands over cannot quietly take that setting over from the native app,
/// whose Settings window is the only place most of them can be seen.
///
/// Carried over from the earlier design record in
/// `docs/contracts/settings-document-v1.md`. Growing it is a deliberate act:
/// add the setting to Desktop's own Settings first.
pub const WRITABLE_KEYS: &[&str] = &[
    "coreProviderOrder",
    "displayMode",
    "menuBarColorBasis",
    "menuBarItems",
    "menuBarTextEnabled",
    "providerPlanLabels",
    "refreshIntervalSeconds",
    "refreshOnPopoverOpen",
    "visibleCoreProviders",
];

/// What another writer took over: settings this process changed which now
/// hold someone else's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacedByAnotherWriter {
    pub replaced_keys: Vec<String>,
}

/// Something other than this process wrote the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalChange {
    /// Set only when a setting chosen here now holds someone else's value.
    /// `None` for the ordinary case: a change to something nobody here
    /// touched, which is adopted silently because nothing was lost.
    pub replaced: Option<ReplacedByAnotherWriter>,
}

/// What a save did, including anything it found the other writer had
/// changed. A save re-reads, so it can be the first to notice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The settings actually written. Empty when everything asked for was
    /// already the value on disk.
    pub written: BTreeSet<String>,
    pub folded: FoldedExternalChange,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldedExternalChange {
    /// Settings chosen here that the file now holds someone else's value for.
    pub replaced: Option<ReplacedByAnotherWriter>,
}

pub struct SettingsWriter {
    path: PathBuf,
    /// The file as this process last saw it, which is what a write measures
    /// its own edits against.
    baseline: Object,
    /// The values this process last wrote, for the settings it has written.
    /// What was replaced is measured against these, not against the file.
    last_mine: Object,
    /// The settings this process has actually changed since it started, which
    /// is the only way to tell "someone replaced the value I chose" from
    /// "someone changed a setting I have never touched".
    edited_keys: BTreeSet<String>,
}

impl SettingsWriter {
    pub fn new(path: PathBuf) -> Self {
        let baseline = settings_document::read(&path).unwrap_or_default();
        Self {
            last_mine: baseline.clone(),
            path,
            baseline,
            edited_keys: BTreeSet::new(),
        }
    }

    fn owned() -> BTreeSet<String> {
        WRITABLE_KEYS.iter().map(|key| (*key).to_string()).collect()
    }

    fn directory(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    /// Apply `changes` to the file, under the lock.
    ///
    /// Desktop holds no in-memory copy of the settings — every surface reads
    /// `settings.json` when it needs it — so a save here is not a merge of two
    /// diverged states. It is a read of what the file says now, the changed
    /// keys set on top, and a write. Every other key keeps the value it had,
    /// including the ones this build has never heard of, because they are
    /// never taken out of the object in the first place.
    ///
    /// Returns the keys actually written, empty when there was nothing to do.
    pub fn apply(&mut self, changes: &Object) -> Applied {
        let owned = Self::owned();
        let refused: Vec<&String> = changes.keys().filter(|key| !owned.contains(*key)).collect();
        debug_assert!(
            refused.is_empty(),
            "settings Desktop does not present: {refused:?} — add them to its own Settings first"
        );

        let directory = self.directory().to_path_buf();
        file_lock::with_lock("settings", &directory, || {
            let theirs =
                settings_document::read(&self.path).unwrap_or_else(|| self.baseline.clone());
            // The other writer got here first, and this save is about to put
            // its own keys on top and record the result as the file it has
            // seen. Without saying so, the next `poll` compares the file with
            // a baseline that already holds their change and finds nothing.
            let their_changes = settings_document::changed_keys(&self.baseline, &theirs, None);
            let mut merged = theirs.clone();
            let mut written = BTreeSet::new();
            for (key, value) in changes {
                if !owned.contains(key) {
                    continue;
                }
                if merged.get(key).is_some_and(|existing| {
                    settings_document::values_equal(existing, value)
                }) {
                    // Already what we would write. Recorded as ours anyway: the
                    // user chose it here, and if the other writer takes it over
                    // later they should be told.
                    self.edited_keys.insert(key.clone());
                    self.last_mine.insert(key.clone(), value.clone());
                    continue;
                }
                merged.insert(key.clone(), value.clone());
                written.insert(key.clone());
            }
            let replaced = self.replaced_among(&their_changes, &theirs, &written);
            if written.is_empty() {
                // Nothing of ours to write, but their change is still news:
                // leave the baseline where it is so `poll` reports it.
                return Applied { written, folded: FoldedExternalChange { replaced } };
            }
            let Ok(bytes) = settings_document::to_bytes(&merged) else {
                return Applied::default();
            };
            if write_atomically(&self.path, &bytes).is_err() {
                return Applied::default();
            }
            for key in &written {
                self.edited_keys.insert(key.clone());
                if let Some(value) = merged.get(key) {
                    self.last_mine.insert(key.clone(), value.clone());
                }
            }
            // Their value is our position now for anything we did not write.
            for key in their_changes.iter().filter(|key| !written.contains(*key)) {
                match merged.get(key) {
                    Some(value) => {
                        self.last_mine.insert(key.clone(), value.clone());
                    }
                    None => {
                        self.last_mine.remove(key);
                    }
                }
                self.edited_keys.remove(key);
            }
            self.baseline = merged;
            Applied { written, folded: FoldedExternalChange { replaced } }
        })
    }

    /// Which of the settings the other writer just changed had been chosen
    /// here, and now hold their value instead.
    fn replaced_among(
        &self,
        their_changes: &BTreeSet<String>,
        theirs: &Object,
        ours: &BTreeSet<String>,
    ) -> Option<ReplacedByAnotherWriter> {
        let keys: Vec<String> = their_changes
            .iter()
            .filter(|key| self.edited_keys.contains(*key) || ours.contains(*key))
            .filter(|key| match (self.last_mine.get(*key), theirs.get(*key)) {
                (Some(ours), Some(theirs)) => !settings_document::values_equal(ours, theirs),
                (None, None) => false,
                _ => true,
            })
            .cloned()
            .collect();
        (!keys.is_empty()).then_some(ReplacedByAnotherWriter { replaced_keys: keys })
    }

    /// What another writer has changed since this one last looked, and which
    /// of those settings this process had chosen a value for.
    ///
    /// The file wins: Desktop reads it fresh on every surface, so there is no
    /// stale copy to defend. The only thing worth saying is when a choice made
    /// here has been taken over, which is what `replaced` carries.
    pub fn poll(&mut self) -> Option<ExternalChange> {
        let theirs = settings_document::read(&self.path)?;
        if settings_document::values_equal(
            &Value::Object(theirs.clone()),
            &Value::Object(self.baseline.clone()),
        ) {
            return None;
        }

        let their_changes = settings_document::changed_keys(&self.baseline, &theirs, None);
        let replaced = self.replaced_among(&their_changes, &theirs, &BTreeSet::new());

        self.baseline = theirs.clone();
        // Their value is our position now, not our edit: leaving these behind
        // would report the same key as lost again the next time they touched
        // it, about a value this process no longer holds an opinion on.
        for key in &their_changes {
            match theirs.get(key) {
                Some(value) => {
                    self.last_mine.insert(key.clone(), value.clone());
                }
                None => {
                    self.last_mine.remove(key);
                }
            }
            self.edited_keys.remove(key);
        }

        Some(ExternalChange { replaced })
    }
}

/// Temporary file and rename, on a directory handle rather than a path.
///
/// The same primitives `ClientStore` uses, and for the same reason: opening
/// the directory once and creating, writing and renaming through that handle
/// means a path swapped underneath cannot redirect the write. The shared root
/// deserves at least the care the client namespace gets, not less.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "settings path has no parent")
    })?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "settings path has no file name")
    })?;
    // The shared root's own permissions are not Desktop's to change: that is
    // exactly the "helpful repair" this crate refuses everywhere else.
    let directory = crate::paths::open_or_create_ambient_dir(parent)?;

    let (temp, mut file) = crate::client_store::create_temp_file(&directory, name)?;
    let written = (|| -> std::io::Result<()> {
        crate::client_store::restrict_file(&file)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    })();
    drop(file);
    if let Err(error) = written {
        let _ = directory.remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = directory.rename(&temp, &directory, name) {
        let _ = directory.remove_file(&temp);
        return Err(error);
    }
    crate::client_store::sync_directory(&directory)
}
