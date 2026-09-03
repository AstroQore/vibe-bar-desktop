//! Projects a skill from the SSOT into an app's skills directory and takes
//! it back out — the native `SkillSyncEngine`. Two invariants hold for
//! every mutation: the name passed the validator and the resolved path
//! sits under an allowed root; deletion never follows a symlink and never
//! removes anything the user could have authored — a foreign directory,
//! or a copy whose content drifted from the hash recorded when it was made.

use std::path::{Path, PathBuf};

use super::catalog::{self, AppTarget};
use super::hasher;
use super::registry::{Materialization, SyncMethod};
use super::validator;
use super::SkillError;

/// lstat-level classification: everything here tells a symlink apart from
/// the directory it points at, and never follows one while deleting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Missing,
    Directory,
    Symlink,
    RegularFile,
    Other,
}

pub fn kind(path: &Path) -> Kind {
    match std::fs::symlink_metadata(path) {
        Err(_) => Kind::Missing,
        Ok(meta) if meta.file_type().is_symlink() => Kind::Symlink,
        Ok(meta) if meta.is_dir() => Kind::Directory,
        Ok(meta) if meta.is_file() => Kind::RegularFile,
        Ok(_) => Kind::Other,
    }
}

/// The write fence every mutation passes: `path` sits lexically under an
/// allowed root, and no component between `home` and `path` is a symlink —
/// otherwise a dotfile-managed `~/.agents` would let a delete or a copy
/// land wherever that link points.
pub fn check_write_fence(path: &Path, home: &Path) -> Result<(), SkillError> {
    if !catalog::is_write_allowed(path, home) || has_symlinked_ancestor(path, home) {
        return Err(SkillError::WriteOutsideAllowedRoots(
            path.display().to_string(),
        ));
    }
    Ok(())
}

/// Whether any ancestor of `path` strictly between `home` and `path` is a
/// symlink (lstat; nothing is followed). `home` itself is not inspected —
/// a home reached through a link is the person's business.
pub fn has_symlinked_ancestor(path: &Path, home: &Path) -> bool {
    let path = catalog::lexical_normalize(path);
    let home = catalog::lexical_normalize(home);
    path.ancestors()
        .skip(1)
        .take_while(|ancestor| ancestor.starts_with(&home) && *ancestor != home.as_path())
        .any(|ancestor| kind(ancestor) == Kind::Symlink)
}

/// Create `path` and any missing ancestors up to (never above) `root`, one
/// component at a time, so the creation is provably confined to the
/// terminal path — AntiGravity's `~/.gemini/config/skills` is created
/// without ever touching a sibling of `config/`.
pub fn ensure_directory(path: &Path, root: &Path) -> Result<(), SkillError> {
    let path = catalog::lexical_normalize(path);
    let root = catalog::lexical_normalize(root);
    if !path.starts_with(&root) {
        return Err(SkillError::WriteOutsideAllowedRoots(
            path.display().to_string(),
        ));
    }
    let mut cursor = root.clone();
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| SkillError::WriteOutsideAllowedRoots(path.display().to_string()))?;
    for component in relative.components() {
        cursor.push(component);
        match kind(&cursor) {
            Kind::Directory => {}
            Kind::Missing => std::fs::create_dir(&cursor)?,
            // Something that is not a directory sits where one has to be.
            // That is the app's root being unusable, not this skill's slot
            // being taken, and callers tell the two apart.
            _ => {
                return Err(SkillError::AppDirectoryUnusable(
                    cursor.display().to_string(),
                ))
            }
        }
    }
    Ok(())
}

pub struct SyncEngine {
    home: PathBuf,
}

impl SyncEngine {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    pub fn source_directory(&self, name: &str) -> PathBuf {
        catalog::ssot_dir(&self.home).join(name)
    }

    pub fn destination(&self, name: &str, app: AppTarget) -> PathBuf {
        catalog::skills_dir(app, &self.home).join(name)
    }

