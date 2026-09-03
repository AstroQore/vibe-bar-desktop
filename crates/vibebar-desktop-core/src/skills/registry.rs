//! The registry of installed skills at `~/.vibebar/skills.json` — the
//! native `SkillsStore` schema (version 1), read leniently entry by entry
//! and written whole: pretty with sorted keys, dates as seconds since 2001
//! the way Foundation's `JSONEncoder` writes them, to a temporary file
//! renamed into place. It is a shared store the native app also writes;
//! this client re-reads it immediately before each write and writes only
//! at the person's explicit request.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::catalog::AppTarget;
use super::SkillError;

pub const SCHEMA_VERSION: u32 = 1;
pub const LOCAL_SOURCE: &str = "local";

/// `owner/repo:directory` for a repository-backed skill, `local:directory`
/// for one installed from a folder or archive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SkillId {
    Repo {
        owner: String,
        repo: String,
        directory: String,
    },
    Local {
        directory: String,
    },
}

impl SkillId {
    pub fn raw(&self) -> String {
        match self {
            SkillId::Repo {
                owner,
                repo,
                directory,
            } => format!("{owner}/{repo}:{directory}"),
            SkillId::Local { directory } => format!("{LOCAL_SOURCE}:{directory}"),
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let (source, directory) = raw.split_once(':')?;
        if directory.is_empty() {
            return None;
        }
        if source == LOCAL_SOURCE {
            return Some(SkillId::Local {
                directory: directory.to_string(),
            });
        }
        let (owner, repo) = source.split_once('/')?;
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            return None;
        }
        Some(SkillId::Repo {
            owner: owner.to_string(),
            repo: repo.to_string(),
            directory: directory.to_string(),
        })
    }

    pub fn directory(&self) -> &str {
        match self {
            SkillId::Repo { directory, .. } | SkillId::Local { directory } => directory,
        }
    }

    pub fn repository_slug(&self) -> Option<String> {
        match self {
            SkillId::Repo { owner, repo, .. } => Some(format!("{owner}/{repo}")),
            SkillId::Local { .. } => None,
        }
    }
}

impl Serialize for SkillId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw())
    }
}

impl<'de> Deserialize<'de> for SkillId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        SkillId::parse(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("not a skill id: {raw}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    Auto,
    Symlink,
    Copy,
}

/// How a skill sits in one app's directory: a link back to the SSOT, or a
/// copy whose content hash at copy time is what makes it replaceable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Materialization {
    pub method: SyncMethod,
    #[serde(default)]
    pub adopted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash_at_copy: Option<String>,
}

impl Materialization {
    pub fn symlink() -> Self {
        Self {
            method: SyncMethod::Symlink,
            adopted: false,
            content_hash_at_copy: None,
        }
    }
    pub fn adopted_symlink() -> Self {
        Self {
            method: SyncMethod::Symlink,
            adopted: true,
            content_hash_at_copy: None,
        }
    }
    pub fn copy(hash: String) -> Self {
        Self {
            method: SyncMethod::Copy,
            adopted: false,
            content_hash_at_copy: Some(hash),
        }
    }
}

/// One installed skill, as the registry records it. Dates are seconds
/// since 2001-01-01 UTC (Foundation's reference date), the encoding the
/// native app writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: SkillId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub directory: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
    pub installed_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    /// Keyed by the app's raw name, so a registry written by a build that
    /// knows an app this one does not still round-trips.
    #[serde(default)]
    pub apps: std::collections::BTreeMap<String, Materialization>,
}

impl Skill {
    pub fn materialization(&self, app: AppTarget) -> Option<&Materialization> {
        self.apps.get(app.raw())
    }
    pub fn projected_apps(&self) -> Vec<AppTarget> {
        self.apps
            .keys()
            .filter_map(|raw| AppTarget::from_raw(raw))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub schema_version: u32,
    pub skills: Vec<Skill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discover_repos: Option<Vec<String>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            skills: Vec::new(),
            discover_repos: None,
        }
    }
}

/// `~/.vibebar/skills.json` under this data root.
pub fn registry_file(vibebar_dir: &Path) -> PathBuf {
    vibebar_dir.join("skills.json")
}

/// Read the registry leniently: an entry that does not decode is dropped
/// (the native reader does the same), a missing file is an empty registry,
/// and a file that cannot be parsed at all is an error rather than a fresh
/// default that would overwrite it.
pub fn read(vibebar_dir: &Path) -> Result<Registry, SkillError> {
    let path = registry_file(vibebar_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Registry::default())
        }
        Err(error) => return Err(SkillError::Io(error.to_string())),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SkillError::Io(format!("skills.json: {e}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| SkillError::Io("skills.json is not an object".into()))?;
    let schema_version = object
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;
    let skills = object
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| serde_json::from_value::<Skill>(entry.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let discover_repos = object
        .get("discoverRepos")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok());
    Ok(Registry {
        schema_version,
        skills,
        discover_repos,
    })
}

/// Write the registry whole, pretty with sorted keys, atomically.
pub fn write(vibebar_dir: &Path, registry: &Registry) -> Result<(), SkillError> {
    let value = serde_json::to_value(registry).map_err(|e| SkillError::Io(e.to_string()))?;
    let bytes = crate::shared::settings_document::to_bytes(
        value
            .as_object()
            .ok_or_else(|| SkillError::Io("registry is not an object".into()))?,
    )
    .map_err(|e| SkillError::Io(e.to_string()))?;
    std::fs::create_dir_all(vibebar_dir)?;
    crate::shared::write_atomic(&registry_file(vibebar_dir), &bytes)
        .map_err(|e| SkillError::Io(e.to_string()))
}

/// Seconds since 2001-01-01 UTC, now.
pub fn now_apple_seconds() -> f64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    crate::shared::unix_to_apple_seconds(unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_in_the_native_raw_form() {
        assert_eq!(
            SkillId::parse("anthropics/skills:docx").unwrap().raw(),
            "anthropics/skills:docx"
        );
        assert_eq!(
            SkillId::parse("local:code-review").unwrap(),
            SkillId::Local {
                directory: "code-review".into()
            }
        );
        assert!(SkillId::parse("nocolon").is_none());
        assert!(SkillId::parse("local:").is_none());
        assert!(SkillId::parse("a/b/c:dir").is_none());
    }

    #[test]
    fn the_registry_reads_leniently_and_writes_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let vibebar = dir.path().join(".vibebar");
        std::fs::create_dir_all(&vibebar).unwrap();
        std::fs::write(
            registry_file(&vibebar),
            r#"{"schemaVersion":1,"skills":[{"id":"local:docx","name":"docx","directory":"docx","installedAt":780000000,"apps":{"codex":{"method":"symlink","adopted":false},"future-app":{"method":"copy","contentHashAtCopy":"ab"}}},{"id":"broken"}],"discoverRepos":["anthropics/skills"]}"#,
        )
        .unwrap();
        let registry = read(&vibebar).unwrap();
        assert_eq!(
            registry.skills.len(),
            1,
            "the entry that does not decode is dropped"
        );
        assert_eq!(registry.skills[0].projected_apps(), vec![AppTarget::Codex]);
        assert!(
            registry.skills[0].apps.contains_key("future-app"),
            "an unknown app's record survives"
        );
        write(&vibebar, &registry).unwrap();
        let text = std::fs::read_to_string(registry_file(&vibebar)).unwrap();
        assert!(
            text.starts_with("{\n  \"discoverRepos\""),
            "pretty, keys sorted: {text}"
        );
        assert!(text.contains("\"future-app\""));
    }
}
