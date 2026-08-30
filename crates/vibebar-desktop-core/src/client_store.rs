//! The only place this crate writes.
//!
//! Everything lands under `<data root>/client/desktop/`, and every write goes
//! through [`ClientStore::write_json`], which refuses paths outside that
//! namespace. The guard exists because the shared root has no cross-process
//! locking: the native app coalesces writes for up to 30 seconds and several
//! of its stores respond to a schema mismatch by dropping data, so a stray
//! write from a second implementation is not a merge conflict — it is data
//! loss with no way to notice.
//!
//! Writes are atomic (temp file in the destination directory, then rename)
//! and formatted the way the native `VibeBarLocalStore.writeJSON` formats:
//! pretty-printed with sorted keys, so a human diff of the two clients'
//! files stays readable.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::CoreError;
use crate::model::{AccountQuota, QuotaOrigin};
use crate::paths::DataRoot;

pub struct ClientStore {
    root: DataRoot,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct LaunchStateFile {
    schema: u8,
    has_completed_first_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunState {
    Missing,
    Completed,
    Unusable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    Show,
    ShowAndMarkFirstRunComplete,
    HideToTray,
}

/// The only safe tray-only transition: a known local completion record plus
/// a successfully installed tray. Every uncertainty leaves the main window
/// visible, rather than creating a headless app with no usable control.
pub fn startup_action(demo: bool, tray_installed: bool, state: FirstRunState) -> StartupAction {
    if demo || !tray_installed {
        return StartupAction::Show;
    }
    match state {
        FirstRunState::Completed => StartupAction::HideToTray,
        FirstRunState::Missing => StartupAction::ShowAndMarkFirstRunComplete,
        FirstRunState::Unusable => StartupAction::Show,
    }
}

impl ClientStore {
    pub fn new(root: DataRoot) -> Self {
        Self { root }
    }

    pub fn data_root(&self) -> &DataRoot {
        &self.root
    }

    /// Persist a quota this client fetched. Note the destination: Desktop's
    /// own `client/desktop/quotas/`, never the shared `quotas/` the native
    /// app owns.
    pub fn save_quota(&self, quota: &AccountQuota) -> Result<(), CoreError> {
        let path = self
            .root
            .client_quotas_dir()
            .join(crate::shared::quota_cache::cache_file_component(
                &quota.account_id,
            ))
            .with_extension("json");
        self.write_json(&path, quota)
    }

    /// Load back everything [`ClientStore::save_quota`] has written, so a
    /// relaunch shows real numbers before the first refresh completes.
    pub fn load_quotas(&self) -> Vec<AccountQuota> {
        let mut out = Vec::new();
        for path in crate::shared::json_files_in(&self.root.client_quotas_dir()) {
            let Some(mut quota) =
                crate::shared::read_json_file::<StoredClientQuota>(&path, 4 * 1024 * 1024)
                    .and_then(|s| s.into_quota())
            else {
                continue;
            };
            // A restored observation is cache, not a live reading.
            quota.origin = QuotaOrigin::SharedCache;
            out.push(quota);
        }
        out
    }

    pub fn first_run_state(&self) -> FirstRunState {
        let path = self.root.client_launch_state_file();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return FirstRunState::Missing
            }
            Err(_) => return FirstRunState::Unusable,
        };
        if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > 16 * 1024 {
            return FirstRunState::Unusable;
        }
        let Some(state) = crate::shared::read_json_file::<LaunchStateFile>(&path, 16 * 1024) else {
            return FirstRunState::Unusable;
        };
        if state.schema != 1 {
            return FirstRunState::Unusable;
        }
        if state.has_completed_first_run {
            FirstRunState::Completed
        } else {
            FirstRunState::Missing
        }
    }

    pub fn mark_first_run_complete(&self) -> Result<(), CoreError> {
        self.write_json(
            &self.root.client_launch_state_file(),
            &LaunchStateFile {
                schema: 1,
                has_completed_first_run: true,
            },
        )
    }

    /// Atomic, namespace-guarded JSON write.
    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), CoreError> {
        if !self.root.is_within_client_namespace(path) {
            return Err(CoreError::WriteOutsideClientNamespace(
                path.display().to_string(),
            ));
        }
        let Some(parent) = path.parent() else {
            return Err(CoreError::WriteOutsideClientNamespace(
                path.display().to_string(),
            ));
        };
        std::fs::create_dir_all(parent)?;
        // Tighten every directory we created, not just the leaf: the client
        // namespace holds observations about the user's accounts.
        for directory in [self.root.client_dir().as_path(), parent].iter().copied() {
            restrict_directory(directory);
        }

        let mut buffer = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
        value.serialize(&mut serializer)?;
        buffer.push(b'\n');

        let temp = temp_sibling(path);
        {
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(&buffer)?;
            file.sync_all()?;
        }
        restrict_file(&temp);
        std::fs::rename(&temp, path)?;
        Ok(())
    }
}

/// Round-trip shape for quotas this client wrote. Kept separate from the
/// shared cache's `StoredQuota` on purpose: this is Desktop's own format and
/// may diverge, whereas the shared shape is a contract with the native app.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredClientQuota {
    account_id: String,
    tool: String,
    #[serde(default)]
    buckets: Vec<crate::model::QuotaBucket>,
    #[serde(default)]
    plan: Option<String>,
    queried_at: f64,
}