    /// Materialize `name` into `app` and report what was done. `recorded`
    /// only matters when a real directory already sits at the destination:
    /// it is replaceable when its hash still matches the copy Vibe Bar made
    /// (recorded hash, or byte-identical to the current SSOT content).
    /// Anything else is user data and is a conflict.
    pub fn materialize(
        &self,
        name: &str,
        app: AppTarget,
        method: SyncMethod,
        recorded: Option<&Materialization>,
    ) -> Result<Materialization, SkillError> {
        validator::validate(name)?;
        let source = self.source_directory(name);
        match kind(&source) {
            Kind::Directory => {}
            Kind::Missing => return Err(SkillError::SourceDirectoryMissing(name.to_string())),
            _ => return Err(SkillError::SourceNotADirectory(name.to_string())),
        }
        if !source.join("SKILL.md").is_file() {
            return Err(SkillError::MissingSkillMd(name.to_string()));
        }
        let app_directory = catalog::skills_dir(app, &self.home);
        let destination = self.destination(name, app);
        check_write_fence(&destination, &self.home)?;
        ensure_directory(&app_directory, &self.home)?;
        let existing = kind(&destination);
        // A link that does not point back at this skill's SSOT directory is
        // someone else's: it is never replaced, whatever the method.
        if existing == Kind::Symlink && !self.is_our_link(&destination, &source) {
            return Err(SkillError::DirectoryConflict(name.to_string()));
        }
        match method {
            SyncMethod::Auto => match existing {
                Kind::Missing => self
                    .link(&source, &destination)
                    .or_else(|_| self.copy(&source, &destination)),
                Kind::Symlink => {
                    remove_link(&destination)?;
                    self.link(&source, &destination)
                }
                Kind::Directory => {
                    if !self.is_vibebar_copy(&destination, recorded)? {
                        return Err(SkillError::DirectoryConflict(name.to_string()));
                    }
                    self.copy(&source, &destination)
                }
                _ => Err(SkillError::DirectoryConflict(name.to_string())),
            },
            SyncMethod::Symlink => {
                match existing {
                    Kind::Missing => {}
                    Kind::Symlink => remove_link(&destination)?,
                    Kind::Directory => {
                        if !self.is_vibebar_copy(&destination, recorded)? {
                            return Err(SkillError::DirectoryConflict(name.to_string()));
                        }
                        remove_tree_without_following_links(&destination)?;
                    }
                    _ => return Err(SkillError::DirectoryConflict(name.to_string())),
                }
                self.link(&source, &destination)
            }
            SyncMethod::Copy => {
                match existing {
                    Kind::Missing => {}
                    Kind::Symlink => remove_link(&destination)?,
                    Kind::Directory => {
                        if !self.is_vibebar_copy(&destination, recorded)? {
                            return Err(SkillError::DirectoryConflict(name.to_string()));
                        }
                    }
                    _ => return Err(SkillError::DirectoryConflict(name.to_string())),
                }
                self.copy(&source, &destination)
            }
        }
    }

    /// Remove `name` from `app`; `false` means the entry was left alone on
    /// purpose — a foreign directory, a link pointing outside the SSOT, or
    /// a copy the user has since edited.
    pub fn unmaterialize(
        &self,
        name: &str,
        app: AppTarget,
        recorded: Option<&Materialization>,
    ) -> Result<bool, SkillError> {
        validator::validate(name)?;
        let destination = self.destination(name, app);
        check_write_fence(&destination, &self.home)?;
        let source = catalog::lexical_normalize(&self.source_directory(name));
        match kind(&destination) {
            Kind::Missing => Ok(true),
            Kind::Symlink => {
                let Some(resolved) = lexical_symlink_target(&destination) else {
                    return Ok(false);
                };
                if resolved != source {
                    return Ok(false);
                }
                remove_link(&destination)?;
                Ok(true)
            }
            Kind::Directory => {
                // Only the hash recorded when Vibe Bar made the copy says the
                // copy is Vibe Bar's. Matching the SSOT byte for byte does not:
                // a directory someone else put there can look identical.
                let Ok(current) = hasher::hash(&destination) else {
                    return Ok(false);
                };
                let recorded_hash = recorded.and_then(|r| r.content_hash_at_copy.as_deref());
                if recorded_hash != Some(current.as_str()) {
                    return Ok(false);
                }
                remove_tree_without_following_links(&destination)?;
                Ok(true)
            }
            Kind::RegularFile | Kind::Other => Ok(false),
        }
    }

