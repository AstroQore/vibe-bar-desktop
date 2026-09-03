//! The operations the Skills page performs — the native `SkillsService`'s
//! install, uninstall, projection toggles, adoption of a folder someone
//! else put in the SSOT, and backup restore. Each one re-reads the registry
//! immediately before it writes it, and every path it touches goes through
//! the sync engine's fence.
//!
//! Not here yet: patching a harness's own config for native activation
//! (`config.toml`, `settings.json`) and repository-backed installs; the
//! native app does those, and the page says so.

use std::path::{Path, PathBuf};

use super::backups::{Backup, BackupManager};
use super::catalog::{self, AppTarget};
use super::hasher;
use super::registry::{self, now_apple_seconds, Registry, Skill, SkillId, SyncMethod};
use super::sync::{self, Kind, SyncEngine};
use super::validator;
use super::SkillError;

pub struct SkillsService {
    home: PathBuf,
    vibebar_dir: PathBuf,
    engine: SyncEngine,
    backups: BackupManager,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub backup_path: String,
    /// Per app: whether its entry was actually removed. `false` means it was
    /// left alone on purpose — foreign, or a copy the user edited.
    pub removed_by_app: std::collections::BTreeMap<String, bool>,
}

impl SkillsService {
    pub fn new(home: impl Into<PathBuf>, vibebar_dir: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let vibebar_dir = vibebar_dir.into();
        Self {
            engine: SyncEngine::new(home.clone()),
            backups: BackupManager::new(home.clone(), vibebar_dir.clone()),
            home,
            vibebar_dir,
        }
    }

    pub fn registry(&self) -> Result<Registry, SkillError> {
        registry::read(&self.vibebar_dir)
    }

    pub fn skill(&self, id: &SkillId) -> Result<Option<Skill>, SkillError> {
        Ok(self
            .registry()?
            .skills
            .into_iter()
            .find(|skill| &skill.id == id))
    }

    /// Install a skill from a folder on disk: copied into the SSOT under
    /// `name`, recorded, and projected into `enable_for`.
    pub fn install_local(
        &self,
        source: &Path,
        name: &str,
        enable_for: &[AppTarget],
    ) -> Result<Skill, SkillError> {
        validator::validate(name)?;
        if sync::kind(source) != Kind::Directory {
            return Err(SkillError::SourceNotADirectory(
                source.display().to_string(),
            ));
        }
        if !source.join("SKILL.md").is_file() {
            return Err(SkillError::MissingSkillMd(name.to_string()));
        }
        // Preflight before the first mutation: the registry must be one this
        // build may write, and every slot must be free (or already ours), so a
        // conflict surfaces before anything is copied rather than after.
        self.registry()?;
        for app in enable_for {
            self.engine.preflight(name, *app)?;
        }
        self.copy_into_ssot(source, name)?;
        let mut skill = self.make_local_skill(name)?;
        for app in enable_for {
            match self.engine.materialize(name, *app, SyncMethod::Auto, None) {
                Ok(materialization) => {
                    skill.apps.insert(app.raw().to_string(), materialization);
                }
                Err(error) => {
                    self.roll_back_install(&skill);
                    return Err(error);
                }
            }
        }
        if let Err(error) = self.upsert(skill.clone()) {
            self.roll_back_install(&skill);
            return Err(error);
        }
        Ok(skill)
    }

    /// Undo an install that failed part-way: take back the projections it
    /// made (each through the same ownership checks as a toggle) and the
    /// SSOT copy, so nothing unrecorded is left behind. Best effort — the
    /// error that triggered it is the one the caller sees.
    fn roll_back_install(&self, skill: &Skill) {
        self.roll_back_projections(skill, &[]);
        let _ = self.remove_from_ssot(&skill.directory);
    }

