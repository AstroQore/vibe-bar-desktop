//! Where Vibe Bar's data lives, and the one place that decides it.
//!
//! Mirrors the role of the native app's `RealHomeDirectory` /
//! `VibeBarLocalStore` pair: every path used by this crate is derived here,
//! so a demo/test redirect is one override rather than a hunt across call
//! sites, and so the read-only boundary has exactly one enforcement point.

use std::path::{Path, PathBuf};

use cap_fs_ext::DirExt;
use cap_std::ambient_authority;
use cap_std::fs::Dir;

/// Environment override, byte-compatible with the native app's demo mode.
pub const DEMO_HOME_ENV: &str = "VIBEBAR_DEMO_HOME";

/// The canonical Vibe Bar data root plus this client's private namespace
/// inside it.
#[derive(Debug, Clone)]
pub struct DataRoot {
    root: PathBuf,
    demo: bool,
}

impl DataRoot {
    /// Resolve the data root for this machine.
    ///
    /// macOS and Linux use `~/.vibebar` — on a Mac this is deliberately the
    /// *same* directory the native app uses, because it is the user's Vibe
    /// Bar data, not any one client's private store. Windows uses
    /// `%APPDATA%\VibeBar`, which has the identical internal layout.
    pub fn discover() -> Self {
        if let Some(demo) = std::env::var_os(DEMO_HOME_ENV) {
            let path = PathBuf::from(demo);
            if !path.as_os_str().is_empty() {
                return Self {
                    root: path.join(".vibebar"),
                    demo: true,
                };
            }
        }
        Self {
            root: Self::platform_default(),
            demo: false,
        }
    }

    /// Explicit root, for tests and for hosts that manage their own layout.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            demo: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn at_non_demo(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            demo: false,
        }
    }

    fn platform_default() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                return PathBuf::from(appdata).join("VibeBar");
            }
        }
        home_directory().join(".vibebar")
    }

    /// True when running against a synthetic home. Every outbound call and
    /// every credential read must be suppressed in this mode.
    pub fn is_demo(&self) -> bool {
        self.demo
    }

    /// The shared data root. **Read-only for this client** — use
    /// [`DataRoot::client_dir`] for anything this client writes.
    pub fn shared(&self) -> &Path {
        &self.root
    }

    // Shared stores this client reads.
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn quotas_dir(&self) -> PathBuf {
        self.root.join("quotas")
    }
    pub fn quota_field_registry_file(&self) -> PathBuf {
        self.root.join("quota_field_registry.json")
    }
    pub fn service_status_file(&self) -> PathBuf {
        self.root.join("service_status.json")
    }
    pub fn session_index_file(&self) -> PathBuf {
        self.root.join("session_index.sqlite3")
    }
    /// The native app's MCP socket. Only ever probed for liveness, never
    /// connected to: Desktop must not depend on the native app for data.
    pub fn native_mcp_socket(&self) -> PathBuf {
        self.root.join("mcp.sock")
    }

    /// This client's private namespace. The only directory this crate writes.
    pub fn client_dir(&self) -> PathBuf {
        self.root.join("client").join("desktop")
    }
    pub fn client_settings_file(&self) -> PathBuf {
        self.client_dir().join("settings.json")
    }
    pub fn client_quotas_dir(&self) -> PathBuf {
        self.client_dir().join("quotas")
    }
    pub fn client_cost_snapshot_file(&self) -> PathBuf {
        self.client_dir().join("cost-snapshot.json")
    }

    /// Guard used by every write path in this crate.
    pub fn is_within_client_namespace(&self, path: &Path) -> bool {
        let client = self.client_dir();
        let Ok(relative) = path.strip_prefix(client) else {
            return false;
        };

        // `Path::starts_with` alone is not a containment check: a path such
        // as `client/desktop/../../settings.json` still has that lexical
        // prefix. All writes must name a non-empty sequence of ordinary child
        // components. Symlink containment is checked by `ClientStore` at the
        // filesystem boundary immediately before creating or renaming files.
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
    }
}

/// The user's real home directory.
///
/// On Unix this reads the passwd entry rather than `$HOME`: the native app
/// learned that lesson under the sandbox, and the passwd entry is also what
/// keeps a `sudo`-launched or service-launched process pointing at the right
/// user's data instead of root's.
pub fn home_directory() -> PathBuf {
    #[cfg(unix)]
    {
        if let Some(dir) = passwd_home() {
            return dir;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile);
    }
    PathBuf::from(".")
}

