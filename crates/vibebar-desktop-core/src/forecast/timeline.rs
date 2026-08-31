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

/// One observation, keyed the way both stores key them.
#[derive(Debug, Clone, Copy)]
pub struct StoredObservation {
    pub sampled_at: f64,
    pub used_percent: f64,
    pub reset_at: Option<f64>,
    pub raw_window_seconds: Option<i64>,
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
        let Ok(source) = Connection::open_with_flags(
            native_timeline,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
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
               FROM fill_points WHERE sampled_at >= ?1 ORDER BY sampled_at",
        ) else {
            return 0;
        };
        let cutoff = now - RETENTION_SECONDS;
        let Ok(rows) = statement.query_map([cutoff], |row| {
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
