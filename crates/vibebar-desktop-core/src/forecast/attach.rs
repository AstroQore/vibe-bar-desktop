//! Wiring the forecast into a refresh: record what was seen, then say what it
//! implies.
//!
//! Kept apart from `QuotaEngine` because forecasting must never be able to
//! break a refresh. Every failure here — an unwritable store, a locked
//! database, a bucket with no reset time — costs a forecast and nothing else;
//! the quota numbers still arrive.

use crate::model::AccountQuota;
use crate::paths::DataRoot;

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

        for bucket in account.buckets.iter_mut() {
            let (Some(reset_at), Some(window)) = (bucket.reset_at, bucket.raw_window_seconds)
            else {
                continue;
            };
            let Ok(observations) =
                store.observations(&account.account_id, &bucket.id, now - LOOKBACK_SECONDS)
            else {
                continue;
            };
            bucket.forecast = compute(&ForecastInput {
                used_percent: bucket.used_percent,
                reset_at,
                raw_window_seconds: window,
                now,
                observations,
                completed_cycles: Vec::new(),
            });
        }
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
    // Opening the store creates it, so a read only forecasts when a store is
    // already there. A client that has never refreshed shows no forecast,
    // which is honest: it has observed nothing.
    if !root.client_dir().join("observations.sqlite3").is_file() {
        return;
    }
    let Ok(store) = ObservationStore::open(root) else {
        return;
    };
    for account in accounts.iter_mut() {
        for bucket in account.buckets.iter_mut() {
            let (Some(reset_at), Some(window)) = (bucket.reset_at, bucket.raw_window_seconds)
            else {
                continue;
            };
            let Ok(observations) =
                store.observations(&account.account_id, &bucket.id, now - LOOKBACK_SECONDS)
            else {
                continue;
            };
            bucket.forecast = compute(&ForecastInput {
                used_percent: bucket.used_percent,
                reset_at,
                raw_window_seconds: window,
                now,
                observations,
                completed_cycles: Vec::new(),
            });
        }
    }
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
    let Ok(store) = ObservationStore::open(root) else {
        return 0;
    };
    // Only worth seeding when this client has little of its own. Past that,
    // Desktop's own record is the better one: it is current, and it is the
    // only one that keeps growing on a machine without the native app.
    match store.count() {
        Ok(count) if count < 100 => store.seed_from_native(&root.fill_timeline_file(), now),
        _ => 0,
    }
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