    /// Take back the projections this call made, leaving alone any whose app
    /// name is in `keep` — the ones that were already there. Best effort: the
    /// error that triggered the rollback is the one the caller sees.
    fn roll_back_projections(&self, skill: &Skill, keep: &[String]) {
        for app in skill.projected_apps() {
            if keep.iter().any(|raw| raw == app.raw()) {
                continue;
            }
            let _ = self
                .engine
                .unmaterialize(&skill.directory, app, skill.materialization(app));
        }
    }

    /// Adopt a folder that is already in the SSOT (put there by hand, by
    /// another tool, or by the native app before this registry knew it),
    /// recording it and any symlinks into it that apps already have.
    pub fn adopt_existing(
        &self,
        name: &str,
        enable_for: &[AppTarget],
    ) -> Result<Skill, SkillError> {
        validator::validate(name)?;
        let source = catalog::ssot_dir(&self.home).join(name);
        // Recording a skill means uninstall may delete it later: the tree
        // has to pass the same fence a deletion would.
        sync::check_write_fence(&source, &self.home)?;
        if sync::kind(&source) != Kind::Directory {
            return Err(SkillError::SourceDirectoryMissing(name.to_string()));
        }
        if !source.join("SKILL.md").is_file() {
            return Err(SkillError::MissingSkillMd(name.to_string()));
        }
        // A skill the third-party installer put here came from a repository,
        // and the lock file is the only place that says which. Adopting it as
        // `local:<dir>` would tell the native app it is a hand-made folder.
        // Same preflight as an install: the registry must be one this build
        // may write, and every requested slot free, before anything is made.
        self.registry()?;
        for app in enable_for {
            self.engine.preflight(name, *app)?;
        }
        let mut skill = self.make_local_skill(name)?;
        let provenance = super::lock::LockFile::read(&self.home).provenance(name);
        skill.id = provenance.id;
        skill.repo_branch = provenance.branch;
        if let Some(installed_at) = provenance.installed_at {
            skill.installed_at = installed_at;
        }
        skill.updated_at = provenance.updated_at;
        for app in AppTarget::ALL {
            if let Some(adopted) = self.engine.adoption_state(name, app) {
                skill.apps.insert(app.raw().to_string(), adopted);
            }
        }
        // Only what this call creates is rolled back: a projection the person
        // already had was adopted above, not made here, and taking it away
        // would be a change they did not ask for.
        let adopted: Vec<String> = skill.apps.keys().cloned().collect();
        for app in enable_for {
            if skill.apps.contains_key(app.raw()) {
                continue;
            }
            match self.engine.materialize(name, *app, SyncMethod::Auto, None) {
                Ok(materialization) => {
                    skill.apps.insert(app.raw().to_string(), materialization);
                }
                Err(error) => {
                    self.roll_back_projections(&skill, &adopted);
                    return Err(error);
                }
            }
        }
        if let Err(error) = self.upsert(skill.clone()) {
            self.roll_back_projections(&skill, &adopted);
            return Err(error);
        }
        Ok(skill)
    }

    /// Project a skill into an app, or take it out. Returns whether the
    /// app-side entry changed.
    pub fn set_projection(
        &self,
        id: &SkillId,
        app: AppTarget,
        on: bool,
    ) -> Result<bool, SkillError> {
        let mut skill = self
            .skill(id)?
            .ok_or_else(|| SkillError::NotInstalled(id.raw()))?;
        let recorded = skill.materialization(app).cloned();
        if on {
            let materialization = self.engine.materialize(
                &skill.directory,
                app,
                SyncMethod::Auto,
                recorded.as_ref(),
            )?;
            skill.apps.insert(app.raw().to_string(), materialization);
            self.upsert(skill)?;
            Ok(true)
        } else {
            let removed = self
                .engine
                .unmaterialize(&skill.directory, app, recorded.as_ref())?;
            if removed {
                skill.apps.remove(app.raw());
            }
            self.upsert(skill)?;
            Ok(removed)
        }
    }

