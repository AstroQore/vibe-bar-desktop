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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use agent_session_core::discovery::{self, DiscoveredSession};
use agent_session_core::index::{SessionIndexReader, SessionListFilter};
pub use agent_session_core::transcript::TranscriptCursor;
use agent_session_core::{resume, SessionProvider};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::OpenOptions;
use serde::Serialize;

use crate::error::CoreError;
use crate::paths::DataRoot;

const SCAN_LIMIT: usize = 400;
const FUTURE_TIMESTAMP_TOLERANCE_SECONDS: i64 = 300;
const SESSION_REFERENCE_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_SESSION_REFERENCES: usize = 4_096;

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
    /// CSPRNG-backed in-memory capability issued with this listing. It expires
    /// after 15 minutes and intentionally contains neither a path nor a
    /// provider selected by the web UI. References from overlapping listings
    /// coexist so a slower stale request cannot revoke the visible result.
    pub session_ref: String,
    /// Never serialize local paths into the webview. The backend associates
    /// this with `session_ref` before returning a listing.
    #[serde(skip_serializing)]
    source_path: String,
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
    references: Mutex<HashMap<String, ResolvedSession>>,
}

#[derive(Debug, Clone)]
struct ResolvedSession {
    provider: SessionProvider,
    source_path: PathBuf,
    expires_at: Instant,
}

impl SessionsService {
    pub fn new(root: DataRoot) -> Self {
        Self::with_home(root, crate::paths::home_directory())
    }

