//! Pre-uninstall snapshots under `~/.vibebar/skill_backups/` — the native
//! `SkillBackupManager` layout: one directory per backup named
//! `<yyyyMMdd_HHmmss>_<skill>` (a numeric suffix when that exists),
//! holding `skill/` (a verbatim copy of the SSOT directory) and
//! `meta.json` (the registry record, when it was taken, where from). The
//! newest twenty are kept.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::catalog;
use super::registry::{now_apple_seconds, Skill};
use super::sync;
use super::validator;
use super::SkillError;

pub const DEFAULT_RETAINED: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub skill: Skill,
    pub backup_created_at: f64,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// The backup directory.
    pub path: String,
    pub directory_name: String,
    pub skill_name: String,
    pub created_at: f64,
    /// The record, when `meta.json` decoded; a backup without it is
    /// listed but cannot be restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<Skill>,
}

pub fn root(vibebar_dir: &Path) -> PathBuf {
    vibebar_dir.join("skill_backups")
}

pub struct BackupManager {
    home: PathBuf,
    vibebar_dir: PathBuf,
}

impl BackupManager {
    pub fn new(home: impl Into<PathBuf>, vibebar_dir: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            vibebar_dir: vibebar_dir.into(),
        }
    }

    /// Snapshot the SSOT directory of `skill` before it is removed.
    pub fn create_backup(&self, skill: &Skill) -> Result<PathBuf, SkillError> {
        validator::validate(&skill.directory)?;
        let source = catalog::ssot_dir(&self.home).join(&skill.directory);
        if sync::kind(&source) != sync::Kind::Directory {
            return Err(SkillError::SourceDirectoryMissing(skill.directory.clone()));
        }
        let root = self.fenced_root()?;
        std::fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
        }
        let created_at = now_apple_seconds();
        let backup = self.unique_backup_path(&skill.directory, created_at, &root);
        std::fs::create_dir(&backup)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o700));
        }
        copy_tree(&source, &backup.join("skill"))?;
        let metadata = Metadata {
            skill: skill.clone(),
            backup_created_at: created_at,
            source_path: source.display().to_string(),
        };
        let value = serde_json::to_value(&metadata).map_err(|e| SkillError::Io(e.to_string()))?;
        let bytes =
            crate::shared::settings_document::to_bytes(value.as_object().expect("an object"))
                .map_err(|e| SkillError::Io(e.to_string()))?;
        crate::shared::write_atomic(&backup.join("meta.json"), &bytes)
            .map_err(|e| SkillError::Io(e.to_string()))?;
        self.prune(DEFAULT_RETAINED);
        Ok(backup)
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Backup> {
        let Ok(root) = self.fenced_root() else {
            return Vec::new();
        };
        let mut backups: Vec<Backup> = std::fs::read_dir(&root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        if sync::kind(&path) != sync::Kind::Directory {
                            return None;
                        }
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') {
                            return None;
                        }
                        let meta = read_metadata(&path);
                        let directory_name = meta
                            .as_ref()
                            .map(|m| m.skill.directory.clone())
                            .or_else(|| name.splitn(3, '_').nth(2).map(str::to_string))
                            .unwrap_or_else(|| name.clone());
                        let created_at =
                            meta.as_ref()
                                .map(|m| m.backup_created_at)
                                .unwrap_or_else(|| {
                                    std::fs::metadata(&path)
                                        .and_then(|m| m.modified())
                                        .ok()
                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                        .map(|d| {
                                            crate::shared::unix_to_apple_seconds(d.as_secs_f64())
                                        })
                                        .unwrap_or(0.0)
                                });
                        Some(Backup {
                            path: path.display().to_string(),
                            skill_name: meta
                                .as_ref()
                                .map(|m| m.skill.name.clone())
                                .unwrap_or_else(|| directory_name.clone()),
                            directory_name,
                            created_at,
                            skill: meta.map(|m| m.skill),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        backups.sort_by(|a, b| {
            b.created_at
                .partial_cmp(&a.created_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        backups
    }

    /// Put the snapshot back into the SSOT. The destination must be missing:
    /// a restore never overwrites a skill that is installed now.
    pub fn restore(&self, backup: &Path) -> Result<Skill, SkillError> {
        let root = catalog::lexical_normalize(&self.fenced_root()?);
        let backup = catalog::lexical_normalize(backup);
        if !backup.starts_with(&root) || backup == root {
            return Err(SkillError::BackupNotFound(backup.display().to_string()));
        }
        if sync::kind(&backup) != sync::Kind::Directory {
            return Err(SkillError::BackupNotFound(backup.display().to_string()));
        }
        let meta = read_metadata(&backup)
            .ok_or_else(|| SkillError::BackupCorrupted(backup.display().to_string()))?;
        validator::validate(&meta.skill.directory)?;
        let source = backup.join("skill");
        if sync::kind(&source) != sync::Kind::Directory || !source.join("SKILL.md").is_file() {
            return Err(SkillError::BackupCorrupted(backup.display().to_string()));
        }
        let destination = catalog::ssot_dir(&self.home).join(&meta.skill.directory);
        if !catalog::is_write_allowed(&destination, &self.home) {
            return Err(SkillError::WriteOutsideAllowedRoots(
                destination.display().to_string(),
            ));
        }
        if sync::kind(&destination) != sync::Kind::Missing {
            return Err(SkillError::DestinationExists(meta.skill.directory.clone()));
        }
        sync::ensure_directory(&catalog::ssot_dir(&self.home), &self.home)?;
        copy_tree(&source, &destination)?;
        Ok(meta.skill)
    }

    pub fn prune(&self, keeping: usize) {
        let backups = self.list();
        for backup in backups.into_iter().skip(keeping) {
            let _ = sync::remove_tree_without_following_links(Path::new(&backup.path));
        }
    }

    /// The backup root, refused when it is a symlink or sits behind one:
    /// pruning deletes below it, and a link would carry that elsewhere.
    fn fenced_root(&self) -> Result<PathBuf, SkillError> {
        let root = root(&self.vibebar_dir);
        let ok = matches!(
            sync::kind(&root),
            sync::Kind::Missing | sync::Kind::Directory
        ) && !sync::has_symlinked_ancestor(&root, &self.vibebar_dir);
        if !ok {
            return Err(SkillError::WriteOutsideAllowedRoots(
                root.display().to_string(),
            ));
        }
        Ok(root)
    }

    fn unique_backup_path(&self, name: &str, created_at: f64, root: &Path) -> PathBuf {
        let stamp = stamp(created_at);
        let base = format!("{stamp}_{name}");
        let mut candidate = root.join(&base);
        let mut suffix = 2;
        while sync::kind(&candidate) != sync::Kind::Missing {
            candidate = root.join(format!("{base}_{suffix}"));
            suffix += 1;
        }
        candidate
    }
}

fn read_metadata(backup: &Path) -> Option<Metadata> {
    let bytes = std::fs::read(backup.join("meta.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `yyyyMMdd_HHmmss` in local time, from seconds since 2001.
fn stamp(apple_seconds: f64) -> String {
    use chrono::TimeZone;
    let unix = apple_seconds + 978_307_200.0;
    let local = chrono::Local.timestamp_opt(unix as i64, 0).single();
    local
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| "00000000_000000".to_string())
}

fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&from)?;
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&from)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &to)?;
            // Recreating a link needs a privilege Windows does not grant by
            // default, and a copy that quietly drops one is a backup that
            // cannot be restored. Say so instead.
            #[cfg(not(unix))]
            return Err(std::io::Error::other(format!(
                "{} contains a symbolic link this platform cannot copy",
                from.display()
            )));
        } else if meta.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::registry::SkillId;

    fn skill(name: &str) -> Skill {
        Skill {
            id: SkillId::Local {
                directory: name.into(),
            },
            name: name.into(),
            description: None,
            directory: name.into(),
            repo_branch: None,
            installed_at: 780_000_000.0,
            content_hash: None,
            updated_at: None,
            apps: Default::default(),
        }
    }

    #[test]
    fn a_backup_is_a_copy_plus_meta_and_restores_when_the_ssot_slot_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let vibebar = home.join(".vibebar");
        let ssot = catalog::ssot_dir(home).join("docx");
        std::fs::create_dir_all(&ssot).unwrap();
        std::fs::write(ssot.join("SKILL.md"), "---\nname: docx\n---\n").unwrap();
        let manager = BackupManager::new(home, &vibebar);
        let backup = manager.create_backup(&skill("docx")).unwrap();
        assert!(backup.join("skill/SKILL.md").is_file());
        assert!(backup.join("meta.json").is_file());
        let listed = manager.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].directory_name, "docx");
        assert_eq!(
            manager.restore(&backup),
            Err(SkillError::DestinationExists("docx".into()))
        );
        sync::remove_tree_without_following_links(&ssot).unwrap();
        let restored = manager.restore(&backup).unwrap();
        assert_eq!(restored.directory, "docx");
        assert!(ssot.join("SKILL.md").is_file());
    }

    #[test]
    fn a_backup_outside_the_root_is_not_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let manager = BackupManager::new(dir.path(), dir.path().join(".vibebar"));
        assert!(matches!(
            manager.restore(&dir.path().join("elsewhere")),
            Err(SkillError::BackupNotFound(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_backup_root_is_neither_listed_nor_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let elsewhere = dir.path().join("elsewhere");
        for i in 0..25 {
            std::fs::create_dir_all(elsewhere.join(format!("20260101_0000{i:02}_docx/skill")))
                .unwrap();
        }
        std::os::unix::fs::symlink(&elsewhere, root(&vibebar)).unwrap();
        let manager = BackupManager::new(dir.path().to_path_buf(), vibebar);
        assert!(manager.list().is_empty());
        manager.prune(20);
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 25);
        assert!(matches!(
            manager.restore(&elsewhere.join("20260101_000000_docx")),
            Err(SkillError::WriteOutsideAllowedRoots(_))
        ));
    }
}
