//! Sessions, from whichever source this machine actually has.
//!
//! Two modes, and the UI is told which one it is looking at:
//!
//! - **Indexed** — the shared `session_index.sqlite3` exists at a schema this
//!   build understands. Full-text search across every harness the index
//!   writer covers.
//! - **Scanned** — no usable index (a machine that has only ever run Desktop,
//!   or an index written at a schema this build does not know). Codex and
//!   Claude Code logs are enumerated directly; search is a title match.
//!
//! Desktop never builds or repairs the index. The fallback is deliberately
//! narrower than the index rather than a second, competing indexer.

use std::path::Path;

use agent_session_core::discovery::{self, DiscoveredSession};
use agent_session_core::index::{SessionIndexReader, SessionListFilter};
use agent_session_core::{resume, SessionProvider};
use serde::Serialize;

use crate::error::CoreError;
use crate::paths::DataRoot;

const SCAN_LIMIT: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionSource {
    Indexed,
    Scanned,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    /// Index rowid when indexed; absent when scanned.
    pub row_id: Option<i64>,
    pub provider: String,
    pub harness: String,
    pub session_id: String,
    pub title: Option<String>,
    pub project_dir: Option<String>,
    /// Unix epoch seconds.
    pub last_active_at: Option<i64>,
    pub source_path: String,
    pub message_count: Option<i64>,
    /// Ready-to-paste resume line, when the provider has one.
    pub resume_command: Option<String>,
    /// Search excerpt, when this row came from a search.
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListing {
    pub source: SessionSource,
    pub rows: Vec<SessionRow>,
    /// Total sessions the index holds; absent in scanned mode.
    pub indexed_total: Option<i64>,
    /// Set when an index exists but this build will not read it.
    pub index_note: Option<String>,
}

pub struct SessionsService {
    root: DataRoot,
    home: std::path::PathBuf,
}

impl SessionsService {
    pub fn new(root: DataRoot) -> Self {
        let home = crate::paths::home_directory();
        Self { root, home }
    }

    pub fn list(&self, limit: usize) -> SessionListing {
        match self.open_index() {
            IndexState::Ready(reader) => {
                let rows = reader
                    .list(&SessionListFilter {
                        limit,
                        ..Default::default()
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .map(indexed_row)
                    .collect();
                SessionListing {
                    source: SessionSource::Indexed,
                    rows,
                    indexed_total: reader.session_count().ok(),
                    index_note: None,
                }
            }
            IndexState::Unusable(note) => self.scan(limit, Some(note)),
            IndexState::Absent => self.scan(limit, None),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> SessionListing {
        let needle = query.trim();
        if needle.is_empty() {
            return self.list(limit);
        }
        match self.open_index() {
            IndexState::Ready(reader) => {
                let rows = reader
                    .search(
                        needle,
                        &SessionListFilter {
                            limit,
                            ..Default::default()
                        },
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .map(|hit| {
                        let mut row = indexed_row(hit.session);
                        row.excerpt = Some(hit.excerpt);
                        row
                    })
                    .collect();
                SessionListing {
                    source: SessionSource::Indexed,
                    rows,
                    indexed_total: reader.session_count().ok(),
                    index_note: None,
                }
            }
            state => {
                // Scanned mode has no body index; match what we do have.
                let note = match state {
                    IndexState::Unusable(note) => Some(note),
                    _ => None,
                };
                let mut listing = self.scan(SCAN_LIMIT, note);
                let lowered = needle.to_lowercase();
                listing.rows.retain(|row| {
                    row.title
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(&lowered))
                        || row.session_id.to_lowercase().contains(&lowered)
                        || row
                            .project_dir
                            .as_deref()
                            .is_some_and(|p| p.to_lowercase().contains(&lowered))
                });
                listing.rows.truncate(limit);
                listing
            }
        }
    }

    /// Transcript page for one session log.
    pub fn transcript(
        &self,
        provider: &str,
        source_path: &str,
        offset: usize,
        limit: usize,
    ) -> Result<agent_session_core::transcript::TranscriptPage, CoreError> {
        let provider = SessionProvider::from_raw(provider).unwrap_or(SessionProvider::Codex);
        Ok(agent_session_core::transcript::read_page(
            provider,
            Path::new(source_path),
            offset,
            limit,
        )?)
    }

    fn open_index(&self) -> IndexState {
        let path = self.root.session_index_file();
        if !path.is_file() {
            return IndexState::Absent;
        }
        match SessionIndexReader::open(&path) {
            Ok(reader) => IndexState::Ready(reader),
            Err(agent_session_core::SessionCoreError::UnsupportedIndexSchema {
                found,
                expected,
            }) => IndexState::Unusable(format!(
                "The shared session index is at schema v{found} and this build reads v{expected}. \
                 Showing locally scanned sessions instead."
            )),
            Err(error) => IndexState::Unusable(format!("The shared session index is unreadable: {error}")),
        }
    }

    fn scan(&self, limit: usize, note: Option<String>) -> SessionListing {
        let mut discovered = discovery::discover_codex(&self.home, SCAN_LIMIT);
        discovered.extend(discovery::discover_claude(&self.home, SCAN_LIMIT));
        discovered.sort_by_key(|s| std::cmp::Reverse(s.modified_at));
        discovered.truncate(limit);
        SessionListing {
            source: SessionSource::Scanned,
            rows: discovered.into_iter().map(scanned_row).collect(),
            indexed_total: None,
            index_note: note,
        }
    }
}

enum IndexState {
    Ready(SessionIndexReader),
    Unusable(String),
    Absent,
}

fn indexed_row(session: agent_session_core::index::SessionSummary) -> SessionRow {
    let resume_command = resume::command(
        session.provider,
        &session.session_id,
        session.provider_variant.as_deref(),
    )
    .ok()
    .map(|command| resume::shell_line(session.project_dir.as_deref(), &command));
    SessionRow {
        row_id: Some(session.row_id),
        provider: session.provider.raw_value().to_string(),
        harness: session
            .harness
            .clone()
            .unwrap_or_else(|| session.provider.default_harness().to_string()),
        session_id: session.session_id,
        title: session.title,
        project_dir: session.project_dir,
        last_active_at: session.last_active_at.or(session.created_at),
        source_path: session.source_path,
        message_count: session.message_count,
        resume_command,
        excerpt: None,
    }
}

fn scanned_row(session: DiscoveredSession) -> SessionRow {
    let resume_command = resume::command(session.provider, &session.session_id, None)
        .ok()
        .map(|command| resume::shell_line(session.project_dir.as_deref(), &command));
    SessionRow {
        row_id: None,
        provider: session.provider.raw_value().to_string(),
        harness: session.provider.default_harness().to_string(),
        session_id: session.session_id,
        title: session.title,
        project_dir: session.project_dir,
        last_active_at: Some(session.modified_at),
        source_path: session.source_path,
        message_count: None,
        resume_command,
        excerpt: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_scanning_when_no_index_exists() {
        let dir = tempfile::tempdir().unwrap();
        let service = SessionsService {
            root: DataRoot::at(dir.path().join(".vibebar")),
            home: dir.path().to_path_buf(),
        };
        let listing = service.list(20);
        assert_eq!(listing.source, SessionSource::Scanned);
        assert!(listing.rows.is_empty());
        assert!(listing.index_note.is_none());
        assert!(listing.indexed_total.is_none());
    }

    #[test]
    fn an_unknown_index_schema_degrades_with_a_note_and_never_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        let index = root.session_index_file();
        let conn = rusqlite::Connection::open(&index).unwrap();
        conn.execute_batch("PRAGMA user_version = 99; CREATE TABLE sessions(id);")
            .unwrap();
        drop(conn);
        let before = std::fs::read(&index).unwrap();

        let service = SessionsService {
            root,
            home: dir.path().to_path_buf(),
        };
        let listing = service.list(20);
        assert_eq!(listing.source, SessionSource::Scanned);
        let note = listing.index_note.expect("a note explaining the fallback");
        assert!(note.contains("v99"), "note should name the schema: {note}");
        assert_eq!(
            std::fs::read(&index).unwrap(),
            before,
            "the index must be left exactly as found"
        );
    }

    #[test]
    fn scans_codex_logs_and_builds_a_resume_line() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let sessions = home.join(".codex/sessions/2026/08/30");
        std::fs::create_dir_all(&sessions).unwrap();
        let mut f = std::fs::File::create(
            sessions.join("rollout-2026-08-30T04-55-08-0199aaaa-1111-2222-3333-444455556666.jsonl"),
        )
        .unwrap();
        writeln!(
            f,
            "{}",
            serde_json::json!({"type": "session_meta",
                               "payload": {"id": "0199aaaa-1111-2222-3333-444455556666",
                                           "cwd": "/Users/example/proj"}})
        )
        .unwrap();
        drop(f);

        let service = SessionsService {
            root: DataRoot::at(home.join(".vibebar")),
            home: home.to_path_buf(),
        };
        let listing = service.list(20);
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].harness, "Codex");
        assert_eq!(
            listing.rows[0].resume_command.as_deref(),
            Some("cd '/Users/example/proj' && codex resume 0199aaaa-1111-2222-3333-444455556666")
        );

        // Title/id search works without a body index.
        assert_eq!(service.search("0199aaaa", 20).rows.len(), 1);
        assert_eq!(service.search("nothing-matches-this", 20).rows.len(), 0);
    }
}