    /// Snapshot, take the skill out of every app it was projected into,
    /// remove it from the SSOT, forget it.
    pub fn uninstall(&self, id: &SkillId) -> Result<UninstallResult, SkillError> {
        let skill = self
            .skill(id)?
            .ok_or_else(|| SkillError::NotInstalled(id.raw()))?;
        let backup = self.backups.create_backup(&skill)?;
        let mut removed_by_app = std::collections::BTreeMap::new();
        for app in skill.projected_apps() {
            let removed =
                self.engine
                    .unmaterialize(&skill.directory, app, skill.materialization(app))?;
            removed_by_app.insert(app.raw().to_string(), removed);
        }
        self.remove_from_ssot(&skill.directory)?;
        let mut registry = self.registry()?;
        registry.skills.retain(|entry| entry.id != skill.id);
        registry::write(&self.vibebar_dir, &registry)?;
        Ok(UninstallResult {
            backup_path: backup.display().to_string(),
            removed_by_app,
        })
    }

    pub fn backups(&self) -> Vec<Backup> {
        self.backups.list()
    }

    /// Put a snapshot back and record it again; projections are restored
    /// where the snapshot recorded them.
    pub fn restore_backup(&self, backup: &Path) -> Result<Skill, SkillError> {
        // The registry has to be one this build may write before a single
        // file moves; otherwise a refused write would leave an unrecorded
        // skill projected into the apps.
        self.registry()?;
        let mut skill = self.backups.restore(backup)?;
        // An old snapshot can name Hermes or OpenCode. Those roots exist here
        // only so such a record decodes and can be cleaned up; restoring must
        // not put a skill back into one.
        let apps: Vec<AppTarget> = skill
            .projected_apps()
            .into_iter()
            .filter(|app| AppTarget::MANAGED.contains(app))
            .collect();
        skill.apps.clear();
        for app in apps {
            match self
                .engine
                .materialize(&skill.directory, app, SyncMethod::Auto, None)
            {
                Ok(materialization) => {
                    skill.apps.insert(app.raw().to_string(), materialization);
                }
                // A slot something else already occupies is not a failure of
                // the restore; anything else is, and leaves nothing behind.
                Err(SkillError::DirectoryConflict(_)) => {}
                Err(error) => {
                    self.roll_back_install(&skill);
                    return Err(error);
                }
            }
        }
        if let Err(error) = self.upsert(skill.clone()) {
            self.roll_back_install(&skill);
            return Err(error);
        }
        Ok(skill)
    }

    fn make_local_skill(&self, name: &str) -> Result<Skill, SkillError> {
        let directory = catalog::ssot_dir(&self.home).join(name);
        let (title, description) =
            super::inventory::frontmatter_of(&directory.join("SKILL.md"), name);
        Ok(Skill {
            id: SkillId::Local {
                directory: name.to_string(),
            },
            name: title,
            description,
            directory: name.to_string(),
            repo_branch: None,
            installed_at: now_apple_seconds(),
            content_hash: hasher::hash(&directory).ok(),
            updated_at: None,
            apps: Default::default(),
        })
    }

    fn copy_into_ssot(&self, source: &Path, name: &str) -> Result<(), SkillError> {
        let ssot = catalog::ssot_dir(&self.home);
        let destination = ssot.join(name);
        sync::check_write_fence(&destination, &self.home)?;
        if sync::kind(&destination) != Kind::Missing {
            return Err(SkillError::DirectoryConflict(name.to_string()));
        }
        sync::ensure_directory(&ssot, &self.home)?;
        let staging = ssot.join(format!(".{name}.vibebar-{}", std::process::id()));
        if sync::kind(&staging) != Kind::Missing {
            sync::remove_tree_without_following_links(&staging)?;
        }
        sync::copy_tree(source, &staging)?;
        std::fs::rename(&staging, &destination)?;
        Ok(())
    }