impl StoredClientQuota {
    fn into_quota(self) -> Option<AccountQuota> {
        Some(AccountQuota {
            account_id: self.account_id,
            tool: crate::model::ToolType::from_raw(&self.tool)?,
            buckets: self.buckets,
            plan: self.plan,
            queried_at: self.queried_at,
            origin: QuotaOrigin::SharedCache,
            error: None,
        })
    }
}

fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tmp".to_string());
    let unique = std::process::id();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    path.with_file_name(format!(".{name}.tmp-{unique}-{stamp}"))
}

#[cfg(unix)]
fn restrict_directory(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(unix)]
fn restrict_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) {}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{QuotaBucket, ToolType};

    fn sample_quota() -> AccountQuota {
        AccountQuota {
            account_id: "oauth-codex".into(),
            tool: ToolType::Codex,
            buckets: vec![QuotaBucket::new(
                "weekly",
                "Weekly",
                "wk",
                42.0,
                Some(1_788_626_819.0),
                Some(604_800),
                None,
            )],
            plan: Some("pro".into()),
            queried_at: 1_788_038_405.0,
            origin: QuotaOrigin::Live,
            error: None,
        }
    }

    #[test]
    fn round_trips_a_quota_through_the_client_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClientStore::new(DataRoot::at(dir.path().join(".vibebar")));
        store.save_quota(&sample_quota()).unwrap();

        let restored = store.load_quotas();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].account_id, "oauth-codex");
        assert_eq!(restored[0].buckets[0].used_percent, 42.0);
        // Restored data is never claimed as live.
        assert_eq!(restored[0].origin, QuotaOrigin::SharedCache);
    }

    #[test]
    fn refuses_to_write_outside_the_client_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let store = ClientStore::new(root.clone());

        for forbidden in [
            root.settings_file(),
            root.quotas_dir().join("quota-v1-abc.json"),
            root.shared().join("cost_history.json"),
        ] {
            let err = store.write_json(&forbidden, &serde_json::json!({})).unwrap_err();
            assert!(matches!(err, CoreError::WriteOutsideClientNamespace(_)));
            assert!(!forbidden.exists(), "nothing may be created outside the namespace");
        }
    }

    #[test]
    fn writing_never_touches_the_shared_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        // Pre-existing shared state the native app owns.
        std::fs::create_dir_all(root.quotas_dir()).unwrap();
        std::fs::write(root.settings_file(), "{\"displayMode\":\"used\"}").unwrap();
        let before = std::fs::read(root.settings_file()).unwrap();

        let store = ClientStore::new(root.clone());
        store.save_quota(&sample_quota()).unwrap();

        assert_eq!(std::fs::read(root.settings_file()).unwrap(), before);
        assert!(
            crate::shared::json_files_in(&root.quotas_dir()).is_empty(),
            "the shared quota cache must stay untouched"
        );
        assert!(root.client_quotas_dir().exists());
    }

    #[cfg(unix)]
    #[test]
    fn every_created_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        ClientStore::new(root.clone()).save_quota(&sample_quota()).unwrap();

        for directory in [root.client_dir(), root.client_quotas_dir()] {
            let mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} is {mode:o}", directory.display());
        }
    }

    #[test]
    fn no_temp_files_survive_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClientStore::new(DataRoot::at(dir.path().join(".vibebar")));
        store.save_quota(&sample_quota()).unwrap();
        let entries: Vec<_> = std::fs::read_dir(store.data_root().client_quotas_dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries.len(), 1, "got {entries:?}");
        assert!(entries[0].starts_with("quota-v1-"));
    }

    #[test]
    fn first_run_state_round_trips_only_in_the_desktop_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let store = ClientStore::new(root.clone());
        assert_eq!(store.first_run_state(), FirstRunState::Missing);
        store.mark_first_run_complete().unwrap();
        assert_eq!(store.first_run_state(), FirstRunState::Completed);
        assert!(root.client_launch_state_file().is_file());
        assert!(!root.settings_file().exists());
    }

    #[test]
    fn unknown_first_run_schema_is_unusable_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();
        std::fs::write(
            root.client_launch_state_file(),
            r#"{"schema":2,"hasCompletedFirstRun":true}"#,
        )
        .unwrap();
        let before = std::fs::read(root.client_launch_state_file()).unwrap();
        assert_eq!(
            ClientStore::new(root.clone()).first_run_state(),
            FirstRunState::Unusable
        );
        assert_eq!(
            std::fs::read(root.client_launch_state_file()).unwrap(),
            before
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_first_run_state_is_never_trusted() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, r#"{"schema":1,"hasCompletedFirstRun":true}"#).unwrap();
        symlink(&outside, root.client_launch_state_file()).unwrap();
        assert_eq!(
            ClientStore::new(root).first_run_state(),
            FirstRunState::Unusable
        );
    }

    #[test]
    fn startup_decision_fails_open_to_a_visible_window() {
        assert_eq!(
            startup_action(false, true, FirstRunState::Completed),
            StartupAction::HideToTray
        );
        assert_eq!(
            startup_action(false, true, FirstRunState::Missing),
            StartupAction::ShowAndMarkFirstRunComplete
        );
        assert_eq!(
            startup_action(false, true, FirstRunState::Unusable),
            StartupAction::Show
        );
        assert_eq!(
            startup_action(true, true, FirstRunState::Completed),
            StartupAction::Show
        );
        assert_eq!(
            startup_action(false, false, FirstRunState::Completed),
            StartupAction::Show
        );
    }
}
