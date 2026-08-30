//! macOS login-keychain reads for credentials the coding CLIs wrote.
//!
//! Only *other* applications' items are read (`Codex Auth`,
//! `Claude Code-credentials`), and only ever read — this client stores
//! nothing in the keychain in this slice. macOS will prompt for access the
//! first time; that prompt is the user's decision point and is never
//! suppressed or worked around.
//!
//! On non-macOS platforms every lookup reports "not found", and the file-based
//! credential paths carry the whole story.

/// Read a generic-password item's payload as a UTF-8 string.
#[cfg(target_os = "macos")]
pub fn read_generic_password(service: &str) -> Option<String> {
    use security_framework::passwords::get_generic_password;

    // The CLIs write their item with the service name as both service and
    // (usually) account; try the account-less lookup first, then the common
    // convention of account == service.
    for account in ["", service] {
        if let Ok(bytes) = get_generic_password(service, account) {
            if let Ok(text) = String::from_utf8(bytes) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn read_generic_password(_service: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_service_is_absent_not_an_error() {
        // A service no one has ever written must simply be missing, on every
        // platform — including a macOS CI runner with an empty keychain.
        assert!(read_generic_password("com.astroqore.VibeBarDesktop.definitely-absent").is_none());
    }
}
