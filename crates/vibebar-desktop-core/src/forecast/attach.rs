//! Wiring the forecast into a refresh: record what was seen, then say what it
//! implies.
//!
//! Kept apart from `QuotaEngine` because forecasting must never be able to
//! break a refresh. Every failure here — an unwritable store, a locked
//! database, a bucket with no reset time — costs a forecast and nothing else;
//! the quota numbers still arrive.

use crate::model::AccountQuota;
use crate::paths::DataRoot;

use super::cycles::{self, CycleSummary};
use super::model::ForecastInput;
use super::timeline::{ObservationStore, StoredObservation};
use super::{compute, Observation};

/// How far back a forecast looks. Wide enough to cover a monthly window and
/// the completed cycles before it, narrow enough that a bucket refreshed
/// every minute for two months stays a cheap query.
const LOOKBACK_SECONDS: f64 = 45.0 * 86_400.0;

/// Record this refresh's observations and attach a forecast to each bucket
/// that has enough history to support one.
///
/// Demo roots are skipped entirely: a synthetic home must not accumulate
/// state, and a forecast over invented numbers would be a lie told with a
/// straight face.
pub fn attach_forecasts(root: &DataRoot, accounts: &mut [AccountQuota], now: f64) {
    if root.is_demo() {
        return;
    }
    let Ok(store) = ObservationStore::open(root) else {
        return;
    };

    for account in accounts.iter_mut() {
        // A failed observation is not worth reporting: the value is already
        // in the view, and the forecast simply has one sample less.
        for bucket in &account.buckets {
            let _ = store.record(
                &account.account_id,
                &bucket.id,
                StoredObservation {
                    sampled_at: account.queried_at,
                    used_percent: bucket.used_percent,
                    reset_at: bucket.reset_at,
                    raw_window_seconds: bucket.raw_window_seconds,
                },
            );
        }

        forecast_account(&store, account, now);
    }

    let _ = store.prune(now);
}

/// Attach forecasts from the history that already exists, recording nothing.
///
/// This is what a *read* gets: the inspect diagnostic, MCP `quota.get`, and
/// the first tray paint all go through here. Recording on a read would make
/// merely looking at cached data mutate persistent state, and would break the
/// before/after audit that proves this client leaves the data root alone.
pub fn attach_cached_forecasts(root: &DataRoot, accounts: &mut [AccountQuota]) {
    attach_cached_forecasts_at(root, accounts, crate::providers::now_unix());
}

/// `attach_cached_forecasts` with an explicit clock, so a test can evaluate a
/// synthetic window without waiting for one.
pub fn attach_cached_forecasts_at(root: &DataRoot, accounts: &mut [AccountQuota], now: f64) {
    if root.is_demo() {
        return;
    }
    // A genuinely read-only handle: no journal switch, no DDL, no
    // user_version write, and no rebuild of a schema this build does not
    // know — a downgrade must not erase what a newer build recorded. Absent
    // or unreadable means no forecast, which is honest: nothing was observed.
    let Some(store) = ObservationStore::open_read_only(root) else {
        return;
    };
    for account in accounts.iter_mut() {
        forecast_account(&store, account, now);
    }
}

/// Forecast every bucket of one account from the history the store holds.
///
/// One query per bucket serves both halves of the input: the observations the
/// projections read, and the cycles they are compared against. Querying twice
/// would double the cost of the most expensive part of a refresh for nothing.
fn forecast_account(store: &ObservationStore, account: &mut AccountQuota, now: f64) {
    for bucket in account.buckets.iter_mut() {
        let (Some(reset_at), Some(window)) = (bucket.reset_at, bucket.raw_window_seconds) else {
            continue;
        };
        let Ok(history) =
            store.dated_observations(&account.account_id, &bucket.id, now - LOOKBACK_SECONDS, now)
        else {
            continue;
        };
        let (completed, _) = cycles::summarize(&history);
        let observations: Vec<Observation> = history
            .iter()
            .map(|point| Observation {
                sampled_at: point.sampled_at,
                used_percent: point.used_percent,
            })
            .collect();
        bucket.forecast = compute(&ForecastInput {
            used_percent: bucket.used_percent,
            reset_at,
            raw_window_seconds: window,
            now,
            observations,
            completed_cycles: cycles::as_forecast_input(&completed),
        });
    }
}

/// The cycles behind one bucket's history, oldest first, with the open one
/// returned separately. Backs the reset-history chart.
///
/// Read-only: the chart is drawn from what a refresh already recorded, so
/// opening a card can never create or mutate the store.
pub fn cycles_for(
    root: &DataRoot,
    account_id: &str,
    bucket_id: &str,
    lookback_seconds: f64,
) -> (Vec<CycleSummary>, Option<CycleSummary>) {
    cycles_for_at(
        root,
        account_id,
        bucket_id,
        lookback_seconds,
        crate::providers::now_unix(),
    )
}