/// Open one intentionally ambient filesystem anchor. Every subsequent
/// operation must be relative to the returned directory capability.
pub(crate) fn open_ambient_dir(path: &Path) -> std::io::Result<Dir> {
    Dir::open_ambient_dir(path, ambient_authority())
}

/// Open an application data-root anchor, creating its final component when it
/// is absent. Creation is still relative to an open parent directory; an
/// existing root symlink is rejected rather than followed.
pub(crate) fn open_or_create_ambient_dir(path: &Path) -> std::io::Result<Dir> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "data root has no parent directory",
        )
    })?;
    let name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "data root has no final path component",
            )
        })?;
    let parent = open_ambient_dir(parent)?;
    open_or_create_dir_nofollow(&parent, Path::new(name))
}

/// Open each component beneath an existing directory without following a
/// symlink. `cap_std` keeps all resolution beneath `anchor`; `DirExt` adds the
/// final-component no-follow rule required for directories.
pub(crate) fn open_dir_nofollow(anchor: &Dir, relative: &Path) -> std::io::Result<Dir> {
    let mut current = anchor.try_clone()?;
    for component in normal_components(relative)? {
        current = current.open_dir_nofollow(component)?;
    }
    Ok(current)
}

/// Like [`open_dir_nofollow`], creating missing components one at a time. A
/// concurrent symlink replacement is rejected by the no-follow open that
/// immediately follows creation, while the original anchor remains stable.
pub(crate) fn open_or_create_dir_nofollow(anchor: &Dir, relative: &Path) -> std::io::Result<Dir> {
    let mut current = anchor.try_clone()?;
    for component in normal_components(relative)? {
        match current.open_dir_nofollow(component) {
            Ok(directory) => current = directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match current.create_dir(component) {
                    Ok(()) => {}
                    Err(create_error)
                        if create_error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(create_error) => return Err(create_error),
                }
                current = current.open_dir_nofollow(component)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

/// Reject absolute, empty, dot, and parent components before using an
/// untrusted path as a capability-relative name.
pub(crate) fn normal_components(path: &Path) -> std::io::Result<Vec<&std::ffi::OsStr>> {
    let mut components = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "path contains a non-normal component",
            ));
        };
        components.push(component);
    }
    if components.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "path has no components",
        ));
    }
    Ok(components)
}

#[cfg(unix)]
fn passwd_home() -> Option<PathBuf> {
    use std::ffi::CStr;
    // SAFETY: getpwuid returns a pointer into a static buffer owned by libc.
    // The directory string is copied out immediately, before any further
    // libc call could reuse that buffer.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let dir = (*pw).pw_dir;
        if dir.is_null() {
            return None;
        }
        let bytes = CStr::from_ptr(dir).to_bytes();
        if bytes.is_empty() {
            return None;
        }
        Some(PathBuf::from(String::from_utf8_lossy(bytes).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_override_redirects_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(root.is_demo());
        assert!(root.settings_file().ends_with("settings.json"));
        assert!(root.client_dir().ends_with("client/desktop"));
    }

    #[test]
    fn client_namespace_guard_rejects_shared_paths() {
        let root = DataRoot::at("/tmp/vb-test/.vibebar");
        assert!(root.is_within_client_namespace(&root.client_settings_file()));
        assert!(root.is_within_client_namespace(&root.client_quotas_dir().join("x.json")));
        assert!(!root.is_within_client_namespace(
            &root
                .client_dir()
                .join("..")
                .join("..")
                .join("settings.json")
        ));
        // Every shared store is outside the writable namespace.
        assert!(!root.is_within_client_namespace(&root.settings_file()));
        assert!(!root.is_within_client_namespace(&root.quotas_dir().join("q.json")));
        assert!(!root.is_within_client_namespace(&root.session_index_file()));
    }

    #[test]
    fn home_directory_is_absolute_and_real() {
        let home = home_directory();
        assert!(home.is_absolute(), "home should be absolute: {home:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_handle_stays_anchored_after_its_name_is_replaced() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let shared = temp.path().join("shared");
        std::fs::create_dir_all(&root_path).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        let root = open_ambient_dir(&root_path).unwrap();
        let client = open_or_create_dir_nofollow(&root, Path::new("client")).unwrap();

        std::fs::rename(root_path.join("client"), root_path.join("client-moved")).unwrap();
        symlink(&shared, root_path.join("client")).unwrap();

        // The operation uses the open `client` handle, not its now-symlinked
        // pathname, so it stays in the directory that was originally opened.
        open_or_create_dir_nofollow(&client, Path::new("desktop")).unwrap();
        assert!(root_path.join("client-moved/desktop").is_dir());
        assert!(!shared.join("desktop").exists());
    }
}
