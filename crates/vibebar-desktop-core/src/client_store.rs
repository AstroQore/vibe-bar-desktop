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
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt;
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;

use crate::error::CoreError;
use crate::model::{AccountQuota, QuotaOrigin};
use crate::paths::DataRoot;

pub struct ClientStore {
    root: DataRoot,
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

    /// Atomic, namespace-guarded JSON write.
    ///
    /// Kept private so callers cannot turn this into a general-purpose file
    /// writer. Every production destination is constructed from one of the
    /// fixed `DataRoot::client_*` paths above.
    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), CoreError> {
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
        let relative_parent = parent
            .strip_prefix(self.root.client_dir())
            .map_err(|_| CoreError::WriteOutsideClientNamespace(path.display().to_string()))?;
        let filename = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| CoreError::WriteOutsideClientNamespace(path.display().to_string()))?;
        if (!relative_parent.as_os_str().is_empty()
            && crate::paths::normal_components(relative_parent).is_err())
            || crate::paths::normal_components(Path::new(filename)).is_err()
        {
            return Err(CoreError::WriteOutsideClientNamespace(
                path.display().to_string(),
            ));
        }

        // The only ambient operation is opening the shared-root anchor. The
        // client namespace is then created/opened component by component with
        // no-follow handles; temp creation and rename stay on that same final
        // directory handle, so a path replacement cannot redirect a write.
        let root = crate::paths::open_or_create_ambient_dir(self.root.shared())?;
        let client = crate::paths::open_or_create_dir_nofollow(&root, Path::new("client"))?;
        let desktop = crate::paths::open_or_create_dir_nofollow(&client, Path::new("desktop"))?;
        let directory = if relative_parent.as_os_str().is_empty() {
            desktop.try_clone()?
        } else {
            crate::paths::open_or_create_dir_nofollow(&desktop, relative_parent)?
        };
        // `root` and `client` are shared ancestors. Creating them when absent
        // is necessary to reach this namespace, but Desktop must never change
        // their existing metadata; only its own subtree is private authority.
        restrict_directory(&desktop)?;
        restrict_directory(&directory)?;

        let mut buffer = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
        value.serialize(&mut serializer)?;
        buffer.push(b'\n');

        let (temp, mut file) = create_temp_file(&directory, filename)?;
        let write_result = (|| -> std::io::Result<()> {
            restrict_file(&file)?;
            file.write_all(&buffer)?;
            file.sync_all()?;
            Ok(())
        })();
        drop(file);
        if let Err(error) = write_result {
            let _ = directory.remove_file(&temp);
            return Err(error.into());
        }
        if let Err(error) = directory.rename(&temp, &directory, filename) {
            let _ = directory.remove_file(&temp);
            return Err(error.into());
        }
        sync_directory(&directory)?;
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

fn temp_sibling(path: &std::ffi::OsStr, attempt: u8) -> std::ffi::OsString {
    let name = path.to_string_lossy();
    let unique = std::process::id();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!(".{name}.tmp-{unique}-{stamp}-{attempt}").into()
}

/// `create_new` refuses a pre-existing temp path instead of following a
/// symlink the process does not own.
fn create_temp_file(
    directory: &Dir,
    name: &std::ffi::OsStr,
) -> std::io::Result<(std::ffi::OsString, cap_std::fs::File)> {
    for attempt in 0..16 {
        let temp = temp_sibling(name, attempt);
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        options.mode(0o600);
        match directory.open_with(Path::new(&temp), &options) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a private temporary file",
    ))
}

#[cfg(unix)]
fn restrict_directory(directory: &Dir) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    directory
        .try_clone()?
        .into_std_file()
        .set_permissions(std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn restrict_file(file: &cap_std::fs::File) -> std::io::Result<()> {
    use cap_std::fs::PermissionsExt;
    file.set_permissions(cap_std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_directory(_directory: &Dir) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file(_file: &cap_std::fs::File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(directory: &Dir) -> std::io::Result<()> {
    directory.try_clone()?.into_std_file().sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Dir) -> std::io::Result<()> {
    // Windows has no portable std equivalent of fsyncing a directory handle;
    // the atomic rename is still capability-scoped and crash consistency is
    // provided by the filesystem's replace semantics.
    Ok(())
}

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
            let err = store
                .write_json(&forbidden, &serde_json::json!({}))
                .unwrap_err();
            assert!(matches!(err, CoreError::WriteOutsideClientNamespace(_)));
            assert!(
                !forbidden.exists(),
                "nothing may be created outside the namespace"
            );
        }
    }

    #[test]
    fn refuses_a_parent_directory_escape() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let store = ClientStore::new(root.clone());
        let escaped = root
            .client_dir()
            .join("..")
            .join("..")
            .join("settings.json");

        let err = store
            .write_json(&escaped, &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err, CoreError::WriteOutsideClientNamespace(_)));
        assert!(!root.settings_file().exists());
    }

    #[test]
    fn writes_a_direct_client_namespace_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let store = ClientStore::new(root.clone());
        let path = root.client_settings_file();

        store
            .write_json(&path, &serde_json::json!({"firstRun": false}))
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["firstRun"], false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_private_subdirectory() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();
        symlink(root.shared(), root.client_quotas_dir()).unwrap();

        let result = ClientStore::new(root.clone()).save_quota(&sample_quota());
        let escaped_quota = root
            .shared()
            .join(crate::shared::quota_cache::cache_file_component(
                "oauth-codex",
            ))
            .with_extension("json");
        assert!(
            result.is_err(),
            "a symlink must never be followed for writes"
        );
        assert!(
            !escaped_quota.exists(),
            "the shared root must not receive a client quota"
        );
        assert!(!root.settings_file().exists());
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
        ClientStore::new(root.clone())
            .save_quota(&sample_quota())
            .unwrap();

        for directory in [root.client_dir(), root.client_quotas_dir()] {
            let mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} is {mode:o}", directory.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn preserves_shared_ancestor_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared().join("client")).unwrap();
        std::fs::set_permissions(root.shared(), std::fs::Permissions::from_mode(0o751)).unwrap();
        std::fs::set_permissions(
            root.shared().join("client"),
            std::fs::Permissions::from_mode(0o711),
        )
        .unwrap();

        ClientStore::new(root.clone())
            .save_quota(&sample_quota())
            .unwrap();

        let root_mode = std::fs::metadata(root.shared())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let client_mode = std::fs::metadata(root.shared().join("client"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(root_mode, 0o751);
        assert_eq!(client_mode, 0o711);
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
}
