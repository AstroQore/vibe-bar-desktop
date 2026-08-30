//! Where Vibe Bar's data lives, and the one place that decides it.
//!
//! Mirrors the role of the native app's `RealHomeDirectory` /
//! `VibeBarLocalStore` pair: every path used by this crate is derived here,
//! so a demo/test redirect is one override rather than a hunt across call
//! sites, and so the read-only boundary has exactly one enforcement point.

use std::path::{Path, PathBuf};

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

    /// Guard used by every write path in this crate.
    pub fn is_within_client_namespace(&self, path: &Path) -> bool {
        let client = self.client_dir();
        // Lexical containment on normalized paths, deliberately without
        // resolving symlinks — same rule the native app's skills write
        // allowlist uses.
        path.starts_with(&client)
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
}
