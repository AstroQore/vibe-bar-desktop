//! Where the forecast's evidence comes from.
//!
//! A forecast needs history, and a client that has just been installed has
//! none. Two sources close that gap:
//!
//! 1. **Desktop's own observations**, written to `client/desktop/` on every
//!    refresh. This is the standalone path — it works on a machine that has
//!    never seen the native app, and it is the only one that keeps growing.
//! 2. **The native app's timeline**, read-only, as a seed. On a Mac where
//!    both are installed this turns "no forecast for two weeks" into a real
//!    forecast immediately, because the observations were already being
//!    recorded — by the other client, about the same quotas, for the same
//!    person.
//!
//! The seed is strictly read-only and strictly optional: a missing file, an
//! unreadable one, or a schema this build does not know all degrade to "no
//! seed", never to an error and never to a write.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::model::Observation;
use crate::paths::DataRoot;

/// Schema of Desktop's own observation store. Independent of the native
/// timeline's version: this file belongs to this client.
const SCHEMA_VERSION: i64 = 1;

/// The native timeline schema this build can read. A different version is
/// left alone rather than guessed at — the file belongs to the other client.
const NATIVE_SCHEMA_VERSION: i64 = 1;

/// Keep enough history to cover the longest quota window several times over
/// without letting the file grow without bound. Monthly buckets are the
/// widest thing forecast, so sixty days is roughly two of them.
const RETENTION_SECONDS: f64 = 60.0 * 86_400.0;

/// Seeding takes nothing dated after the moment it runs.
///
/// A tolerance here does not help: `compute` accepts points through
/// `evaluation + 60`, so even a row thirty seconds ahead becomes the freshest
/// sample and steers the slope, the coverage and the freshness score. Shared
/// data written by another client on a skewed clock is exactly the case, and
/// the cost of dropping such a row is one observation.
const SEED_UPPER_BOUND_SLACK: f64 = 0.0;

/// One observation, keyed the way both stores key them.
#[derive(Debug, Clone, Copy)]
pub struct StoredObservation {
    pub sampled_at: f64,
    pub used_percent: f64,
    pub reset_at: Option<f64>,
    pub raw_window_seconds: Option<i64>,
}

/// True when `path` resolves outside the client namespace, including through
/// a symlinked ancestor. Compares canonical paths, so a link anywhere in the
/// chain is caught rather than only a link at the leaf.
fn path_escapes_namespace(root: &DataRoot, path: &Path) -> bool {
    // Every component from the data root down to the leaf must be a real
    // directory. Canonicalising the client directory would defeat this: if
    // `client/desktop` is itself a link, its target becomes the trusted
    // anchor and everything below it then looks contained.
    let mut current = root.shared().to_path_buf();
    let Ok(relative) = path.strip_prefix(&current) else {
        return true;
    };
    for component in relative.components() {
        use std::path::Component;
        match component {
            Component::Normal(name) => current.push(name),
            // `..`, a root, or a prefix in a path this crate built itself
            // means something is very wrong.
            _ => return true,
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            // Not created yet is fine; anything beyond this point does not
            // exist either, so there is nothing left to escape through.
            Err(_) => return false,
            Ok(_) => {}
        }
    }
    false
}

pub struct ObservationStore {
    connection: Connection,
}