    /// Whether `materialize` could run without displacing anything: the slot
    /// is missing, or already this skill's own link. A directory is a
    /// conflict here because a fresh install has no recorded copy to match.
    pub fn preflight(&self, name: &str, app: AppTarget) -> Result<(), SkillError> {
        validator::validate(name)?;
        let destination = self.destination(name, app);
        check_write_fence(&destination, &self.home)?;
        let source = self.source_directory(name);
        match kind(&destination) {
            Kind::Missing => Ok(()),
            Kind::Symlink if self.is_our_link(&destination, &source) => Ok(()),
            _ => Err(SkillError::DirectoryConflict(name.to_string())),
        }
    }

    /// An existing symlink into the SSOT is a materialization Vibe Bar can
    /// adopt as-is; a real directory is foreign until the user says otherwise.
    pub fn adoption_state(&self, name: &str, app: AppTarget) -> Option<Materialization> {
        if !validator::is_valid(name) {
            return None;
        }
        let destination = self.destination(name, app);
        if kind(&destination) != Kind::Symlink {
            return None;
        }
        let resolved = lexical_symlink_target(&destination)?;
        (resolved == catalog::lexical_normalize(&self.source_directory(name)))
            .then(Materialization::adopted_symlink)
    }

    fn is_our_link(&self, destination: &Path, source: &Path) -> bool {
        lexical_symlink_target(destination).as_deref()
            == Some(catalog::lexical_normalize(source).as_path())
    }

    fn link(&self, source: &Path, destination: &Path) -> Result<Materialization, SkillError> {
        #[cfg(unix)]
        std::os::unix::fs::symlink(catalog::lexical_normalize(source), destination)?;
        #[cfg(not(unix))]
        std::os::windows::fs::symlink_dir(catalog::lexical_normalize(source), destination)?;
        Ok(Materialization::symlink())
    }

    /// Recursive copy that swaps `destination` in as a unit: the tree is
    /// built in a hidden sibling first and only then renamed into place, so
    /// an interrupted copy never leaves a half-written skill where an agent
    /// CLI would read it. `destination` must be missing or a real directory.
    fn copy(&self, source: &Path, destination: &Path) -> Result<Materialization, SkillError> {
        let parent = destination
            .parent()
            .ok_or_else(|| SkillError::Io("destination has no parent".into()))?;
        let name = destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let staging = parent.join(format!(".{name}.vibebar-{}", std::process::id()));
        if kind(&staging) != Kind::Missing {
            remove_tree_without_following_links(&staging)?;
        }
        copy_tree(source, &staging)?;
        if kind(destination) == Kind::Directory {
            let old = parent.join(format!(".{name}.vibebar-old-{}", std::process::id()));
            std::fs::rename(destination, &old)?;
            std::fs::rename(&staging, destination)?;
            let _ = remove_tree_without_following_links(&old);
        } else {
            std::fs::rename(&staging, destination)?;
        }
        let hash = hasher::hash(destination)?;
        Ok(Materialization::copy(hash))
    }

    /// A copied slot is Vibe Bar's only if its hash is the one recorded when
    /// the copy was made — never because it happens to match the SSOT.
    fn is_vibebar_copy(
        &self,
        destination: &Path,
        recorded: Option<&Materialization>,
    ) -> Result<bool, SkillError> {
        let current = hasher::hash(destination)?;
        Ok(recorded.and_then(|r| r.content_hash_at_copy.as_deref()) == Some(current.as_str()))
    }
}

/// A symlink's recorded target resolved lexically — relative targets
/// against the link's own directory, `..` collapsed — without resolving
/// intermediate links or requiring the target to exist.
pub fn lexical_symlink_target(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    let absolute = if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    };
    Some(catalog::lexical_normalize(&absolute))
}

/// Copy a tree, preserving symbolic links rather than following them, and
/// refusing a tree whose links this platform cannot recreate — a copy that
/// silently drops one is a backup that cannot restore what it claims to hold.
pub fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&from)?;
        if meta.file_type().is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&from)?, &to)?;
            // Recreating a link needs a privilege Windows does not grant by
            // default.
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

