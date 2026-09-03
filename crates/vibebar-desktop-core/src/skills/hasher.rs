//! One SHA-256 over every non-hidden regular file, walked recursively and
//! ordered by relative POSIX path compared as UTF-8 bytes — the native
//! `SkillDirectoryHasher`, byte for byte, since the digest is what decides
//! whether a copy is still Vibe Bar's to replace. Each entry contributes
//! `<relative path>\0<payload>\0`; a symlink inside a skill hashes as its
//! target string, never the bytes it points at; hidden entries are skipped
//! at every level; empty directories contribute nothing.

use std::path::Path;

use sha2::{Digest, Sha256};

pub fn hash(directory: &Path) -> std::io::Result<String> {
    let mut entries: Vec<(String, std::path::PathBuf, bool)> = Vec::new();
    collect(directory, "", &mut entries)?;
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut hasher = Sha256::new();
    for (relative, path, is_symlink) in entries {
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        if is_symlink {
            let target = std::fs::read_link(&path)
                .map(|t| t.to_string_lossy().into_owned())
                .unwrap_or_default();
            hasher.update(target.as_bytes());
        } else {
            hasher.update(std::fs::read(&path)?);
        }
        hasher.update([0u8]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect(
    directory: &Path,
    relative: &str,
    out: &mut Vec<(String, std::path::PathBuf, bool)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        let child = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        if meta.file_type().is_symlink() {
            out.push((child, path, true));
        } else if meta.is_dir() {
            collect(&path, &child, out)?;
        } else if meta.is_file() {
            out.push((child, path, false));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_and_order_matter_and_hidden_entries_do_not() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "hello").unwrap();
        std::fs::write(dir.path().join("sub/a.txt"), "a").unwrap();
        let first = hash(dir.path()).unwrap();
        std::fs::write(dir.path().join(".DS_Store"), "junk").unwrap();
        std::fs::create_dir_all(dir.path().join("empty")).unwrap();
        assert_eq!(
            hash(dir.path()).unwrap(),
            first,
            "hidden files and empty dirs are invisible"
        );
        std::fs::write(dir.path().join("sub/a.txt"), "b").unwrap();
        assert_ne!(hash(dir.path()).unwrap(), first);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn the_digest_is_the_native_recipe() {
        // `SKILL.md` containing `hi` → sha256("SKILL.md" ‖ 0 ‖ "hi" ‖ 0)
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "hi").unwrap();
        let mut expected = Sha256::new();
        expected.update(b"SKILL.md");
        expected.update([0u8]);
        expected.update(b"hi");
        expected.update([0u8]);
        let expected: String = expected
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash(dir.path()).unwrap(), expected);
    }
}
