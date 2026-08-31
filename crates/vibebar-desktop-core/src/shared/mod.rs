//! Read-only access to the shared Vibe Bar data root.
//!
//! Every reader here is tolerant by construction: a store written by a newer
//! native build must degrade to "not available" rather than fail the app, and
//! nothing in this module ever writes, deletes, migrates, or rebuilds a
//! shared file. That is not politeness — the shared stores have no
//! cross-process locking yet, and several of them (`session_index`,
//! `usage_events`, `scan_cache`) respond to a schema mismatch by dropping
//! data. A second implementation that "helpfully" repairs one would destroy
//! the user's history.

pub mod field_registry;
pub mod quota_cache;
pub mod service_status;
pub mod settings;

/// Seconds between the Apple reference date (2001-01-01) and the Unix epoch.
/// Swift's `JSONEncoder` writes `Date` as reference-date seconds by default,
/// so every timestamp read out of a shared JSON store crosses this boundary.
pub const APPLE_EPOCH_OFFSET: f64 = 978_307_200.0;

pub fn apple_seconds_to_unix(value: f64) -> f64 {
    value + APPLE_EPOCH_OFFSET
}

/// Read a JSON file, returning `Ok(None)` when it is absent or unreadable.
///
/// Deliberately swallows the difference between "missing" and "corrupt": for
/// a read-only consumer both mean the same thing (render without it), and the
/// alternative — surfacing a parse error as a hard failure — would let one
/// bad shared file take down a client that does not own it.
pub(crate) fn read_json_file<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
    max_bytes: u64,
) -> Option<T> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > max_bytes {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Directory entries that are real JSON files, skipping the artifacts that
/// genuinely live in these directories: `.DS_Store`, `*.bak`, and the
/// `*.sb-<pid>-<rand>` siblings a Foundation atomic write leaves behind when
/// it is interrupted.
pub(crate) fn json_files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || !name.ends_with(".json") {
            continue;
        }
        out.push(path);
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_epoch_conversion_matches_native_cache() {
        // 809731205.24 (reference-date seconds) is 2026-08-30 in Unix terms.
        let unix = apple_seconds_to_unix(809_731_205.24);
        assert!((unix - 1_788_038_405.24).abs() < 0.01, "got {unix}");
    }

    #[test]
    fn json_file_listing_skips_foreign_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.json"), "{}").unwrap();
        std::fs::write(dir.path().join(".DS_Store"), "x").unwrap();
        std::fs::write(dir.path().join("b.json.sb-34695573-YVfz45"), "{}").unwrap();
        std::fs::write(dir.path().join("c.bak"), "{}").unwrap();
        let files = json_files_in(dir.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.json"));
    }
}