impl ObservationStore {
    /// Open (creating if needed) Desktop's own observation store.
    ///
    /// Lives in `client/desktop/`, so this is a write this client is allowed
    /// to make; nothing here touches the shared root.
    pub fn open(root: &DataRoot) -> Result<Self, rusqlite::Error> {
        let path = root.client_dir().join("observations.sqlite3");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // SQLite opens by path, so the capability handles used elsewhere in
        // this crate cannot be handed to it. Refuse the escape explicitly
        // instead: a symlinked client directory or store file would let a
        // WAL switch and a schema rebuild land outside the namespace this
        // client is allowed to write.
        if path_escapes_namespace(root, &path) {
            return Err(rusqlite::Error::InvalidPath(path));
        }
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.busy_timeout(std::time::Duration::from_millis(5_000))?;

        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            // This store is derived data: every row can be observed again.
            // Rebuilding costs a little history, which is strictly better
            // than reading rows under the wrong assumptions.
            connection.execute_batch("DROP TABLE IF EXISTS observations")?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS observations (
                 account_id TEXT NOT NULL,
                 bucket_id TEXT NOT NULL,
                 sampled_at REAL NOT NULL,
                 used_percent REAL NOT NULL,
                 reset_at REAL,
                 raw_window_seconds INTEGER,
                 PRIMARY KEY(account_id, bucket_id, sampled_at)
             );
             CREATE INDEX IF NOT EXISTS observations_lookup
                 ON observations(account_id, bucket_id, sampled_at);",
        )?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { connection })
    }

    /// Open an existing store without initialising anything.
    ///
    /// `open` is a writer: it switches journal mode, runs DDL, stamps
    /// `user_version`, and rebuilds a store whose version it does not know.
    /// None of that may happen on a read — MCP `quota.get`, the first tray
    /// paint and the inspect diagnostic all read — and a rebuild on a
    /// downgrade would erase history a newer build wrote. Returns `None` when
    /// the file is absent or its schema is not this one.
    pub fn open_read_only(root: &DataRoot) -> Option<Self> {
        let path = root.client_dir().join("observations.sqlite3");
        if !path.is_file() || path_escapes_namespace(root, &path) {
            return None;
        }
        // `immutable=1` also keeps SQLite from creating the -shm sidecar of a
        // WAL database, so a read leaves no trace at all.
        let uri = format!(
            "file:{}?mode=ro&immutable=1",
            path.to_string_lossy().replace('?', "%3f")
        );
        let connection = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )
        .ok()?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .ok()?;
        if version != SCHEMA_VERSION {
            return None;
        }
        Some(Self { connection })
    }

    /// Record one observation. Re-recording the same instant is a no-op, so a
    /// refresh that returns a cached value cannot inflate the sample count and
    /// make the forecast look better evidenced than it is.
    pub fn record(
        &self,
        account_id: &str,
        bucket_id: &str,
        observation: StoredObservation,
    ) -> Result<(), rusqlite::Error> {
        self.connection.execute(
            "INSERT OR IGNORE INTO observations
                 (account_id, bucket_id, sampled_at, used_percent, reset_at, raw_window_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                account_id,
                bucket_id,
                observation.sampled_at,
                observation.used_percent,
                observation.reset_at,
                observation.raw_window_seconds,
            ],
        )?;
        Ok(())
    }

    /// Drop observations older than the retention horizon.
    pub fn prune(&self, now: f64) -> Result<usize, rusqlite::Error> {
        self.connection.execute(
            "DELETE FROM observations WHERE sampled_at < ?1",
            [now - RETENTION_SECONDS],
        )
    }

    /// Observations for one bucket at or after `since`, oldest first.
    pub fn observations(
        &self,
        account_id: &str,
        bucket_id: &str,
        since: f64,
    ) -> Result<Vec<Observation>, rusqlite::Error> {
        let mut statement = self.connection.prepare(
            "SELECT sampled_at, used_percent FROM observations
              WHERE account_id = ?1 AND bucket_id = ?2 AND sampled_at >= ?3
              ORDER BY sampled_at",
        )?;
        let rows = statement.query_map(rusqlite::params![account_id, bucket_id, since], |row| {
            Ok(Observation {
                sampled_at: row.get(0)?,
                used_percent: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Observations with the window each belonged to, for reconstructing
    /// cycles. Separate from `observations` because the forecast only needs
    /// the time and the percentage, and this carries more per row.
    /// Bounded at both ends. `record` refuses an observation from the future,
    /// but a clock that was ahead and has since been corrected leaves one
    /// behind, and cycle inference replays whatever comes last as the newest
    /// state — an open cycle at a percentage nobody reached, or a cycle closed
    /// on a reset that has not happened. `observations` needs no such bound
    /// because `compute` filters to the current window itself.
    pub fn dated_observations(
        &self,
        account_id: &str,
        bucket_id: &str,
        since: f64,
        until: f64,
    ) -> Result<Vec<super::cycles::DatedObservation>, rusqlite::Error> {
        let mut statement = self.connection.prepare(
            "SELECT sampled_at, used_percent, reset_at, raw_window_seconds FROM observations
              WHERE account_id = ?1 AND bucket_id = ?2
                AND sampled_at >= ?3 AND sampled_at <= ?4
              ORDER BY sampled_at",
        )?;
        let rows = statement.query_map(
            rusqlite::params![account_id, bucket_id, since, until],
            |row| {
                Ok(super::cycles::DatedObservation {
                    sampled_at: row.get(0)?,
                    used_percent: row.get(1)?,
                    reset_at: row.get(2).ok(),
                    raw_window_seconds: row.get(3).ok(),
                })
            },
        )?;
        rows.collect()
    }

    /// Every (account, bucket) pair the store holds, for diagnostics that
    /// need to walk what is there rather than what a caller expected.
    pub fn distinct_series(&self) -> Result<Vec<(String, String)>, rusqlite::Error> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT account_id, bucket_id FROM observations ORDER BY 1, 2")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    pub fn count(&self) -> Result<i64, rusqlite::Error> {
        self.connection
            .query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))
    }

    /// Copy observations out of the native app's timeline, once.
    ///
    /// Returns how many rows were adopted. Every failure mode — no file, no
    /// permission, an unknown schema, a corrupt database — reports zero
    /// rather than propagating, because a seed that cannot be read is a
    /// missing convenience, not a broken client.
    pub fn seed_from_native(&self, native_timeline: &Path, now: f64) -> usize {
        // `immutable=1` promises SQLite the file will not change under it,
        // which is what stops it from creating or touching the -shm sidecar
        // of a WAL database. The repository's read-only audit tolerates that
        // side effect for `session_index.sqlite3` alone; the native timeline
        // is a second file and must stay untouched. A native app writing
        // concurrently is exactly why this is a seed and not a live source.
        let uri = format!(
            "file:{}?mode=ro&immutable=1",
            native_timeline.to_string_lossy().replace('?', "%3f")
        );
        let Ok(source) = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        ) else {
            return 0;
        };
        let version: i64 = match source.query_row("PRAGMA user_version", [], |row| row.get(0)) {
            Ok(version) => version,
            Err(_) => return 0,
        };
        if version != NATIVE_SCHEMA_VERSION {
            return 0;
        }
        let Ok(mut statement) = source.prepare(
            "SELECT account_id, bucket_id, sampled_at, used_percent, reset_at, raw_window_seconds
               FROM fill_points WHERE sampled_at >= ?1 AND sampled_at <= ?2 ORDER BY sampled_at",
        ) else {
            return 0;
        };
        let cutoff = now - RETENTION_SECONDS;
        let Ok(rows) = statement.query_map([cutoff, now + SEED_UPPER_BOUND_SLACK], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                StoredObservation {
                    sampled_at: row.get(2)?,
                    used_percent: row.get(3)?,
                    reset_at: row.get(4).ok(),
                    raw_window_seconds: row.get(5).ok(),
                },
            ))
        }) else {
            return 0;
        };

        let mut adopted = 0usize;
        for row in rows.flatten() {
            if self.record(&row.0, &row.1, row.2).is_ok() {
                adopted += 1;
            }
        }
        adopted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, DataRoot) {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();
        (dir, root)
    }

    #[test]
    fn records_and_reads_back_in_order() {
        let (_dir, root) = temp_root();
        let store = ObservationStore::open(&root).unwrap();
        for (at, used) in [(300.0, 3.0), (100.0, 1.0), (200.0, 2.0)] {
            store
                .record(
                    "acct",
                    "five_hour",
                    StoredObservation {
                        sampled_at: at,
                        used_percent: used,
                        reset_at: Some(1_000.0),
                        raw_window_seconds: Some(18_000),
                    },
                )
                .unwrap();
        }
        let got = store.observations("acct", "five_hour", 0.0).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].used_percent, 1.0);
        assert_eq!(got[2].used_percent, 3.0);
    }

    #[test]
    fn re_recording_the_same_instant_does_not_inflate_the_sample() {
        let (_dir, root) = temp_root();
        let store = ObservationStore::open(&root).unwrap();
        let point = StoredObservation {
            sampled_at: 100.0,
            used_percent: 5.0,
            reset_at: None,
            raw_window_seconds: None,
        };
        store.record("a", "b", point).unwrap();
        store.record("a", "b", point).unwrap();
        // A cached refresh must not make the forecast look better evidenced
        // than it is.
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn prune_drops_only_what_is_past_the_horizon() {
        let (_dir, root) = temp_root();
        let store = ObservationStore::open(&root).unwrap();
        let now = 100.0 * 86_400.0;
        for age_days in [1.0, 30.0, 59.0, 61.0, 120.0] {
            store
                .record(
                    "a",
                    "b",
                    StoredObservation {
                        sampled_at: now - age_days * 86_400.0,
                        used_percent: 1.0,
                        reset_at: None,
                        raw_window_seconds: None,
                    },
                )
                .unwrap();
        }
        let removed = store.prune(now).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.count().unwrap(), 3);
    }

    // Creating a symlink on Windows needs elevation, so this boundary is
    // exercised where it can be: the guard itself is not platform-specific,
    // but the attack it refuses can only be staged here.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_store_path_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();

        // A link where the store belongs would let the WAL switch and the
        // schema rebuild land outside the namespace this client may write.
        let outside = dir.path().join("outside.sqlite3");
        std::fs::write(&outside, b"").unwrap();
        std::os::unix::fs::symlink(&outside, root.client_dir().join("observations.sqlite3"))
            .unwrap();

        assert!(ObservationStore::open(&root).is_err());
    }

    #[test]
    fn seeding_ignores_observations_from_the_future() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();
        let store = ObservationStore::open(&root).unwrap();
        let native = dir.path().join("fill_timeline.sqlite3");
        let now = 1_000_000.0;

        let source = Connection::open(&native).unwrap();
        source
            .execute_batch(
                "PRAGMA user_version = 1;
                 CREATE TABLE fill_points (
                     account_id TEXT NOT NULL, tool TEXT NOT NULL, bucket_id TEXT NOT NULL,
                     slot_start REAL NOT NULL, used_percent REAL NOT NULL, sampled_at REAL NOT NULL,
                     reset_at REAL, raw_window_seconds INTEGER,
                     PRIMARY KEY(account_id, bucket_id, slot_start));
                 INSERT INTO fill_points VALUES
                     ('a','claude','five_hour', 1.0, 10.0, 999000.0, NULL, NULL),
                     ('a','claude','five_hour', 2.0, 99.0, 2000000.0, NULL, NULL);",
            )
            .unwrap();
        drop(source);

        // The past row is adopted; the one dated a fortnight ahead is not,
        // because it would become the freshest sample and steer the slope.
        assert_eq!(store.seed_from_native(&native, now), 1);
        let got = store.observations("a", "five_hour", 0.0).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].used_percent, 10.0);
    }

    // Creating a symlink on Windows needs elevation, so this boundary is
    // exercised where it can be: the guard itself is not platform-specific,
    // but the attack it refuses can only be staged here.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_client_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();

        // The escape the first guard missed: canonicalising the client
        // directory would have made this external target the trusted anchor,
        // and everything below it would then look contained.
        let outside = dir.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(root.shared().join("client")).unwrap();
        std::os::unix::fs::symlink(&outside, root.client_dir()).unwrap();

        assert!(ObservationStore::open(&root).is_err());
        assert!(ObservationStore::open_read_only(&root).is_none());
        assert!(!outside.join("observations.sqlite3").exists());
    }

    #[test]
    fn a_read_only_open_initialises_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.client_dir()).unwrap();

        // Absent means absent: a read must not bring the store into being.
        assert!(ObservationStore::open_read_only(&root).is_none());
        assert!(!root.client_dir().join("observations.sqlite3").exists());

        let store = ObservationStore::open(&root).unwrap();
        store
            .record(
                "a",
                "b",
                StoredObservation {
                    sampled_at: 100.0,
                    used_percent: 5.0,
                    reset_at: None,
                    raw_window_seconds: None,
                },
            )
            .unwrap();
        drop(store);

        let reader = ObservationStore::open_read_only(&root).expect("existing store");
        assert_eq!(reader.count().unwrap(), 1);
        // A read-only handle cannot write, so a downgrade cannot rebuild the
        // store and erase what a newer build recorded.
        assert!(reader
            .record(
                "a",
                "c",
                StoredObservation {
                    sampled_at: 200.0,
                    used_percent: 6.0,
                    reset_at: None,
                    raw_window_seconds: None,
                }
            )
            .is_err());
    }

    #[test]
    fn seeding_is_optional_and_never_fails_loudly() {
        let (_dir, root) = temp_root();
        let store = ObservationStore::open(&root).unwrap();
        // Absent, and a file that is not a database at all.
        assert_eq!(
            store.seed_from_native(Path::new("/nonexistent/x.sqlite3"), 0.0),
            0
        );
        let junk = _dir.path().join("junk.sqlite3");
        std::fs::write(&junk, b"not a database").unwrap();
        assert_eq!(store.seed_from_native(&junk, 0.0), 0);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn seeding_adopts_a_native_timeline_and_refuses_a_future_schema() {
        let (dir, root) = temp_root();
        let store = ObservationStore::open(&root).unwrap();
        let native = dir.path().join("fill_timeline.sqlite3");
        let now = 1_000_000.0;

        let source = Connection::open(&native).unwrap();
        source
            .execute_batch(
                "PRAGMA user_version = 1;
                 CREATE TABLE fill_points (
                     account_id TEXT NOT NULL, tool TEXT NOT NULL, bucket_id TEXT NOT NULL,
                     slot_start REAL NOT NULL, used_percent REAL NOT NULL, sampled_at REAL NOT NULL,
                     reset_at REAL, raw_window_seconds INTEGER,
                     PRIMARY KEY(account_id, bucket_id, slot_start));
                 INSERT INTO fill_points VALUES
                     ('oauth-claude','claude','five_hour', 100.0, 10.0, 999000.0, 1000500.0, 18000),
                     ('oauth-claude','claude','five_hour', 200.0, 20.0, 999500.0, 1000500.0, 18000);",
            )
            .unwrap();
        drop(source);

        assert_eq!(store.seed_from_native(&native, now), 2);
        let got = store
            .observations("oauth-claude", "five_hour", 0.0)
            .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].used_percent, 20.0);

        // A schema this build does not know is left alone rather than guessed.
        let future = Connection::open(&native).unwrap();
        future.pragma_update(None, "user_version", 99).unwrap();
        drop(future);
        let (_d2, root2) = temp_root();
        let fresh = ObservationStore::open(&root2).unwrap();
        assert_eq!(fresh.seed_from_native(&native, now), 0);
    }
}
