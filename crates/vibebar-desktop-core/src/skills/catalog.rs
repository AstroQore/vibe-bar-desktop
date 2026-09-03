//! Which apps a skill is projected into and where — the native
//! `SkillAppCatalog` and `SkillAppTarget`, row for row.

use std::path::{Path, PathBuf};

pub const SSOT_RELATIVE_PATH: &str = ".agents/skills";
pub const LOCK_FILE_RELATIVE_PATH: &str = ".agents/.skill-lock.json";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum AppTarget {
    Claude,
    Codex,
    Gemini,
    Grok,
    Hermes,
    Opencode,
    Antigravity,
    Cursor,
}

impl AppTarget {
    pub const ALL: [AppTarget; 8] = [
        AppTarget::Claude,
        AppTarget::Codex,
        AppTarget::Gemini,
        AppTarget::Grok,
        AppTarget::Hermes,
        AppTarget::Opencode,
        AppTarget::Antigravity,
        AppTarget::Cursor,
    ];

    /// The harnesses the page offers. Hermes and OpenCode stay in the
    /// allow-list only so old registries decode and clean up.
    pub const MANAGED: [AppTarget; 6] = [
        AppTarget::Codex,
        AppTarget::Claude,
        AppTarget::Gemini,
        AppTarget::Antigravity,
        AppTarget::Grok,
        AppTarget::Cursor,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            AppTarget::Claude => "claude",
            AppTarget::Codex => "codex",
            AppTarget::Gemini => "gemini",
            AppTarget::Grok => "grok",
            AppTarget::Hermes => "hermes",
            AppTarget::Opencode => "opencode",
            AppTarget::Antigravity => "antigravity",
            AppTarget::Cursor => "cursor",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|app| app.raw() == raw)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AppTarget::Claude => "Claude Code",
            AppTarget::Codex => "Codex",
            AppTarget::Gemini => "Gemini CLI",
            AppTarget::Grok => "Grok Build",
            AppTarget::Hermes => "Hermes",
            AppTarget::Opencode => "OpenCode",
            AppTarget::Antigravity => "AntiGravity",
            AppTarget::Cursor => "Cursor",
        }
    }

    pub fn relative_path(self) -> &'static str {
        match self {
            AppTarget::Claude => ".claude/skills",
            AppTarget::Codex => ".codex/skills",
            AppTarget::Gemini => ".gemini/skills",
            AppTarget::Grok => ".grok/skills",
            AppTarget::Hermes => ".hermes/skills",
            AppTarget::Opencode => ".config/opencode/skills",
            AppTarget::Antigravity => ".gemini/config/skills",
            AppTarget::Cursor => ".cursor/skills",
        }
    }

    /// Has a per-skill on/off in its own config (`config.toml`,
    /// `settings.json`) that the native app patches. This client reads
    /// those states and does not patch them yet.
    pub fn supports_native_activation(self) -> bool {
        matches!(
            self,
            AppTarget::Codex | AppTarget::Claude | AppTarget::Gemini | AppTarget::Grok
        )
    }

    /// Reads `~/.agents/skills` directly, so removing its projection alone
    /// does not hide a skill from it.
    pub fn discovers_shared_root(self) -> bool {
        matches!(
            self,
            AppTarget::Codex | AppTarget::Gemini | AppTarget::Grok | AppTarget::Cursor
        )
    }
}

pub fn ssot_dir(home: &Path) -> PathBuf {
    home.join(SSOT_RELATIVE_PATH)
}

pub fn lock_file(home: &Path) -> PathBuf {
    home.join(LOCK_FILE_RELATIVE_PATH)
}

pub fn skills_dir(app: AppTarget, home: &Path) -> PathBuf {
    home.join(app.relative_path())
}

/// The SSOT plus every app skills directory: the sync engine asserts that
/// each path it mutates sits under one of these.
pub fn allowed_write_roots(home: &Path) -> Vec<PathBuf> {
    std::iter::once(ssot_dir(home))
        .chain(AppTarget::ALL.iter().map(|app| skills_dir(*app, home)))
        .collect()
}

/// Lexically below one of the allowed roots. Lexical on purpose: the check
/// is about what a name resolved to, before anything on disk is consulted.
pub fn is_write_allowed(path: &Path, home: &Path) -> bool {
    let candidate = lexical_normalize(path);
    allowed_write_roots(home)
        .iter()
        .map(|root| lexical_normalize(root))
        .any(|root| candidate.starts_with(&root) && candidate != root)
}

/// Collapse `.` and `..` without touching the disk.
pub(crate) fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_stay_under_the_roots() {
        let home = Path::new("/Users/example");
        assert!(is_write_allowed(
            Path::new("/Users/example/.agents/skills/docx"),
            home
        ));
        assert!(is_write_allowed(
            Path::new("/Users/example/.gemini/config/skills/docx"),
            home
        ));
        assert!(
            !is_write_allowed(Path::new("/Users/example/.agents/skills"), home),
            "the root itself never qualifies"
        );
        assert!(!is_write_allowed(
            Path::new("/Users/example/.agents/skills/../../.ssh"),
            home
        ));
        assert!(!is_write_allowed(
            Path::new("/Users/example/.gemini/config"),
            home
        ));
    }

    #[test]
    fn the_catalog_matches_the_native_rows() {
        assert_eq!(
            AppTarget::Antigravity.relative_path(),
            ".gemini/config/skills"
        );
        assert_eq!(
            AppTarget::Opencode.relative_path(),
            ".config/opencode/skills"
        );
        assert_eq!(AppTarget::MANAGED.len(), 6);
        assert!(
            AppTarget::Cursor.discovers_shared_root()
                && !AppTarget::Cursor.supports_native_activation()
        );
        assert!(
            AppTarget::Claude.supports_native_activation()
                && !AppTarget::Claude.discovers_shared_root()
        );
    }
}