/// `cycles_for` with an explicit clock, so a test can replay a synthetic
/// history without waiting for one.
pub fn cycles_for_at(
    root: &DataRoot,
    account_id: &str,
    bucket_id: &str,
    lookback_seconds: f64,
    now: f64,
) -> (Vec<CycleSummary>, Option<CycleSummary>) {
    let Some(store) = ObservationStore::open_read_only(root) else {
        return (Vec::new(), None);
    };
    let Ok(history) = store.dated_observations(account_id, bucket_id, now - lookback_seconds, now)
    else {
        return (Vec::new(), None);
    };
    cycles::summarize(&history)
}

/// Adopt the native app's observation history, once per launch.
///
/// Returns how many observations were adopted, for the diagnostic to report.
/// Doing this at launch rather than per refresh keeps a Mac with both clients
/// from re-reading tens of thousands of rows every minute.
pub fn seed_from_native_once(root: &DataRoot, now: f64) -> usize {
    if root.is_demo() {
        return 0;
    }
    // Nothing to seed from means nothing to create. Checking first keeps a
    // read-only diagnostic on a machine that has never refreshed from
    // creating a store merely by looking.
    if !root.fill_timeline_file().is_file() {
        return 0;
    }
    // Only worth seeding when this client has little of its own. An existing
    // store is asked read-only, so the count cannot itself create one.
    if let Some(existing) = ObservationStore::open_read_only(root) {
        if existing.count().unwrap_or(i64::MAX) >= 100 {
            return 0;
        }
    }
    let Ok(store) = ObservationStore::open(root) else {
        return 0;
    };
    store.seed_from_native(&root.fill_timeline_file(), now)
}

