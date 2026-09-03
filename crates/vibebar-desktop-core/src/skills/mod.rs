//! The Skills manager: the shared skill library at `~/.agents/skills` (the
//! SSOT), projected into each agent CLI's skills directory, with the
//! registry at `~/.vibebar/skills.json` and pre-uninstall snapshots under
//! `~/.vibebar/skill_backups/`.
//!
//! The native app's `SkillSyncEngine` / `SkillsService` rules, verbatim:
//! a skill name is one safe path segment; every path mutated sits under
//! the SSOT or an allow-listed app skills directory; a sync needs a
//! `SKILL.md`; ancestors are created one component at a time; deletion
//! never follows a symlink and removes only links that resolve back into
//! the SSOT or copies whose recorded hash still matches, so a folder the
//! user authored or edited is left in place. `~/.agents/.skill-lock.json`
//! is read for provenance and never written.

pub mod backups;
pub mod catalog;
pub mod hasher;
pub mod inventory;
pub mod lock;
pub mod registry;
pub mod service;
pub mod sync;
pub mod validator;

pub use inventory::{scan, SkillInventoryRow, SkillsInventoryView};

/// What the skills layer refuses, in the native `SkillError`'s terms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    #[error("{0:?} is not a valid skill directory name")]
    InvalidDirectoryName(String),
    #[error("{0} has no SKILL.md")]
    MissingSkillMd(String),
    #[error("{0} already exists and is not something Vibe Bar wrote")]
    DirectoryConflict(String),
    #[error("{0} is in the way of a skills directory")]
    AppDirectoryUnusable(String),
    #[error("{0} is not installed")]
    NotInstalled(String),
    #[error("refusing to write outside the skills roots: {0}")]
    WriteOutsideAllowedRoots(String),
    #[error("the source directory {0} is missing")]
    SourceDirectoryMissing(String),
    #[error("{0} is not a directory")]
    SourceNotADirectory(String),
    #[error("{0} already exists")]
    DestinationExists(String),
    #[error("the backup {0} was not found")]
    BackupNotFound(String),
    #[error("the backup {0} is unreadable")]
    BackupCorrupted(String),
    #[error("skills.json is schema {0}; this build understands schema 1 and will not rewrite it")]
    UnsupportedRegistrySchema(u32),
    #[error("skills.json is not readable as a registry: {0}")]
    MalformedRegistry(String),
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for SkillError {
    fn from(error: std::io::Error) -> Self {
        SkillError::Io(error.to_string())
    }
}
