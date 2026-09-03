//! `~/.agents/.skill-lock.json`, the provenance file the third-party skill
//! installer maintains — the native `SkillLockFileReader`.
//!
//! Vibe Bar reads it so an adopted skill keeps the repository it came from,
//! and **never writes it**: the schema is not ours, so every field is
//! optional and anything unparseable simply means "no provenance".

use std::collections::HashMap;
use std::path::Path;

use super::registry::SkillId;

pub const RELATIVE_PATH: &str = ".agents/.skill-lock.json";

#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub id: SkillId,
    pub branch: Option<String>,
    /// Apple reference-date seconds, the registry's own unit.
    pub installed_at: Option<f64>,
    pub updated_at: Option<f64>,
}

#[derive(Debug, Default, Clone)]
pub struct LockFile {
    entries: HashMap<String, Entry>,
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    source: Option<String>,
    source_type: Option<String>,
    source_url: Option<String>,
    branch: Option<String>,
    source_branch: Option<String>,
    installed_at: Option<String>,
    updated_at: Option<String>,
}

impl LockFile {
    pub fn read(home: &Path) -> Self {
        Self::parse(&std::fs::read(home.join(RELATIVE_PATH)).unwrap_or_default())
    }

    pub fn parse(bytes: &[u8]) -> Self {
        let Ok(root) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return Self::default();
        };
        let Some(skills) = root.get("skills").and_then(|v| v.as_object()) else {
            return Self::default();
        };
        Self {
            entries: skills
                .iter()
                .filter_map(|(name, value)| {
                    serde_json::from_value::<Entry>(value.clone())
                        .ok()
                        .map(|entry| (name.clone(), entry))
                })
                .collect(),
        }
    }

    /// What the lock says about one skill directory. A directory it does not
    /// mention, or mentions without a usable GitHub source, is local.
    pub fn provenance(&self, directory: &str) -> Provenance {
        let entry = self.entries.get(directory);
        Provenance {
            id: identity(directory, entry),
            branch: entry.and_then(branch),
            installed_at: entry
                .and_then(|e| e.installed_at.as_deref())
                .and_then(apple_seconds),
            updated_at: entry
                .and_then(|e| e.updated_at.as_deref())
                .and_then(apple_seconds),
        }
    }
}

fn identity(directory: &str, entry: Option<&Entry>) -> SkillId {
    let local = || SkillId::Local {
        directory: directory.to_string(),
    };
    let Some(entry) = entry else { return local() };
    if entry.source_type.as_deref().map(str::to_ascii_lowercase) != Some("github".into()) {
        return local();
    }
    let Some(source) = entry.source.as_deref() else {
        return local();
    };
    let mut parts = source.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) if !owner.is_empty() && !repo.is_empty() => SkillId::Repo {
            owner: owner.to_string(),
            repo: repo.to_string(),
            directory: directory.to_string(),
        },
        _ => local(),
    }
}

fn branch(entry: &Entry) -> Option<String> {
    let named = entry
        .branch
        .as_deref()
        .or(entry.source_branch.as_deref())
        .map(str::trim)
        .filter(|b| !b.is_empty());
    if let Some(branch) = named {
        return Some(branch.to_string());
    }
    branch_from_source_url(entry.source_url.as_deref()?)
}

/// `sourceUrl` is a plain clone URL most of the time, but the installer also
/// accepts the browse-URL forms people paste. Checked in order: a
/// `/tree/<branch>/…` path segment, a `#branch` fragment, then a `?branch=` /
/// `?ref=` query value. Only the first segment after `/tree/` is taken — a
/// slashed branch name is indistinguishable from `<branch>/<path>` without
/// asking the remote.
pub fn branch_from_source_url(raw: &str) -> Option<String> {
    let (base, fragment) = match raw.split_once('#') {
        Some((base, fragment)) => (base, Some(fragment)),
        None => (raw, None),
    };
    let (base, query) = match base.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (base, None),
    };
    if let Some((_, after)) = base.split_once("/tree/") {
        let segment = after.split('/').next().unwrap_or("");
        if !segment.is_empty() {
            return Some(segment.to_string());
        }
    }
    if let Some(fragment) = fragment.filter(|f| !f.is_empty()) {
        return Some(fragment.to_string());
    }
    for pair in query.unwrap_or("").split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if (key == "branch" || key == "ref") && !value.is_empty() {
            return Some(percent_decoded(value));
        }
    }
    None
}