/// Observations held for one bucket, for diagnostics and the history chart.
pub fn observations_for(
    root: &DataRoot,
    account_id: &str,
    bucket_id: &str,
    since: f64,
) -> Vec<Observation> {
    ObservationStore::open(root)
        .and_then(|store| store.observations(account_id, bucket_id, since))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{QuotaBucket, QuotaOrigin, ToolType};

    fn account(now: f64, used: f64, reset_at: Option<f64>) -> AccountQuota {
        AccountQuota {
            account_id: "acct".into(),
            tool: ToolType::Claude,
            buckets: vec![QuotaBucket::new(
                "five_hour",
                "5 Hours",
                "5h",
                used,
                reset_at,
                Some(18_000),
                None,
            )],
            plan: None,
            queried_at: now,
            origin: QuotaOrigin::Live,
            error: None,
        }
    }

    #[test]
    fn a_single_refresh_records_but_cannot_yet_forecast_confidently() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = 1_000_000.0;
        let mut accounts = vec![account(now, 10.0, Some(now + 9_000.0))];

        attach_forecasts(&root, &mut accounts, now);

        // One observation is enough to produce a forecast, but not enough for
        // it to claim confidence.
        let forecast = accounts[0].buckets[0].forecast.as_ref().expect("forecast");
        assert_eq!(forecast.current_observation_count, 1);
        assert!(matches!(
            forecast.confidence,
            super::super::Confidence::Learning
        ));
    }

    #[test]
    fn history_accumulates_across_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let reset = 1_020_000.0;
        for (i, used) in [2.0, 4.0, 6.0, 8.0, 10.0].iter().enumerate() {
            let now = 1_003_000.0 + i as f64 * 600.0;
            let mut accounts = vec![account(now, *used, Some(reset))];
            attach_forecasts(&root, &mut accounts, now);
            if i == 4 {
                let forecast = accounts[0].buckets[0].forecast.as_ref().expect("forecast");
                assert_eq!(forecast.current_observation_count, 5);
                // A rising series must project above where it stands now.
                assert!(forecast.projected_used_percent >= 10.0);
            }
        }
    }

    #[test]
    fn a_bucket_without_a_reset_time_gets_no_forecast() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = 1_000_000.0;
        let mut accounts = vec![account(now, 10.0, None)];
        attach_forecasts(&root, &mut accounts, now);
        assert!(accounts[0].buckets[0].forecast.is_none());
    }

    /// Reading must not write. This regressed once: routing the cached view
    /// through the recording path made `inspect`, MCP `quota.get` and the
    /// first tray paint create and mutate the store.
    #[test]
    fn a_cached_read_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = 1_000_000.0;
        let mut accounts = vec![account(now, 10.0, Some(now + 9_000.0))];

        attach_cached_forecasts(&root, &mut accounts);

        assert!(!root.client_dir().join("observations.sqlite3").exists());
        // No history means no forecast, rather than one invented from a
        // single live value.
        assert!(accounts[0].buckets[0].forecast.is_none());
    }

    #[test]
    fn a_cached_read_uses_history_a_refresh_already_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let reset = 1_020_000.0;
        for (i, used) in [2.0, 4.0, 6.0].iter().enumerate() {
            let now = 1_003_000.0 + i as f64 * 600.0;
            attach_forecasts(&root, &mut [account(now, *used, Some(reset))], now);
        }
        let before = std::fs::metadata(root.client_dir().join("observations.sqlite3"))
            .unwrap()
            .len();

        let mut accounts = vec![account(1_004_800.0, 8.0, Some(reset))];
        attach_cached_forecasts_at(&root, &mut accounts, 1_004_800.0);

        let forecast = accounts[0].buckets[0].forecast.as_ref().expect("forecast");
        assert_eq!(
            forecast.current_observation_count, 3,
            "reads the recorded history"
        );
        let after = std::fs::metadata(root.client_dir().join("observations.sqlite3"))
            .unwrap()
            .len();
        assert_eq!(before, after, "a read must not grow the store");
    }

    /// The chart reads; it must not write. `cycles_for` backs an IPC call the
    /// UI makes whenever a card is drawn, so a store created here would mean
    /// merely looking at the app mutates persistent state.
    #[test]
    fn asking_for_cycles_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));

        let (completed, current) = cycles_for(&root, "acct", "five_hour", 45.0 * 86_400.0);

        assert!(completed.is_empty());
        assert!(current.is_none());
        assert!(!root.client_dir().join("observations.sqlite3").exists());
    }

    #[test]
    fn cycles_for_reads_what_refreshes_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = crate::providers::now_unix();
        // Two windows: one that ends, then a fresh one after the reset.
        for (offset, used, reset) in [
            (-7_200.0, 20.0, 1_800.0),
            (-6_600.0, 45.0, 1_800.0),
            (-1_800.0, 3.0, 19_800.0),
            (-600.0, 9.0, 19_800.0),
        ] {
            let at = now + offset;
            attach_forecasts(&root, &mut [account(at, used, Some(now + reset))], at);
        }

        let (completed, current) = cycles_for(&root, "acct", "five_hour", 45.0 * 86_400.0);

        assert_eq!(completed.len(), 1, "the window that reset");
        assert_eq!(completed[0].peak_used_percent, 45.0);
        let current = current.expect("the window still open");
        assert_eq!(current.peak_used_percent, 9.0);
        assert_eq!(current.observation_count, 2);
    }

    /// An observation left behind by a clock that was ahead must not be
    /// replayed as the newest state. `record` refuses one from the future, but
    /// a correction after the fact leaves it stored, and cycle inference takes
    /// whatever comes last as current — an open cycle at a percentage nobody
    /// has reached, or a cycle closed on a reset that has not happened.
    #[test]
    fn an_observation_from_the_future_is_not_replayed_as_current() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = 1_000_000.0;
        let reset = now + 9_000.0;
        for (offset, used) in [(-1_200.0, 4.0), (-600.0, 6.0)] {
            let at = now + offset;
            attach_forecasts(&root, &mut [account(at, used, Some(reset))], at);
        }
        // Stored while the clock was an hour ahead.
        let store = ObservationStore::open(&root).unwrap();
        store
            .record(
                "acct",
                "five_hour",
                StoredObservation {
                    sampled_at: now + 3_600.0,
                    used_percent: 99.0,
                    reset_at: Some(reset + 3_600.0),
                    raw_window_seconds: Some(18_000),
                },
            )
            .expect("a skewed row can exist however it got there");
        assert_eq!(
            store
                .dated_observations("acct", "five_hour", 0.0, now + 86_400.0)
                .unwrap()
                .len(),
            3,
            "the skewed row has to actually be stored for this to test anything"
        );
        // The reader opens with `immutable=1`, which ignores the WAL entirely,
        // so a live writer's row is invisible to it and this would pass without
        // testing anything at all.
        drop(store);

        let (_, current) = cycles_for_at(&root, "acct", "five_hour", 45.0 * 86_400.0, now);
        let current = current.expect("an open cycle");
        assert_eq!(
            current.peak_used_percent, 6.0,
            "the future-dated reading became the current state"
        );
    }

    #[test]
    fn a_demo_root_accumulates_nothing_and_forecasts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(root.is_demo());
        let now = 1_000_000.0;
        let mut accounts = vec![account(now, 10.0, Some(now + 9_000.0))];

        attach_forecasts(&root, &mut accounts, now);

        assert!(accounts[0].buckets[0].forecast.is_none());
        assert!(!root.client_dir().join("observations.sqlite3").exists());
        assert_eq!(seed_from_native_once(&root, now), 0);
    }
}