    fn remove_from_ssot(&self, name: &str) -> Result<(), SkillError> {
        validator::validate(name)?;
        let path = catalog::ssot_dir(&self.home).join(name);
        sync::check_write_fence(&path, &self.home)?;
        match sync::kind(&path) {
            Kind::Missing => Ok(()),
            Kind::Directory => Ok(sync::remove_tree_without_following_links(&path)?),
            // A link in the SSOT is unlinked, never followed.
            Kind::Symlink => Ok(sync::remove_link(&path)?),
            _ => Err(SkillError::SourceNotADirectory(name.to_string())),
        }
    }

    fn upsert(&self, skill: Skill) -> Result<(), SkillError> {
        let mut registry = self.registry()?;
        match registry
            .skills
            .iter_mut()
            .find(|entry| entry.id == skill.id)
        {
            Some(existing) => *existing = skill,
            None => registry.skills.push(skill),
        }
        registry::write(&self.vibebar_dir, &registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, SkillsService, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let source = dir.path().join("downloads/docx");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: Docx tools\ndescription: Word documents.\n---\nbody",
        )
        .unwrap();
        let service = SkillsService::new(&home, home.join(".vibebar"));
        (dir, service, source)
    }

    #[test]
    fn install_projects_records_and_uninstall_takes_it_all_back_with_a_backup() {
        let (dir, service, source) = setup();
        let skill = service
            .install_local(&source, "docx", &[AppTarget::Codex, AppTarget::Claude])
            .unwrap();
        assert_eq!(skill.name, "Docx tools");
        assert_eq!(skill.description.as_deref(), Some("Word documents."));
        assert!(catalog::ssot_dir(dir.path())
            .join("docx/SKILL.md")
            .is_file());
        assert_eq!(
            sync::kind(&catalog::skills_dir(AppTarget::Codex, dir.path()).join("docx")),
            Kind::Symlink
        );
        let registry = service.registry().unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert_eq!(registry.skills[0].projected_apps().len(), 2);
        let result = service.uninstall(&skill.id).unwrap();
        assert_eq!(result.removed_by_app.values().filter(|v| **v).count(), 2);
        assert!(!catalog::ssot_dir(dir.path()).join("docx").exists());
        assert!(Path::new(&result.backup_path)
            .join("skill/SKILL.md")
            .is_file());
        assert!(service.registry().unwrap().skills.is_empty());
        // And back again.
        let restored = service
            .restore_backup(Path::new(&result.backup_path))
            .unwrap();
        assert_eq!(restored.projected_apps().len(), 2);
        assert!(catalog::ssot_dir(dir.path())
            .join("docx/SKILL.md")
            .is_file());
    }