/// `%XX` escapes, decoded from the bytes. Working on bytes rather than string
/// slices matters: a `%` followed by a multibyte character would put a slice
/// boundary inside it and panic.
fn percent_decoded(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if let (b'%', Some(high), Some(low)) = (
            bytes[index],
            bytes.get(index + 1).copied().and_then(hex_digit),
            bytes.get(index + 2).copied().and_then(hex_digit),
        ) {
            out.push(high << 4 | low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// RFC 3339, with or without fractional seconds, as Apple reference-date
/// seconds — the unit the registry stores.
fn apple_seconds(raw: &str) -> Option<f64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(raw.trim()).ok()?;
    Some(crate::shared::unix_to_apple_seconds(
        parsed.timestamp() as f64 + f64::from(parsed.timestamp_subsec_nanos()) / 1e9,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = r#"{
      "version": 1,
      "skills": {
        "docx": {"source": "AstroQore/skills", "sourceType": "github",
                 "branch": "main", "installedAt": "2026-08-01T00:00:00Z",
                 "updatedAt": "2026-08-20T12:00:00.500Z"},
        "pdf": {"source": "AstroQore/skills", "sourceType": "github",
                "sourceUrl": "https://github.com/AstroQore/skills/tree/next/pdf"},
        "notes": {"source": "not-a-repo", "sourceType": "github"},
        "local-only": {"sourceType": "path", "source": "AstroQore/skills"},
        "broken": ["not", "an", "object"]
      }
    }"#;

    #[test]
    fn a_github_entry_becomes_a_repo_id_with_its_branch() {
        let lock = LockFile::parse(LOCK.as_bytes());
        let docx = lock.provenance("docx");
        assert_eq!(
            docx.id,
            SkillId::Repo {
                owner: "AstroQore".into(),
                repo: "skills".into(),
                directory: "docx".into()
            }
        );
        assert_eq!(docx.id.raw(), "AstroQore/skills:docx");
        assert_eq!(docx.branch.as_deref(), Some("main"));
        // 2026-08-01T00:00:00Z in Apple reference-date seconds.
        assert_eq!(docx.installed_at, Some(807_235_200.0));
        assert_eq!(docx.updated_at, Some(808_920_000.5));
    }

    #[test]
    fn a_browse_url_yields_the_branch_it_carries() {
        let lock = LockFile::parse(LOCK.as_bytes());
        assert_eq!(lock.provenance("pdf").branch.as_deref(), Some("next"));
        assert_eq!(
            branch_from_source_url("https://github.com/o/r.git#release%2F2"),
            Some("release%2F2".into())
        );
        assert_eq!(
            branch_from_source_url("https://github.com/o/r?ref=release%2F2"),
            Some("release/2".into())
        );
        // A `%` followed by a multibyte character must not slice inside it.
        assert_eq!(
            branch_from_source_url("https://github.com/o/r?ref=%€uro"),
            Some("%€uro".into())
        );
        assert_eq!(branch_from_source_url("https://github.com/o/r.git"), None);
    }

    #[test]
    fn anything_the_lock_cannot_vouch_for_is_local() {
        let lock = LockFile::parse(LOCK.as_bytes());
        for name in ["notes", "local-only", "broken", "never-heard-of-it"] {
            assert_eq!(
                lock.provenance(name).id,
                SkillId::Local {
                    directory: name.into()
                },
                "{name}"
            );
            assert_eq!(lock.provenance(name).branch, None, "{name}");
        }
        assert_eq!(
            LockFile::parse(b"<not json>").provenance("docx").id,
            SkillId::Local {
                directory: "docx".into()
            }
        );
        assert_eq!(
            LockFile::parse(b"{}").provenance("docx").id,
            SkillId::Local {
                directory: "docx".into()
            }
        );
    }

    #[test]
    fn a_missing_lock_file_is_simply_no_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let lock = LockFile::read(dir.path());
        assert_eq!(
            lock.provenance("docx").id,
            SkillId::Local {
                directory: "docx".into()
            }
        );
    }
}
