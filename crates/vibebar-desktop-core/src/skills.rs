//! Bounded, read-only inventory of skills in fixed local roots.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

const MAX_ENTRIES: usize = 4_096;
const MAX_SKILL_MD: u64 = 1024 * 1024;
const TARGET_ROOTS: [(&str, &str); 6] = [
    ("claude", ".claude/skills"),
    ("codex", ".codex/skills"),
    ("gemini", ".gemini/skills"),
    ("antigravity", ".gemini/config/skills"),
    ("grok", ".grok/skills"),
    ("cursor", ".cursor/skills"),
];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillsInventoryView {
    pub skills: Vec<SkillInventoryRow>,
    pub warnings: Vec<String>,
    pub scanned_at: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillInventoryRow {
    pub name: String,
    pub directory: String,
    pub description: Option<String>,
    pub targets: Vec<String>,
    pub health: String,
    pub source: String,
}

pub fn scan(root: &crate::paths::DataRoot) -> SkillsInventoryView {
    let home = if root.is_demo() {
        root.shared()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.shared().to_path_buf())
    } else {
        crate::paths::home_directory()
    };
    scan_home(&home)
}

fn scan_home(home: &Path) -> SkillsInventoryView {
    let ssot = home.join(".agents/skills");
    let mut warnings = Vec::new();
    let mut rows = Vec::new();
    let mut row_by_directory = std::collections::BTreeMap::<String, usize>::new();

    if safe_root(&ssot, "ssot", &mut warnings) {
        match fs::read_dir(&ssot) {
            Ok(entries) => {
                let mut entries = entries.flatten();
                for entry in entries.by_ref().take(MAX_ENTRIES) {
                    let Ok(directory) = entry.file_name().into_string() else {
                        warnings.push("ignored non-UTF-8 skill directory".into());
                        continue;
                    };
                    if !valid_directory_name(&directory) {
                        warnings.push(format!("ignored invalid skill directory: {directory}"));
                        continue;
                    }
                    let path = entry.path();
                    let Ok(metadata) = fs::symlink_metadata(&path) else {
                        warnings.push(format!("unreadable skill entry: {directory}"));
                        continue;
                    };
                    if metadata.file_type().is_symlink() {
                        warnings.push(format!("ignored symlink skill entry: {directory}"));
                        continue;
                    }
                    if !metadata.is_dir() {
                        continue;
                    }

                    let skill_md = path.join("SKILL.md");
                    let (name, description, health) = match read_skill_md(&skill_md) {
                        SkillDocument::Missing => (directory.clone(), None, "missing_skill_md"),
                        SkillDocument::Symlink => (directory.clone(), None, "symlink_ignored"),
                        SkillDocument::Oversize => (directory.clone(), None, "oversize"),
                        SkillDocument::Unreadable => (directory.clone(), None, "unreadable"),
                        SkillDocument::Text(text) => {
                            let (name, description, valid) = frontmatter(&directory, &text);
                            (
                                name,
                                description,
                                if valid { "healthy" } else { "unreadable" },
                            )
                        }
                    };
                    let index = rows.len();
                    rows.push(SkillInventoryRow {
                        name,
                        directory: directory.clone(),
                        description,
                        targets: Vec::new(),
                        health: health.into(),
                        source: "local".into(),
                    });
                    if health == "healthy" {
                        row_by_directory.insert(directory, index);
                    }
                }
                if entries.next().is_some() {
                    warnings.push("skill entry limit exceeded".into());
                }
            }
            Err(_) => warnings.push("unable to enumerate skill root: ssot".into()),
        }
    }

    let mut projection_entries = 0usize;
    'targets: for (target, relative) in TARGET_ROOTS {
        let directory = home.join(relative);
        if !safe_root(&directory, target, &mut warnings) {
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                warnings.push(format!("unable to enumerate skill root: {target}"));
                continue;
            }
        };
        for entry in entries.flatten() {
            if projection_entries >= MAX_ENTRIES {
                warnings.push("projection entry limit exceeded".into());
                break 'targets;
            }
            projection_entries += 1;
            let Ok(name) = entry.file_name().into_string() else {
                warnings.push(format!("ignored non-UTF-8 projection in {target}"));
                continue;
            };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                let expected = lexical_normalize(&ssot.join(&name));
                let resolved = fs::read_link(&path)
                    .ok()
                    .and_then(|link| resolve_link(&path, &link));
                if expected.is_some() && resolved == expected {
                    if let Some(index) = row_by_directory.get(&name) {
                        rows[*index].targets.push(target.into());
                    } else {
                        warnings.push(format!("ignored foreign or dangling link: {target}/{name}"));
                    }
                } else {
                    warnings.push(format!("ignored foreign or dangling link: {target}/{name}"));
                }
            } else if metadata.is_dir() {
                warnings.push(format!("ignored unmanaged directory: {target}/{name}"));
            }
        }
    }

    for row in &mut rows {
        row.targets.sort();
        row.targets.dedup();
    }
    rows.sort_by(|left, right| left.directory.cmp(&right.directory));
    SkillsInventoryView {
        skills: rows,
        warnings,
        scanned_at: now(),
    }
}