    /// Construct a read-only session reader for an explicit home directory.
    /// Embedders use this for a synthetic/demo root; it performs no discovery
    /// until a list or search call.
    pub fn with_home(root: DataRoot, home: impl Into<std::path::PathBuf>) -> Self {
        Self {
            root,
            home: home.into(),
            references: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self, limit: usize) -> SessionListing {
        self.list_filtered(None, None, None, 0, limit)
    }

    /// List sessions using only fields present in both the shared index and
    /// the scanned fallback. Harness values are the display names stored in
    /// the shared index (for example `Codex` and `Claude Code`).
    pub fn list_filtered(
        &self,
        providers: Option<&[SessionProvider]>,
        harnesses: Option<&[String]>,
        since: Option<i64>,
        offset: usize,
        limit: usize,
    ) -> SessionListing {
        match self.open_index() {
            IndexState::Ready(reader) => {
                let until = unix_now().saturating_add(FUTURE_TIMESTAMP_TOLERANCE_SECONDS);
                let mut rows: Vec<_> = reader
                    .list(&SessionListFilter {
                        providers: providers.map(<[SessionProvider]>::to_vec),
                        harnesses: harnesses.map(<[String]>::to_vec),
                        since,
                        until: Some(until),
                        limit,
                        offset,
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .map(indexed_row)
                    .collect();
                self.authorize_rows(&mut rows);
                let (indexed_total, index_note) = indexed_summary(&reader);
                SessionListing {
                    source: SessionSource::Indexed,
                    rows,
                    indexed_total,
                    index_note,
                }
            }
            IndexState::Unusable(note) => {
                self.scan(providers, harnesses, since, offset, limit, Some(note))
            }
            IndexState::Absent => self.scan(providers, harnesses, since, offset, limit, None),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> SessionListing {
        self.search_filtered(query, None, None, limit)
    }

    pub fn search_filtered(
        &self,
        query: &str,
        providers: Option<&[SessionProvider]>,
        harnesses: Option<&[String]>,
        limit: usize,
    ) -> SessionListing {
        let needle = query.trim();
        if needle.is_empty() {
            return self.list_filtered(providers, harnesses, None, 0, limit);
        }
        match self.open_index() {
            IndexState::Ready(reader) => {
                let until = unix_now().saturating_add(FUTURE_TIMESTAMP_TOLERANCE_SECONDS);
                let mut rows: Vec<_> = reader
                    .search(
                        needle,
                        &SessionListFilter {
                            providers: providers.map(<[SessionProvider]>::to_vec),
                            harnesses: harnesses.map(<[String]>::to_vec),
                            until: Some(until),
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
                self.authorize_rows(&mut rows);
                let (indexed_total, index_note) = indexed_summary(&reader);
                SessionListing {
                    source: SessionSource::Indexed,
                    rows,
                    indexed_total,
                    index_note,
                }
            }
            state => {
                // Scanned mode has no body index; match what we do have.
                let note = match state {
                    IndexState::Unusable(note) => Some(note),
                    _ => None,
                };
                let mut rows = self.scanned_rows();
                let now = unix_now();
                rows.retain(|row| {
                    has_plausible_timestamp(row, now)
                        && matches_filters(row, providers, harnesses, None)
                });
                let lowered = needle.to_lowercase();
                rows.retain(|row| {
                    row.title
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(&lowered))
                        || row.session_id.to_lowercase().contains(&lowered)
                        || row
                            .project_dir
                            .as_deref()
                            .is_some_and(|p| p.to_lowercase().contains(&lowered))
                });
                rows.truncate(limit);
                // Capabilities are issued only for rows this search returns.
                // Authorizing the wider scan first lets stale overlapping
                // searches evict references still visible in the UI.
                self.authorize_rows(&mut rows);
                SessionListing {
                    source: SessionSource::Scanned,
                    rows,
                    indexed_total: None,
                    index_note: note,
                }
            }
        }
    }

    /// Transcript page for a session capability issued by `list` or `search`.
    ///
    /// The capability is only an in-memory lookup key. It is not a path (or a
    /// path encoding), expires after 15 minutes, and a stale process cannot be
    /// used to read a newly supplied file.
    pub fn transcript(
        &self,
        session_ref: &str,
        offset: usize,
        limit: usize,
    ) -> Result<agent_session_core::transcript::TranscriptPage, CoreError> {
        self.transcript_with_cursor(session_ref, offset, limit, None)
    }

    pub fn transcript_with_cursor(
        &self,
        session_ref: &str,
        offset: usize,
        limit: usize,
        cursor: Option<TranscriptCursor>,
    ) -> Result<agent_session_core::transcript::TranscriptPage, CoreError> {
        let resolved = self
            .references
            .lock()
            .ok()
            .and_then(|references| references.get(session_ref).cloned())
            .filter(|resolved| resolved.expires_at > Instant::now())
            .ok_or(CoreError::SessionReferenceInvalid)?;
        let file = self
            .open_approved_session_file(resolved.provider, &resolved.source_path)
            .ok_or(CoreError::TranscriptUnavailable)?;

        // Do not pass through filesystem errors: those can contain local
        // path details and are irrelevant to a UI that can simply reload its
        // session listing.
        agent_session_core::transcript::read_page_from_file_with_cursor(
            resolved.provider,
            file,
            offset,
            limit,
            cursor,
        )
        .map_err(|_| CoreError::TranscriptUnavailable)
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
            Err(error) => {
                IndexState::Unusable(format!("The shared session index is unreadable: {error}"))
            }
        }
    }

    fn scan(
        &self,
        providers: Option<&[SessionProvider]>,
        harnesses: Option<&[String]>,
        since: Option<i64>,
        offset: usize,
        limit: usize,
        note: Option<String>,
    ) -> SessionListing {
        let mut rows = self.scanned_rows();
        let now = unix_now();
        rows.retain(|row| {
            has_plausible_timestamp(row, now) && matches_filters(row, providers, harnesses, since)
        });
        let mut rows: Vec<_> = rows.into_iter().skip(offset).take(limit).collect();
        self.authorize_rows(&mut rows);
        SessionListing {
            source: SessionSource::Scanned,
            rows,
            indexed_total: None,
            index_note: note,
        }
    }

    fn scanned_rows(&self) -> Vec<SessionRow> {
        let mut discovered = discovery::discover_codex(&self.home, SCAN_LIMIT);
        discovered.extend(discovery::discover_claude(&self.home, SCAN_LIMIT));
        discovered.sort_by_key(|s| std::cmp::Reverse(s.modified_at));
        discovered.into_iter().map(scanned_row).collect()
    }

    fn authorize_rows(&self, rows: &mut [SessionRow]) {
        let Ok(mut references) = self.references.lock() else {
            // A poisoned cache is safer treated as empty. Returning blank
            // references makes every transcript request fail closed.
            for row in rows {
                row.session_ref.clear();
            }
            return;
        };
        let now = Instant::now();
        references.retain(|_, resolved| resolved.expires_at > now);
        for row in rows {
            let Some(provider) = SessionProvider::from_raw(&row.provider) else {
                row.session_ref.clear();
                continue;
            };
            let source_path = PathBuf::from(&row.source_path);
            // Reuse one unexpired capability per resolved session. Repeated
            // listings therefore do not grow the cache, and a stale request
            // cannot evict the capability held by the currently visible row.
            let existing = references.iter().find_map(|(session_ref, resolved)| {
                (resolved.provider == provider && resolved.source_path == source_path)
                    .then(|| session_ref.clone())
            });
            if let Some(session_ref) = existing {
                if let Some(resolved) = references.get_mut(&session_ref) {
                    resolved.expires_at = now + SESSION_REFERENCE_TTL;
                }
                row.session_ref = session_ref;
                continue;
            }
            // Never evict an unexpired capability to make space for a newer
            // listing: the backend cannot know whether that listing was later
            // discarded by the frontend's cancellation guard. Fail closed for
            // new rows until an old reference expires instead.
            if references.len() >= MAX_SESSION_REFERENCES {
                row.session_ref.clear();
                continue;
            }
            let Some(session_ref) = opaque_reference(&references) else {
                row.session_ref.clear();
                continue;
            };
            references.insert(
                session_ref.clone(),
                ResolvedSession {
                    provider,
                    source_path,
                    expires_at: now + SESSION_REFERENCE_TTL,
                },
            );
            row.session_ref = session_ref;
        }
    }

    fn open_approved_session_file(
        &self,
        provider: SessionProvider,
        source_path: &Path,
    ) -> Option<std::fs::File> {
        // Transcript parsing understands only these two on-disk formats. In
        // particular, never fall back from an unknown provider to Codex.
        let roots: &[&str] = match provider {
            SessionProvider::Codex => &[".codex/sessions", ".codex/archived_sessions"],
            SessionProvider::Claude => &[".claude/projects", ".config/claude/projects"],
            _ => return None,
        };
        let home = crate::paths::open_ambient_dir(&self.home).ok()?;

        for root_relative in roots {
            let root_path = self.home.join(root_relative);
            let Ok(relative) = source_path.strip_prefix(&root_path) else {
                continue;
            };
            let Ok(mut components) = crate::paths::normal_components(relative) else {
                continue;
            };
            let Some(leaf) = components.pop() else {
                continue;
            };

            // Each provider root and intermediate directory is opened through
            // the original home handle without following symlinks. The leaf
            // itself is then opened exactly once with `follow(No)`.
            let Ok(mut directory) =
                crate::paths::open_dir_nofollow(&home, Path::new(root_relative))
            else {
                continue;
            };
            let mut valid = true;
            for component in components {
                match cap_fs_ext::DirExt::open_dir_nofollow(&directory, component) {
                    Ok(next) => directory = next,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let Ok(file) = directory.open_with(Path::new(leaf), &options) else {
                continue;
            };
            if !file
                .metadata()
                .ok()
                .is_some_and(|metadata| metadata.is_file())
            {
                continue;
            }
            return Some(file.into_std());
        }
        None
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn has_plausible_timestamp(row: &SessionRow, now: i64) -> bool {
    row.last_active_at
        .is_none_or(|timestamp| timestamp <= now.saturating_add(FUTURE_TIMESTAMP_TOLERANCE_SECONDS))
}

fn matches_filters(
    row: &SessionRow,
    providers: Option<&[SessionProvider]>,
    harnesses: Option<&[String]>,
    since: Option<i64>,
) -> bool {
    if let Some(providers) = providers {
        if !providers
            .iter()
            .any(|provider| provider.raw_value() == row.provider)
        {
            return false;
        }
    }
    if let Some(harnesses) = harnesses {
        if !harnesses
            .iter()
            .any(|harness| harness_matches_row(harness, &row.harness))
        {
            return false;
        }
    }
    if let Some(since) = since {
        if !row.last_active_at.is_some_and(|active| active >= since) {
            return false;
        }
    }
    true
}

fn harness_matches_row(filter: &str, row: &str) -> bool {
    filter == row || harness_display_name(filter).is_some_and(|display| display == row)
}

fn harness_display_name(raw: &str) -> Option<&'static str> {
    match raw {
        "codex" => Some("Codex"),
        "chatgptWork" => Some("ChatGPT Work"),
        "claudeCode" => Some("Claude Code"),
        "claudeCowork" => Some("Claude Cowork"),
        "geminiCLI" => Some("Gemini CLI"),
        "antigravity" => Some("AntiGravity"),
        "grokBuild" => Some("Grok Build"),
        "cursor" => Some("Cursor"),
        "grokBot" => Some("Grok Bot"),
        _ => None,
    }
}

enum IndexState {
    Ready(SessionIndexReader),
    Unusable(String),
    Absent,
}

fn indexed_summary(reader: &SessionIndexReader) -> (Option<i64>, Option<String>) {
    let known = reader.session_count().ok();
    let Ok(compatibility) = reader.provider_compatibility() else {
        return (known, None);
    };
    let total = known.and_then(|known| known.checked_add(compatibility.unknown_session_count));
    let note = (compatibility.unknown_session_count > 0).then(|| {
        let (subject, verb) = if compatibility.unknown_session_count == 1 {
            ("session uses", "is")
        } else {
            ("sessions use", "are")
        };
        format!(
            "{} {subject} provider keys this Desktop build does not understand and {verb} omitted until it is updated.",
            compatibility.unknown_session_count,
        )
    });
    (total, note)
}

fn indexed_row(session: agent_session_core::index::SessionSummary) -> SessionRow {
    let resume_command = resume::command(
        session.provider,
        &session.session_id,
        session.provider_variant.as_deref(),
    )
    .ok()
    .map(|command| resume_line(session.project_dir.as_deref(), &command));
    SessionRow {
        row_id: Some(session.row_id),
        provider: session.provider.raw_value().to_string(),
        harness: session
            .harness
            .as_deref()
            .map(|raw| harness_display_name(raw).unwrap_or(raw).to_string())
            .unwrap_or_else(|| session.provider.default_harness().to_string()),
        session_id: session.session_id,
        title: session.title,
        project_dir: session.project_dir,
        last_active_at: session.last_active_at.or(session.created_at),
        session_ref: String::new(),
        source_path: session.source_path,
        message_count: session.message_count,
        resume_command,
        excerpt: None,
    }
}

fn scanned_row(session: DiscoveredSession) -> SessionRow {
    let resume_command = resume::command(session.provider, &session.session_id, None)
        .ok()
        .map(|command| resume_line(session.project_dir.as_deref(), &command));
    let harness = session
        .harness
        .as_deref()
        .and_then(harness_display_name)
        .unwrap_or_else(|| session.provider.default_harness())
        .to_string();
    SessionRow {
        row_id: None,
        provider: session.provider.raw_value().to_string(),
        harness,
        session_id: session.session_id,
        title: session.title,
        project_dir: session.project_dir,
        last_active_at: Some(session.modified_at),
        session_ref: String::new(),
        source_path: session.source_path,
        message_count: None,
        resume_command,
        excerpt: None,
    }
}

fn resume_line(project_dir: Option<&str>, command: &str) -> String {
    #[cfg(unix)]
    {
        resume::posix_shell_line(project_dir, command)
    }
    #[cfg(not(unix))]
    {
        // The kit deliberately exposes no shell quoting for Windows. Copy the
        // provider command rather than emitting a POSIX `cd` line that would
        // fail in PowerShell or cmd.exe.
        let _ = project_dir;
        command.to_string()
    }
}

fn opaque_reference(references: &HashMap<String, ResolvedSession>) -> Option<String> {
    for _ in 0..4 {
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).ok()?;
        let candidate = format!("session_v1_{}", URL_SAFE_NO_PAD.encode(random));
        if !references.contains_key(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(root: DataRoot, home: &Path) -> SessionsService {
        SessionsService {
            root,
            home: home.to_path_buf(),
            references: Mutex::new(HashMap::new()),
        }
    }

    fn write_codex_session(home: &Path) -> (PathBuf, &'static str) {
        let id = "0199aaaa-1111-2222-3333-444455556666";
        (write_codex_session_named(home, id, "first message"), id)
    }

    fn write_codex_session_named(home: &Path, id: &str, first_message: &str) -> PathBuf {
        write_codex_session_with_originator(home, id, first_message, None)
    }

    fn write_codex_session_with_originator(
        home: &Path,
        id: &str,
        first_message: &str,
        originator: Option<&str>,
    ) -> PathBuf {
        use std::io::Write;

        let sessions = home.join(".codex/sessions/2026/08/30");
        std::fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join(format!("rollout-2026-08-30T04-55-08-{id}.jsonl"));
        let mut file = std::fs::File::create(&path).unwrap();
        let mut payload = serde_json::json!({"id": id, "cwd": "/Users/example/proj"});
        if let Some(originator) = originator {
            payload["originator"] = serde_json::Value::String(originator.to_string());
        }
        writeln!(
            file,
            "{}",
            serde_json::json!({"type": "session_meta", "payload": payload})
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"text": first_message}]}})
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"text": "second message"}]}})
        )
        .unwrap();
        path
    }

    #[test]
    fn falls_back_to_scanning_when_no_index_exists() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(DataRoot::at(dir.path().join(".vibebar")), dir.path());
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

        let service = service(root, dir.path());
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
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write_codex_session(home);

        let service = service(DataRoot::at(home.join(".vibebar")), home);
        let listing = service.list(20);
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].harness, "Codex");
        #[cfg(unix)]
        assert_eq!(
            listing.rows[0].resume_command.as_deref(),
            Some("cd '/Users/example/proj' && codex resume 0199aaaa-1111-2222-3333-444455556666")
        );
        #[cfg(not(unix))]
        assert_eq!(
            listing.rows[0].resume_command.as_deref(),
            Some("codex resume 0199aaaa-1111-2222-3333-444455556666")
        );

        // Title/id search works without a body index.
        assert_eq!(service.search("0199aaaa", 20).rows.len(), 1);
        assert_eq!(service.search("nothing-matches-this", 20).rows.len(), 0);
    }

    #[test]
    fn scanned_codex_originator_distinguishes_chatgpt_work_harness() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write_codex_session_with_originator(
            home,
            "0199aaaa-1111-2222-3333-444455556666",
            "work prompt",
            Some("codex_work_desktop"),
        );
        let service = service(DataRoot::at(home.join(".vibebar")), home);
        let work = ["chatgptWork".to_string()];
        let listing = service.list_filtered(None, Some(&work), None, 0, 10);
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].harness, "ChatGPT Work");
        let codex = ["codex".to_string()];
        assert!(service
            .list_filtered(None, Some(&codex), None, 0, 10)
            .rows
            .is_empty());
    }

    #[test]
    fn scanned_search_authorizes_only_returned_rows() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write_codex_session_named(
            home,
            "0199aaaa-1111-2222-3333-444455556601",
            "the only needle",
        );
        write_codex_session_named(
            home,
            "0199aaaa-1111-2222-3333-444455556602",
            "unrelated alpha",
        );
        write_codex_session_named(
            home,
            "0199aaaa-1111-2222-3333-444455556603",
            "unrelated beta",
        );

        let service = service(DataRoot::at(home.join(".vibebar")), home);
        let listing = service.search("needle", 1);
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].title.as_deref(), Some("the only needle"));
        assert_eq!(service.references.lock().unwrap().len(), 1);
        assert!(service
            .transcript(&listing.rows[0].session_ref, 0, 1)
            .is_ok());
    }

    #[test]
    fn indexed_list_filters_future_rows_before_paging() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let root = DataRoot::at(home.join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        let index = root.session_index_file();
        let conn = rusqlite::Connection::open(&index).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 5;\
             CREATE TABLE sessions(\
               id INTEGER PRIMARY KEY, provider TEXT NOT NULL, session_id TEXT NOT NULL,\
               provider_variant TEXT, harness TEXT, model TEXT, title TEXT, summary TEXT,\
               project_dir TEXT, created_at INTEGER, last_active_at INTEGER, source_path TEXT NOT NULL,\
               size_bytes INTEGER, message_count INTEGER\
             );\
             INSERT INTO sessions(id, provider, session_id, harness, last_active_at, source_path) VALUES\
               (1, 'codex', 'older-codex', 'codex', 250, '/Users/example/older.jsonl'),\
               (2, 'codex', 'work', 'chatgptWork', 300, '/Users/example/work.jsonl'),\
               (3, 'claude', 'claude', 'claudeCode', 350, '/Users/example/claude.jsonl'),\
               (4, 'codex', 'newer-codex', 'codex', 400, '/Users/example/newer.jsonl'),\
               (5, 'codex', 'future-codex', 'codex', 9223372036854775807, '/Users/example/future.jsonl');",
        )
        .unwrap();
        drop(conn);

        let service = service(root, home);
        let providers = [SessionProvider::Codex];
        let harnesses = ["codex".to_string()];
        let first = service.list_filtered(Some(&providers), Some(&harnesses), Some(200), 0, 1);
        assert_eq!(first.rows.len(), 1);
        assert_eq!(first.rows[0].session_id, "newer-codex");
        assert_eq!(first.rows[0].harness, "Codex");

        let listing = service.list_filtered(Some(&providers), Some(&harnesses), Some(200), 1, 1);
        assert_eq!(listing.source, SessionSource::Indexed);
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].session_id, "older-codex");
        assert_eq!(listing.indexed_total, Some(5));
        assert!(listing
            .rows
            .iter()
            .all(|row| row.session_id != "future-codex"));
        assert!(service
            .list_filtered(Some(&[]), None, None, 0, 10)
            .rows
            .is_empty());
    }

    #[test]
    fn scanned_transcript_references_are_opaque_and_fail_closed_when_stale() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let (path, _) = write_codex_session(home);
        let service = service(DataRoot::at(home.join(".vibebar")), home);

        let listing = service.list(20);
        assert_eq!(listing.source, SessionSource::Scanned);
        let session_ref = listing.rows[0].session_ref.clone();
        assert!(session_ref.starts_with("session_v1_"));
        assert_eq!(session_ref.trim_start_matches("session_v1_").len(), 32);
        assert!(!session_ref.contains(&path.display().to_string()));
        assert!(serde_json::to_value(&listing.rows[0])
            .unwrap()
            .get("sourcePath")
            .is_none());

        let first = service.transcript(&session_ref, 0, 1).unwrap();
        assert_eq!(first.total_messages, Some(2));
        assert_eq!(first.messages[0].text, "first message");
        let second = service.transcript(&session_ref, 1, 1).unwrap();
        assert_eq!(second.messages[0].text, "second message");
        assert!(matches!(
            service.transcript("/Users/example/secret.jsonl", 0, 1),
            Err(CoreError::SessionReferenceInvalid)
        ));
        service.references.lock().unwrap().insert(
            "expired".to_string(),
            ResolvedSession {
                provider: SessionProvider::Codex,
                source_path: path.clone(),
                expires_at: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
            },
        );
        assert!(matches!(
            service.transcript("expired", 0, 1),
            Err(CoreError::SessionReferenceInvalid)
        ));

        // A newer listing reuses the unexpired capability for the same
        // resolved session, so repeated or overlapping requests do not grow
        // the bounded cache.
        let refreshed_ref = service.list(20).rows[0].session_ref.clone();
        assert_eq!(refreshed_ref, session_ref);
        assert_eq!(
            service.transcript(&session_ref, 0, 1).unwrap().messages[0].text,
            "first message"
        );
    }

    #[test]
    fn session_reference_cache_is_bounded_without_evicting_visible_rows() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(DataRoot::at(dir.path().join(".vibebar")), dir.path());
        let expires_at = Instant::now() + SESSION_REFERENCE_TTL;
        let mut references = service.references.lock().unwrap();
        for index in 0..MAX_SESSION_REFERENCES {
            references.insert(
                format!("reference-{index}"),
                ResolvedSession {
                    provider: SessionProvider::Codex,
                    source_path: PathBuf::from("/synthetic"),
                    expires_at,
                },
            );
        }
        drop(references);

        let mut incoming = vec![SessionRow {
            row_id: None,
            provider: SessionProvider::Codex.raw_value().to_string(),
            harness: "Codex".to_string(),
            session_id: "new-session".to_string(),
            title: None,
            project_dir: None,
            last_active_at: None,
            session_ref: String::new(),
            source_path: "/synthetic/new".to_string(),
            message_count: None,
            resume_command: None,
            excerpt: None,
        }];
        service.authorize_rows(&mut incoming);
        let references = service.references.lock().unwrap();
        assert_eq!(references.len(), MAX_SESSION_REFERENCES);
        assert!(references.contains_key("reference-0"));
        assert!(incoming[0].session_ref.is_empty());
    }

    #[test]
    fn repeated_listing_reuses_the_same_session_reference() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write_codex_session(home);
        let service = service(DataRoot::at(home.join(".vibebar")), home);

        let first = service.list(20).rows[0].session_ref.clone();
        let second = service.list(20).rows[0].session_ref.clone();
        assert_eq!(second, first);
        assert_eq!(service.references.lock().unwrap().len(), 1);
    }

    #[test]
    fn indexed_transcript_reference_resolves_only_the_indexed_session() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let (path, id) = write_codex_session(home);
        let root = DataRoot::at(home.join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        let index = root.session_index_file();
        let conn = rusqlite::Connection::open(&index).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 5;\
             CREATE TABLE sessions(\
               id INTEGER PRIMARY KEY, provider TEXT NOT NULL, session_id TEXT NOT NULL,\
               provider_variant TEXT, harness TEXT, model TEXT, title TEXT, summary TEXT,\
               project_dir TEXT, created_at INTEGER, last_active_at INTEGER, source_path TEXT NOT NULL,\
               size_bytes INTEGER, message_count INTEGER\
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, provider, session_id, harness, source_path) VALUES
             (1, 'codex', ?1, 'Codex', ?2),
             (2, 'future-provider', 'future-session', 'Future', '/Users/example/future.jsonl')",
            rusqlite::params![id, path.display().to_string()],
        )
        .unwrap();
        drop(conn);

        let service = service(root, home);
        let listing = service.list(20);
        assert_eq!(listing.source, SessionSource::Indexed);
        assert_eq!(listing.indexed_total, Some(2));
        assert!(listing
            .index_note
            .as_deref()
            .is_some_and(|note| note.contains("1 session uses provider keys")));
        assert_eq!(listing.rows.len(), 1);
        let session_ref = listing.rows[0].session_ref.clone();
        assert_eq!(
            service
                .transcript(&session_ref, 0, 1)
                .unwrap()
                .total_messages,
            Some(2)
        );

        // A known session file is still not readable under a different
        // provider. This guards against the old Codex-default behavior.
        service.references.lock().unwrap().insert(
            "wrong-provider".to_string(),
            ResolvedSession {
                provider: SessionProvider::Grok,
                source_path: path,
                expires_at: Instant::now() + SESSION_REFERENCE_TTL,
            },
        );
        assert!(matches!(
            service.transcript("wrong-provider", 0, 1),
            Err(CoreError::TranscriptUnavailable)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn transcript_opener_rejects_root_middle_leaf_and_out_of_root_paths() {
        use std::io::Read;
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let (good_path, _) = write_codex_session(home);
        let service = service(DataRoot::at(home.join(".vibebar")), home);
        let mut good = service
            .open_approved_session_file(SessionProvider::Codex, &good_path)
            .expect("a regular session under the provider root");
        let mut content = String::new();
        good.read_to_string(&mut content).unwrap();
        assert!(content.contains("first message"));

        let outside = home.join("outside.jsonl");
        std::fs::write(&outside, "outside").unwrap();
        assert!(service
            .open_approved_session_file(SessionProvider::Codex, &outside)
            .is_none());

        // A leaf link may point at a readable JSONL but must never be opened.
        let leaf = home.join(".codex/sessions/2026/08/30/leaf.jsonl");
        symlink(&outside, &leaf).unwrap();
        assert!(service
            .open_approved_session_file(SessionProvider::Codex, &leaf)
            .is_none());

        // A symlinked intermediate component is rejected before the leaf is
        // considered.
        let middle_target = home.join("middle-target");
        std::fs::create_dir_all(middle_target.join("08/30")).unwrap();
        let middle_leaf = middle_target.join("08/30/middle.jsonl");
        std::fs::write(&middle_leaf, "middle").unwrap();
        let middle = home.join(".codex/sessions/2026");
        std::fs::remove_dir_all(&middle).unwrap();
        symlink(&middle_target, &middle).unwrap();
        let apparent_middle = home.join(".codex/sessions/2026/08/30/middle.jsonl");
        assert!(service
            .open_approved_session_file(SessionProvider::Codex, &apparent_middle)
            .is_none());

        // The provider root itself is opened component-by-component from the
        // home anchor, so a `.codex` replacement is rejected too.
        std::fs::remove_file(&middle).unwrap();
        let codex = home.join(".codex");
        std::fs::rename(&codex, home.join("codex-real")).unwrap();
        symlink(home.join("codex-real"), &codex).unwrap();
        let apparent_root = home.join(".codex/sessions/2026/08/30/leaf.jsonl");
        assert!(service
            .open_approved_session_file(SessionProvider::Codex, &apparent_root)
            .is_none());
    }
}
