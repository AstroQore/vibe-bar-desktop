//! Whether the setup assistant should open on launch — the native
//! `OnboardingGate` verbatim, on the same shared signals, so a Mac that has
//! seen the native app's assistant is not asked twice and a fresh install
//! gets it from whichever client launches first.

use crate::paths::DataRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    /// The person has been through it (or said no to it).
    Skip,
    /// Not completed, but this is not a fresh install: quota caches or a
    /// settings file already exist, so an upgrade should not be greeted.
    /// The caller records completion instead of showing.
    MarkCompleted,
    /// A fresh install: show the assistant.
    Show,
}

pub fn decide(has_completed_onboarding: bool, has_quota_caches: bool, had_settings_file: bool) -> Decision {
    if has_completed_onboarding {
        return Decision::Skip;
    }
    if has_quota_caches || had_settings_file {
        return Decision::MarkCompleted;
    }
    Decision::Show
}

/// Any non-hidden entry in the shared quota directory.
pub fn has_quota_caches(root: &DataRoot) -> bool {
    std::fs::read_dir(root.quotas_dir())
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false)
}

/// The decision for this data root, read from the shared settings file and
/// the shared quota directory as they are right now.
pub fn decide_for(root: &DataRoot, has_completed_onboarding: bool) -> Decision {
    decide(
        has_completed_onboarding,
        has_quota_caches(root),
        root.settings_file().is_file(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_matches_the_native_rule() {
        assert_eq!(decide(true, true, true), Decision::Skip);
        assert_eq!(decide(true, false, false), Decision::Skip);
        assert_eq!(decide(false, true, false), Decision::MarkCompleted);
        assert_eq!(decide(false, false, true), Decision::MarkCompleted);
        assert_eq!(decide(false, false, false), Decision::Show);
    }

    #[test]
    fn quota_caches_ignore_hidden_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(!has_quota_caches(&root));
        std::fs::create_dir_all(root.quotas_dir()).unwrap();
        std::fs::write(root.quotas_dir().join(".DS_Store"), "").unwrap();
        assert!(!has_quota_caches(&root));
        std::fs::write(root.quotas_dir().join("quota-v1-abc.json"), "{}").unwrap();
        assert!(has_quota_caches(&root));
    }
}