    #[test]
    fn projection_toggles_leave_foreign_entries_alone() {
        let (dir, service, source) = setup();
        let skill = service.install_local(&source, "docx", &[]).unwrap();
        assert!(service
            .set_projection(&skill.id, AppTarget::Gemini, true)
            .unwrap());
        assert!(service
            .set_projection(&skill.id, AppTarget::Gemini, false)
            .unwrap());
        // Someone else's directory under the same name is never removed.
        let foreign = catalog::skills_dir(AppTarget::Grok, dir.path()).join("docx");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("SKILL.md"), "theirs").unwrap();
        assert_eq!(
            service.set_projection(&skill.id, AppTarget::Grok, true),
            Err(SkillError::DirectoryConflict("docx".into()))
        );
        assert!(!service
            .set_projection(&skill.id, AppTarget::Grok, false)
            .unwrap());
        assert!(foreign.join("SKILL.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn installing_over_an_existing_ssot_folder_is_refused_and_adoption_records_it() {
        let (dir, service, source) = setup();
        service.install_local(&source, "docx", &[]).unwrap();
        assert_eq!(
            service.install_local(&source, "docx", &[]),
            Err(SkillError::DirectoryConflict("docx".into()))
        );
        // A folder that appeared in the SSOT on its own, with a link an app already has.
        let handmade = catalog::ssot_dir(dir.path()).join("notes");
        std::fs::create_dir_all(&handmade).unwrap();
        std::fs::write(handmade.join("SKILL.md"), "---\nname: Notes\n---\n").unwrap();
        let link = catalog::skills_dir(AppTarget::Cursor, dir.path()).join("notes");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&handmade, &link).unwrap();
        let adopted = service
            .adopt_existing("notes", &[AppTarget::Codex])
            .unwrap();
        assert!(adopted.materialization(AppTarget::Cursor).unwrap().adopted);
        assert_eq!(
            adopted.materialization(AppTarget::Codex).unwrap().method,
            SyncMethod::Symlink
        );
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_never_follows_a_symlinked_agents_dir() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        let skill = outside.join("skills/docx");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        std::os::unix::fs::symlink(&outside, dir.path().join(".agents")).unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar);
        let id = SkillId::Local {
            directory: "docx".into(),
        };
        // Adoption is refused up front, and a registry entry that predates
        // the link still cannot delete through it.
        assert!(matches!(
            service.adopt_existing("docx", &[]),
            Err(SkillError::WriteOutsideAllowedRoots(_))
        ));
        service
            .upsert(service.make_local_skill("docx").unwrap())
            .unwrap();
        assert!(matches!(
            service.uninstall(&id),
            Err(SkillError::WriteOutsideAllowedRoots(_))
        ));
        assert!(skill.join("SKILL.md").is_file());
    }

    #[test]
    fn install_leaves_nothing_behind_when_a_slot_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("incoming/docx");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        // Claude already has a foreign docx directory.
        let taken = catalog::skills_dir(AppTarget::Claude, dir.path()).join("docx");
        std::fs::create_dir_all(&taken).unwrap();
        std::fs::write(taken.join("SKILL.md"), "theirs").unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar.clone());
        assert!(matches!(
            service.install_local(&source, "docx", &[AppTarget::Codex, AppTarget::Claude]),
            Err(SkillError::DirectoryConflict(_))
        ));
        assert_eq!(
            sync::kind(&catalog::ssot_dir(dir.path()).join("docx")),
            Kind::Missing
        );
        assert_eq!(
            sync::kind(&catalog::skills_dir(AppTarget::Codex, dir.path()).join("docx")),
            Kind::Missing
        );
        assert_eq!(
            std::fs::read_to_string(taken.join("SKILL.md")).unwrap(),
            "theirs"
        );
        assert!(service.registry().unwrap().skills.is_empty());
        // A registry this build may not write stops the install before any copy.
        std::fs::write(
            vibebar.join("skills.json"),
            r#"{"schemaVersion":2,"skills":[]}"#,
        )
        .unwrap();
        std::fs::remove_dir_all(&taken).unwrap();
        assert!(matches!(
            service.install_local(&source, "docx", &[AppTarget::Codex]),
            Err(SkillError::UnsupportedRegistrySchema(2))
        ));
        assert_eq!(
            sync::kind(&catalog::ssot_dir(dir.path()).join("docx")),
            Kind::Missing
        );
    }

    #[test]
    fn adoption_keeps_the_repository_the_lock_file_records() {
        let dir = tempfile::tempdir().unwrap();
        let skill = catalog::ssot_dir(dir.path()).join("docx");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        std::fs::write(
            dir.path().join(super::super::lock::RELATIVE_PATH),
            r#"{"version":1,"skills":{"docx":{"source":"AstroQore/skills","sourceType":"github",
                 "branch":"main","installedAt":"2026-08-01T00:00:00Z"}}}"#,
        )
        .unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar);
        let adopted = service.adopt_existing("docx", &[]).unwrap();
        assert_eq!(adopted.id.raw(), "AstroQore/skills:docx");
        assert_eq!(adopted.repo_branch.as_deref(), Some("main"));
        assert_eq!(adopted.installed_at, 807_235_200.0);
        // And the registry keeps it, so the native app still sees a repo skill.
        let stored = service.skill(&adopted.id).unwrap().unwrap();
        assert_eq!(stored.id.raw(), "AstroQore/skills:docx");
    }

    #[test]
    fn a_registry_this_build_cannot_write_stops_a_restore_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("incoming/docx");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar.clone());
        let skill = service
            .install_local(&source, "docx", &[AppTarget::Codex])
            .unwrap();
        let backup = service.uninstall(&skill.id).unwrap().backup_path;
        std::fs::write(
            vibebar.join("skills.json"),
            r#"{"schemaVersion":1,"skills":"not an array"}"#,
        )
        .unwrap();
        assert!(matches!(
            service.restore_backup(Path::new(&backup)),
            Err(SkillError::MalformedRegistry(_))
        ));
        // Nothing was put back: the store said unavailable, and it meant it.
        assert_eq!(
            sync::kind(&catalog::ssot_dir(dir.path()).join("docx")),
            Kind::Missing
        );
        assert_eq!(
            sync::kind(&catalog::skills_dir(AppTarget::Codex, dir.path()).join("docx")),
            Kind::Missing
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_restore_that_cannot_finish_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("incoming/docx");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar);
        let skill = service
            .install_local(&source, "docx", &[AppTarget::Codex, AppTarget::Claude])
            .unwrap();
        let backup = service.uninstall(&skill.id).unwrap().backup_path;
        // Claude's skills root becomes a file, so creating its directory —
        // and therefore that projection — fails for a reason that is not a
        // slot conflict.
        let claude_root = catalog::skills_dir(AppTarget::Claude, dir.path());
        let _ = std::fs::remove_dir_all(&claude_root);
        std::fs::create_dir_all(claude_root.parent().unwrap()).unwrap();
        std::fs::write(&claude_root, "not a directory").unwrap();
        assert!(service.restore_backup(Path::new(&backup)).is_err());
        assert_eq!(
            sync::kind(&catalog::ssot_dir(dir.path()).join("docx")),
            Kind::Missing
        );
        assert_eq!(
            sync::kind(&catalog::skills_dir(AppTarget::Codex, dir.path()).join("docx")),
            Kind::Missing
        );
        assert!(service.registry().unwrap().skills.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn adoption_that_cannot_finish_leaves_only_what_was_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let ssot = catalog::ssot_dir(dir.path()).join("docx");
        std::fs::create_dir_all(&ssot).unwrap();
        std::fs::write(ssot.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        // Codex already links to it; Gemini's root is a file, so projecting
        // there fails for a reason that is not a slot conflict.
        let codex = catalog::skills_dir(AppTarget::Codex, dir.path());
        std::fs::create_dir_all(&codex).unwrap();
        std::os::unix::fs::symlink(&ssot, codex.join("docx")).unwrap();
        let gemini = catalog::skills_dir(AppTarget::Gemini, dir.path());
        std::fs::create_dir_all(gemini.parent().unwrap()).unwrap();
        std::fs::write(&gemini, "not a directory").unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        let service = SkillsService::new(dir.path().to_path_buf(), vibebar);
        assert!(service
            .adopt_existing("docx", &[AppTarget::Claude, AppTarget::Gemini])
            .is_err());
        // The link that was already there stays; the one this call made does
        // not; the SSOT folder is untouched, since adoption never made it.
        assert_eq!(sync::kind(&codex.join("docx")), Kind::Symlink);
        assert_eq!(
            sync::kind(&catalog::skills_dir(AppTarget::Claude, dir.path()).join("docx")),
            Kind::Missing
        );
        assert_eq!(sync::kind(&ssot), Kind::Directory);
        assert!(service.registry().unwrap().skills.is_empty());
    }
}
