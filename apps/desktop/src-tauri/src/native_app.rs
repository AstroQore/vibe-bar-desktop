//! Detecting the macOS native app — and nothing more.
//!
//! Desktop must run identically whether or not the native app is installed.
//! This module exists only so the UI can say "the native macOS app is also
//! available here" and offer to open it. Nothing in the data path may branch
//! on the result, and Desktop never talks to the native app's MCP socket or
//! any other native component.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppPresence {
    /// The native macOS app bundle appears to be installed.
    pub installed: bool,
    /// It also appears to be running right now (its MCP socket exists).
    /// Informational only — never a data source.
    pub running: bool,
    pub bundle_id: &'static str,
}

const BUNDLE_ID: &str = "com.astroqore.VibeBar";

pub fn detect(data_root: &vibebar_desktop_core::paths::DataRoot) -> NativeAppPresence {
    NativeAppPresence {
        installed: bundle_installed(),
        // A socket file is a hint, not a handshake: we only stat it.
        running: data_root.native_mcp_socket().exists(),
        bundle_id: BUNDLE_ID,
    }
}

#[cfg(target_os = "macos")]
fn bundle_installed() -> bool {
    ["/Applications/Vibe Bar.app"]
        .iter()
        .any(|path| std::path::Path::new(path).is_dir())
        || vibebar_desktop_core::paths::home_directory()
            .join("Applications/Vibe Bar.app")
            .is_dir()
}

#[cfg(not(target_os = "macos"))]
fn bundle_installed() -> bool {
    false
}