fn safe_root(path: &Path, label: &str, warnings: &mut Vec<String>) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
        Ok(_) => {
            warnings.push(format!("ignored unsafe skill root: {label}"));
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => {
            warnings.push(format!("unreadable skill root: {label}"));
            false
        }
    }
}

enum SkillDocument {
    Missing,
    Symlink,
    Oversize,
    Unreadable,
    Text(String),
}

fn read_skill_md(path: &Path) -> SkillDocument {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return SkillDocument::Missing
        }
        Err(_) => return SkillDocument::Unreadable,
    };
    if metadata.file_type().is_symlink() {
        return SkillDocument::Symlink;
    }
    if !metadata.is_file() {
        return SkillDocument::Unreadable;
    }
    if metadata.len() > MAX_SKILL_MD {
        return SkillDocument::Oversize;
    }
    let Ok(file) = fs::File::open(path) else {
        return SkillDocument::Unreadable;
    };
    let mut bytes = Vec::new();
    if file.take(MAX_SKILL_MD + 1).read_to_end(&mut bytes).is_err() {
        return SkillDocument::Unreadable;
    }
    if bytes.len() as u64 > MAX_SKILL_MD {
        return SkillDocument::Oversize;
    }
    String::from_utf8(bytes)
        .map(SkillDocument::Text)
        .unwrap_or(SkillDocument::Unreadable)
}

fn frontmatter(fallback: &str, text: &str) -> (String, Option<String>, bool) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (fallback.into(), None, true);
    }
    let mut name = None;
    let mut description = None;
    let mut closed = false;
    for line in lines.take(80) {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = clean_value(value, 200);
        } else if let Some(value) = line.strip_prefix("description:") {
            description = clean_value(value, 1_000);
        }
    }
    (name.unwrap_or_else(|| fallback.into()), description, closed)
}

fn clean_value(value: &str, limit: usize) -> Option<String> {
    let value = value.trim().trim_matches(['"', '\'']);
    if value.is_empty() || matches!(value, ">" | ">-" | "|" | "|-") {
        return None;
    }
    Some(value.chars().take(limit).collect())
}

fn valid_directory_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.chars().count() <= 255
        && !value.chars().any(char::is_control)
}

fn resolve_link(link_path: &Path, target: &Path) -> Option<PathBuf> {
    let candidate = if target.is_absolute() {
        target.to_path_buf()
    } else {
        link_path.parent()?.join(target)
    };
    lexical_normalize(&candidate)
}

fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    Some(normalized)
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_at(directory: &tempfile::TempDir) -> crate::paths::DataRoot {
        crate::paths::DataRoot::at(directory.path().join(".vibebar"))
    }

    fn write_skill(home: &Path, directory: &str, body: &[u8]) {
        let path = home.join(".agents/skills").join(directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn scans_healthy_and_missing_skills_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let root = root_at(&directory);
        write_skill(
            directory.path(),
            "demo",
            b"---\nname: Demo\ndescription: Test\n---\nbody",
        );
        fs::create_dir_all(directory.path().join(".agents/skills/missing")).unwrap();
        let before = fs::read(directory.path().join(".agents/skills/demo/SKILL.md")).unwrap();
        let view = scan(&root);
        let demo = view
            .skills
            .iter()
            .find(|skill| skill.directory == "demo")
            .unwrap();
        assert_eq!(demo.name, "Demo");
        assert_eq!(demo.description.as_deref(), Some("Test"));
        assert!(demo.targets.is_empty());
        assert!(view
            .skills
            .iter()
            .any(|skill| skill.health == "missing_skill_md"));
        assert_eq!(
            fs::read(directory.path().join(".agents/skills/demo/SKILL.md")).unwrap(),
            before
        );
        assert!(!root.shared().exists());
    }

    #[cfg(unix)]
    #[test]
    fn recognizes_only_lexically_safe_ssot_projections() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let root = root_at(&directory);
        write_skill(directory.path(), "demo", b"body");
        let codex = directory.path().join(".codex/skills");
        fs::create_dir_all(&codex).unwrap();
        symlink("../../.agents/skills/demo", codex.join("demo")).unwrap();
        symlink("../../outside", codex.join("foreign")).unwrap();
        symlink("../../.agents/skills/missing", codex.join("missing")).unwrap();
        let view = scan(&root);
        assert_eq!(view.skills[0].targets, vec!["codex"]);
        assert!(view
            .warnings
            .iter()
            .any(|warning| warning.contains("foreign or dangling")));
        assert!(view
            .warnings
            .iter()
            .any(|warning| warning.ends_with("codex/missing")));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_roots_and_ssot_entries_are_ignored() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let root = root_at(&directory);
        let foreign = directory.path().join("foreign");
        fs::create_dir_all(&foreign).unwrap();
        fs::create_dir_all(directory.path().join(".agents")).unwrap();
        symlink(&foreign, directory.path().join(".agents/skills")).unwrap();
        let view = scan(&root);
        assert!(view.skills.is_empty());
        assert!(view.warnings.iter().any(|warning| warning.contains("root")));
    }

    #[test]
    fn oversize_invalid_utf8_and_unclosed_frontmatter_are_not_healthy() {
        let directory = tempfile::tempdir().unwrap();
        let root = root_at(&directory);
        write_skill(
            directory.path(),
            "oversize",
            &vec![b'x'; MAX_SKILL_MD as usize + 1],
        );
        write_skill(directory.path(), "binary", &[0xff, 0xfe]);
        write_skill(directory.path(), "broken", b"---\nname: Broken\nbody");
        let view = scan(&root);
        assert_eq!(
            view.skills
                .iter()
                .find(|skill| skill.directory == "oversize")
                .unwrap()
                .health,
            "oversize"
        );
        for name in ["binary", "broken"] {
            assert_eq!(
                view.skills
                    .iter()
                    .find(|skill| skill.directory == name)
                    .unwrap()
                    .health,
                "unreadable"
            );
        }
    }

    #[test]
    fn frontmatter_block_descriptions_do_not_render_yaml_markers() {
        let (name, description, valid) =
            frontmatter("demo", "---\nname: Demo\ndescription: >-\n  text\n---\n");
        assert_eq!(name, "Demo");
        assert_eq!(description, None);
        assert!(valid);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_existing_root_is_not_reported_as_empty() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = root_at(&directory);
        let ssot = directory.path().join(".agents/skills");
        fs::create_dir_all(&ssot).unwrap();
        fs::set_permissions(&ssot, fs::Permissions::from_mode(0o000)).unwrap();
        let view = scan(&root);
        fs::set_permissions(&ssot, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(view.skills.is_empty());
        assert!(view
            .warnings
            .iter()
            .any(|warning| warning == "unable to enumerate skill root: ssot"));
    }
}