/// Remove `path` — a directory tree, a link, or a file — without ever
/// following a symlink. The terminal entry is classified first, so a staging
/// path that has become a link to somewhere else is unlinked rather than
/// walked into; inside a real tree a link is unlinked, never descended, and
/// each entry's type comes from the listing.
pub fn remove_tree_without_following_links(path: &Path) -> std::io::Result<()> {
    let dir = match kind(path) {
        Kind::Directory => path,
        Kind::Missing => return Ok(()),
        Kind::Symlink => return remove_link(path),
        _ => return std::fs::remove_file(path),
    };
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            remove_tree_without_following_links(&entry.path())?;
        } else if file_type.is_symlink() {
            remove_link(&entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    std::fs::remove_dir(dir)
}

/// Unlink a symlink itself, never what it points at. Windows keeps directory
/// links apart from file links and refuses `remove_file` on the former with
/// "access denied", so the directory form is tried second there.
pub fn remove_link(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        #[cfg(windows)]
        Err(_) => std::fs::remove_dir(path),
        #[cfg(not(windows))]
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with_skill(name: &str) -> (tempfile::TempDir, SyncEngine) {
        let dir = tempfile::tempdir().unwrap();
        let ssot = catalog::ssot_dir(dir.path()).join(name);
        std::fs::create_dir_all(&ssot).unwrap();
        std::fs::write(ssot.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        let engine = SyncEngine::new(dir.path());
        (dir, engine)
    }

    #[cfg(unix)]
    #[test]
    fn auto_links_and_unmaterialize_removes_only_our_link() {
        let (dir, engine) = home_with_skill("docx");
        let m = engine
            .materialize("docx", AppTarget::Codex, SyncMethod::Auto, None)
            .unwrap();
        assert_eq!(m.method, SyncMethod::Symlink);
        let dest = engine.destination("docx", AppTarget::Codex);
        assert_eq!(kind(&dest), Kind::Symlink);
        // A foreign link is left alone.
        let foreign = engine.destination("other", AppTarget::Codex);
        std::os::unix::fs::symlink(dir.path().join("elsewhere"), &foreign).unwrap();
        assert!(!engine
            .unmaterialize("other", AppTarget::Codex, None)
            .unwrap());
        assert_eq!(kind(&foreign), Kind::Symlink);
        assert!(engine
            .unmaterialize("docx", AppTarget::Codex, None)
            .unwrap());
        assert_eq!(kind(&dest), Kind::Missing);
    }

    #[test]
    fn a_copy_is_replaceable_only_while_its_hash_matches() {
        let (_dir, engine) = home_with_skill("docx");
        let m = engine
            .materialize("docx", AppTarget::Claude, SyncMethod::Copy, None)
            .unwrap();
        assert_eq!(m.method, SyncMethod::Copy);
        let dest = engine.destination("docx", AppTarget::Claude);
        assert_eq!(kind(&dest), Kind::Directory);
        // Untouched: replaceable and removable.
        assert!(engine
            .materialize("docx", AppTarget::Claude, SyncMethod::Copy, Some(&m))
            .is_ok());
        // Edited by the user: a conflict, and never removed.
        std::fs::write(dest.join("SKILL.md"), "edited").unwrap();
        assert_eq!(
            engine.materialize("docx", AppTarget::Claude, SyncMethod::Copy, Some(&m)),
            Err(SkillError::DirectoryConflict("docx".into()))
        );
        assert!(!engine
            .unmaterialize("docx", AppTarget::Claude, Some(&m))
            .unwrap());
        assert!(dest.join("SKILL.md").exists());
    }

    #[test]
    fn a_foreign_directory_is_a_conflict_and_a_missing_skill_md_refuses() {
        let (dir, engine) = home_with_skill("docx");
        let foreign = engine.destination("docx", AppTarget::Gemini);
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("SKILL.md"), "theirs").unwrap();
        assert_eq!(
            engine.materialize("docx", AppTarget::Gemini, SyncMethod::Auto, None),
            Err(SkillError::DirectoryConflict("docx".into()))
        );
        std::fs::remove_file(catalog::ssot_dir(dir.path()).join("docx/SKILL.md")).unwrap();
        assert_eq!(
            engine.materialize("docx", AppTarget::Grok, SyncMethod::Auto, None),
            Err(SkillError::MissingSkillMd("docx".into()))
        );
    }

    #[test]
    fn ensure_directory_walks_one_component_at_a_time_inside_the_home() {
        let dir = tempfile::tempdir().unwrap();
        let target = catalog::skills_dir(AppTarget::Antigravity, dir.path());
        ensure_directory(&target, dir.path()).unwrap();
        assert!(target.is_dir());
        assert!(ensure_directory(Path::new("/tmp/elsewhere"), dir.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_foreign_link_in_the_slot_is_never_replaced() {
        let (dir, engine) = home_with_skill("docx");
        let dest = engine.destination("docx", AppTarget::Codex);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, &dest).unwrap();
        for method in [SyncMethod::Auto, SyncMethod::Symlink, SyncMethod::Copy] {
            assert!(matches!(
                engine.materialize("docx", AppTarget::Codex, method, None),
                Err(SkillError::DirectoryConflict(_))
            ));
        }
        assert_eq!(std::fs::read_link(&dest).unwrap(), elsewhere);
        // Our own link is replaceable.
        std::fs::remove_file(&dest).unwrap();
        engine
            .materialize("docx", AppTarget::Codex, SyncMethod::Symlink, None)
            .unwrap();
        engine
            .materialize("docx", AppTarget::Codex, SyncMethod::Auto, None)
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_ancestor_fails_the_write_fence() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(outside.join("skills/docx")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.path().join(".agents")).unwrap();
        let path = dir.path().join(".agents/skills/docx");
        assert!(has_symlinked_ancestor(&path, dir.path()));
        assert!(matches!(
            check_write_fence(&path, dir.path()),
            Err(SkillError::WriteOutsideAllowedRoots(_))
        ));
        // A real tree passes; the home itself may be reached through a link.
        let real = tempfile::tempdir().unwrap();
        let ssot = real.path().join(".agents/skills/docx");
        std::fs::create_dir_all(&ssot).unwrap();
        assert!(!has_symlinked_ancestor(&ssot, real.path()));
        check_write_fence(&ssot, real.path()).unwrap();
    }

    #[test]
    fn a_copy_without_a_recorded_hash_is_left_alone() {
        let (_dir, engine) = home_with_skill("docx");
        // A directory that happens to match the SSOT byte for byte, but that
        // no record says Vibe Bar made.
        let dest = engine.destination("docx", AppTarget::Codex);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("SKILL.md"), "---\nname: Docx\n---\n").unwrap();
        assert!(!engine
            .unmaterialize("docx", AppTarget::Codex, None)
            .unwrap());
        let unhashed = Materialization {
            content_hash_at_copy: None,
            ..Materialization::symlink()
        };
        assert!(!engine
            .unmaterialize("docx", AppTarget::Codex, Some(&unhashed))
            .unwrap());
        assert!(dest.join("SKILL.md").is_file());
        assert!(matches!(
            engine.materialize("docx", AppTarget::Codex, SyncMethod::Copy, None),
            Err(SkillError::DirectoryConflict(_))
        ));
        assert!(matches!(
            engine.preflight("docx", AppTarget::Codex),
            Err(SkillError::DirectoryConflict(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_staging_path_that_is_a_link_is_unlinked_not_walked_into() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("precious.txt"), "keep me").unwrap();
        let staging = dir.path().join("staging");
        std::os::unix::fs::symlink(&outside, &staging).unwrap();
        remove_tree_without_following_links(&staging).unwrap();
        assert_eq!(kind(&staging), Kind::Missing);
        assert!(outside.join("precious.txt").is_file());
        // A missing path is nothing to do; a file is removed as itself.
        remove_tree_without_following_links(&dir.path().join("absent")).unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "x").unwrap();
        remove_tree_without_following_links(&file).unwrap();
        assert_eq!(kind(&file), Kind::Missing);
    }
}
