//! Subscription cycles, inferred from a stream of observations.
//!
//! The native app draws these as "Reset history — each bar is one quota
//! cycle", and the forecast's historical projection compares against them.
//!
//! Neither available signal identifies a cycle on its own. Grouping by the
//! `reset_at` an observation carried looks obvious and is wrong: a rolling
//! window's reset time slides with every poll, so exact grouping turned one
//! Claude 5-hour bucket's 7,338 real observations into 5,814 "cycles".
//! Watching for usage to drop is wrong the other way: a provider can refill
//! and be used again between two polls, and the drop never appears.
//!
//! So this is a port of the native `SubscriptionHistoryStore` inference — a
//! material usage drop, a reset time advancing by a meaningful fraction of the
//! window, or two weaker versions of those signals agreeing. The thresholds
//! are the native ones; changing them here alone would make the two clients
//! disagree about how many cycles the same history contains.

use super::model::CompletedCycle;

/// A reset advance this large is independent evidence on its own.
const STRONG_RESET_ADVANCE_FRACTION: f64 = 0.10;
/// A reset advance this large counts only alongside another signal.
const WEAK_RESET_ADVANCE_FRACTION: f64 = 0.01;
/// Percentage points a usage drop must clear to stand alone.
const MINIMUM_STRONG_USAGE_DROP: f64 = 0.5;
/// Percentage points a usage drop must clear to count as corroboration.
const MINIMUM_WEAK_USAGE_DROP: f64 = 0.25;
/// No reset advance below five minutes is meaningful, however short the
/// window: that is polling jitter.
const MINIMUM_MEANINGFUL_ADVANCE_SECONDS: f64 = 300.0;
/// A provider may reset slightly before its stated time; native allows the
/// same two minutes.
const BOUNDARY_SLACK_SECONDS: f64 = 120.0;
/// Fallback window length for a bucket that never reported one, from native.
const ASSUMED_WINDOW_SECONDS: f64 = 86_400.0;

/// Why a cycle was considered finished. Worth keeping: "the provider reset on
/// schedule" and "usage visibly refilled" are different enough that a surface
/// may want to say which happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionReason {
    RefillDetected,
    ScheduledReset,
}

/// One inferred cycle. The one still open carries `completion: None`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleSummary {
    /// Unix seconds the window ended — the *observed* refill time once a cycle
    /// completes, which beats a stale forecast when a provider resets early.
    pub window_end: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_start: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_window_seconds: Option<i64>,
    /// Highest usage seen during the cycle, 0–100.
    pub peak_used_percent: f64,
    /// Last usage seen, 0–100.
    pub last_used_percent: f64,
    pub observation_count: usize,
    pub first_seen_at: f64,
    pub last_seen_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<CompletionReason>,
}

impl CycleSummary {
    /// Capacity that went unused when the window reset.
    pub fn remaining_percent_at_reset(&self) -> f64 {
        (100.0 - self.peak_used_percent).max(0.0)
    }
}

/// An observation as stored, carrying the window it belonged to.
#[derive(Debug, Clone, Copy)]
pub struct DatedObservation {
    pub sampled_at: f64,
    pub used_percent: f64,
    pub reset_at: Option<f64>,
    pub raw_window_seconds: Option<i64>,
}

fn clamp_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}

/// How far usage must fall to count as a refill rather than a correction.
/// Proportional, so a bucket sitting at 3% is not declared refilled by the
/// same absolute drop that would mean nothing at 90%.
fn material_refill_threshold(previous: f64) -> f64 {
    (previous * 0.2).clamp(MINIMUM_STRONG_USAGE_DROP, 15.0)
}

/// A drop from both the previous reading *and* the cycle's peak. Requiring
/// both means one spurious high reading cannot make the next ordinary one look
/// like a refill.
fn is_material_refill(previous: f64, peak: f64, current: f64) -> bool {
    let previous = clamp_percent(previous);
    let peak = clamp_percent(peak);
    let current = clamp_percent(current);
    let threshold = material_refill_threshold(previous);
    previous - current >= threshold && peak - current >= threshold
}

/// Does this observation end the cycle in progress?
fn completion_reason(
    current: &CycleSummary,
    new_used_percent: f64,
    new_reset_at: f64,
    now: f64,
) -> Option<CompletionReason> {
    if is_material_refill(
        current.last_used_percent,
        current.peak_used_percent,
        new_used_percent,
    ) {
        return Some(CompletionReason::RefillDetected);
    }

    let window = current
        .raw_window_seconds
        .map_or(ASSUMED_WINDOW_SECONDS, |seconds| seconds as f64);
    let reset_advance = (new_reset_at - current.window_end).max(0.0);
    let meaningful_advance =
        (window * STRONG_RESET_ADVANCE_FRACTION).max(MINIMUM_MEANINGFUL_ADVANCE_SECONDS);
    if reset_advance >= meaningful_advance {
        // A provider can refill and be used again between polls, so a clear
        // move into the next window stands even when the new usage is not
        // lower than the old.
        return Some(CompletionReason::ScheduledReset);
    }

    let weak_advance =
        (window * WEAK_RESET_ADVANCE_FRACTION).max(MINIMUM_MEANINGFUL_ADVANCE_SECONDS);
    let crossed_old_boundary = now >= current.window_end - BOUNDARY_SLACK_SECONDS;
    if crossed_old_boundary && reset_advance >= weak_advance {
        return Some(CompletionReason::ScheduledReset);
    }

    let usage_drop = (current.last_used_percent - new_used_percent).max(0.0);
    let usage_threshold = material_refill_threshold(current.last_used_percent);
    if usage_drop >= MINIMUM_WEAK_USAGE_DROP
        && reset_advance >= weak_advance
        && usage_drop / usage_threshold + reset_advance / meaningful_advance >= 1.0
    {
        // Two concordant weak signals identify a refill neither could claim
        // alone.
        return Some(CompletionReason::ScheduledReset);
    }
    None
}

/// Replay observations into cycles: the completed ones oldest first, then the
/// one still open.
///
/// Observations must be ordered by `sampled_at` — that is how the store
/// returns them, and it is what makes this replay equivalent to the native
/// store's incremental updates over the same sequence.
pub fn summarize(observations: &[DatedObservation]) -> (Vec<CycleSummary>, Option<CycleSummary>) {
    let mut completed = Vec::new();
    let mut current: Option<CycleSummary> = None;

    for point in observations {
        let Some(reset_at) = point.reset_at else {
            continue;
        };
        if !reset_at.is_finite() || !point.used_percent.is_finite() {
            continue;
        }
        let used = clamp_percent(point.used_percent);
        let at = point.sampled_at;

        let Some(open) = current.as_mut() else {
            current = Some(new_cycle(used, reset_at, point.raw_window_seconds, at));
            continue;
        };

        match completion_reason(open, used, reset_at, at) {
            Some(reason) => {
                open.completion = Some(reason);
                // The observed refill time beats a stale reset forecast when a
                // provider resets early or late.
                open.window_end = at;
                completed.push(*open);
                current = Some(new_cycle(used, reset_at, point.raw_window_seconds, at));
            }
            None => {
                open.window_end = reset_at;
                // A bucket that stops reporting its window keeps the length it
                // last reported, rather than falling back to the assumed day.
                if let Some(seconds) = point.raw_window_seconds {
                    open.raw_window_seconds = Some(seconds);
                    open.window_start = Some(reset_at - seconds as f64);
                } else if open.raw_window_seconds.is_some() {
                    open.window_start =
                        open.raw_window_seconds.map(|s| reset_at - s as f64);
                }
                open.peak_used_percent = open.peak_used_percent.max(used);
                open.last_used_percent = used;
                open.observation_count += 1;
                open.last_seen_at = open.last_seen_at.max(at);
            }
        }
    }

    (completed, current)
}

fn new_cycle(
    used: f64,
    reset_at: f64,
    raw_window_seconds: Option<i64>,
    now: f64,
) -> CycleSummary {
    CycleSummary {
        window_end: reset_at,
        window_start: raw_window_seconds.map(|seconds| reset_at - seconds as f64),
        raw_window_seconds,
        peak_used_percent: used,
        last_used_percent: used,
        observation_count: 1,
        first_seen_at: now,
        last_seen_at: now,
        completion: None,
    }
}

/// The completed cycles in the shape the forecast consumes.
///
/// A cycle that never learned its window length falls back to when it was
/// first seen, which is what native does. Assuming a day instead would hand
/// the projection a span the data never covered.
pub fn as_forecast_input(cycles: &[CycleSummary]) -> Vec<CompletedCycle> {
    cycles
        .iter()
        .map(|cycle| CompletedCycle {
            window_start: cycle.window_start.unwrap_or(cycle.first_seen_at),
            window_end: cycle.window_end,
            peak_used_percent: cycle.peak_used_percent,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE_HOURS: i64 = 18_000;

    fn point(sampled_at: f64, used: f64, reset_at: f64) -> DatedObservation {
        DatedObservation {
            sampled_at,
            used_percent: used,
            reset_at: Some(reset_at),
            raw_window_seconds: Some(FIVE_HOURS),
        }
    }

    /// The bug this module exists to avoid. A rolling window's reset time
    /// advances a little on every poll; that is drift, not a cycle. Grouping
    /// by reset time turned 7,338 real Claude observations into 5,814 of them.
    #[test]
    fn a_rolling_reset_time_is_drift_not_a_cycle() {
        let points: Vec<DatedObservation> = (0..200)
            .map(|i| {
                let t = i as f64 * 60.0;
                // Reset slides one minute per poll; usage creeps up.
                point(t, 10.0 + i as f64 * 0.05, t + 18_000.0)
            })
            .collect();
        let (completed, current) = summarize(&points);
        assert!(
            completed.is_empty(),
            "drift produced {} phantom cycles",
            completed.len()
        );
        assert_eq!(current.expect("one open cycle").observation_count, 200);
    }

    #[test]
    fn a_material_usage_drop_closes_the_cycle() {
        let points = [
            point(0.0, 40.0, 18_000.0),
            point(600.0, 60.0, 18_000.0),
            // 60 -> 5 clears both the previous-reading and the peak test.
            point(1_200.0, 5.0, 36_000.0),
        ];
        let (completed, current) = summarize(&points);
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].peak_used_percent, 60.0);
        assert_eq!(
            completed[0].completion,
            Some(CompletionReason::RefillDetected)
        );
        // The observed refill time replaces the forecast reset.
        assert_eq!(completed[0].window_end, 1_200.0);
        assert_eq!(current.expect("new cycle").peak_used_percent, 5.0);
    }

    /// A provider can refill and be used again before the next poll, so usage
    /// never appears to drop. The reset moving a whole window forward suffices.
    #[test]
    fn a_window_sized_reset_advance_closes_the_cycle_without_a_drop() {
        let points = [point(0.0, 40.0, 18_000.0), point(600.0, 45.0, 36_000.0)];
        let (completed, _) = summarize(&points);
        assert_eq!(completed.len(), 1);
        assert_eq!(
            completed[0].completion,
            Some(CompletionReason::ScheduledReset)
        );
    }

    /// Neither signal clears its own bar; together they do.
    #[test]
    fn two_weak_signals_agree_where_neither_would_stand_alone() {
        let points = [
            point(0.0, 10.0, 18_000.0),
            // A 1.6-point drop is under the 2.0 refill threshold and a 1,500s
            // advance is under the 1,800s strong bar; the weighted sum clears 1.
            point(600.0, 8.4, 19_500.0),
        ];
        let (completed, _) = summarize(&points);
        assert_eq!(completed.len(), 1);
        assert_eq!(
            completed[0].completion,
            Some(CompletionReason::ScheduledReset)
        );
    }

    #[test]
    fn a_small_drop_alone_is_a_correction_not_a_refill() {
        let points = [
            point(0.0, 40.0, 18_000.0),
            // Under the proportional threshold, and the reset has not moved.
            point(600.0, 38.0, 18_000.0),
        ];
        let (completed, current) = summarize(&points);
        assert!(completed.is_empty());
        let open = current.expect("still open");
        assert_eq!(open.peak_used_percent, 40.0);
        assert_eq!(open.last_used_percent, 38.0);
    }

    /// One spurious spike must not make the next ordinary reading look like a
    /// refill: the drop is measured from the peak as well as the last value.
    #[test]
    fn a_drop_back_from_a_spike_is_not_a_refill_unless_it_clears_the_peak() {
        let points = [
            point(0.0, 50.0, 18_000.0),
            point(600.0, 52.0, 18_000.0),
            // Below the previous reading, but nowhere near below the peak.
            point(1_200.0, 51.0, 18_000.0),
        ];
        let (completed, _) = summarize(&points);
        assert!(completed.is_empty());
    }

    #[test]
    fn observations_without_a_reset_time_are_skipped() {
        let points = [
            DatedObservation {
                sampled_at: 100.0,
                used_percent: 50.0,
                reset_at: None,
                raw_window_seconds: None,
            },
            point(200.0, 10.0, 18_000.0),
        ];
        let (_, current) = summarize(&points);
        assert_eq!(current.expect("open").peak_used_percent, 10.0);
    }

    /// A bucket that stops reporting its window keeps the length it last
    /// reported; falling back to the assumed day would move the strong-advance
    /// bar from 30 minutes to 2.4 hours and hide real resets.
    #[test]
    fn a_missing_window_length_falls_back_to_the_last_one_seen() {
        let points = [
            point(0.0, 40.0, 18_000.0),
            DatedObservation {
                sampled_at: 600.0,
                used_percent: 45.0,
                reset_at: Some(36_000.0),
                raw_window_seconds: None,
            },
        ];
        let (completed, _) = summarize(&points);
        assert_eq!(completed.len(), 1, "an 18,000s advance ends a 5-hour window");
    }

    #[test]
    fn forecast_input_carries_each_windows_own_span() {
        let cycles = [CycleSummary {
            window_end: 20_000.0,
            window_start: Some(2_000.0),
            raw_window_seconds: Some(FIVE_HOURS),
            peak_used_percent: 80.0,
            last_used_percent: 4.0,
            observation_count: 5,
            first_seen_at: 2_100.0,
            last_seen_at: 19_900.0,
            completion: Some(CompletionReason::ScheduledReset),
        }];
        let input = as_forecast_input(&cycles);
        assert_eq!(input[0].window_start, 2_000.0);
        assert_eq!(input[0].window_end, 20_000.0);
        assert_eq!(input[0].peak_used_percent, 80.0);
    }

    /// A cycle whose bucket never reported a window length takes its own first
    /// observation as the start, matching native's `windowStart ?? firstSeenAt`.
    /// Assuming a day would hand the historical projection a span the data
    /// never covered, and it filters observations by that span.
    #[test]
    fn a_cycle_with_no_window_length_starts_where_it_was_first_seen() {
        let cycles = [CycleSummary {
            window_end: 20_000.0,
            window_start: None,
            raw_window_seconds: None,
            peak_used_percent: 40.0,
            last_used_percent: 40.0,
            observation_count: 3,
            first_seen_at: 18_500.0,
            last_seen_at: 19_900.0,
            completion: Some(CompletionReason::RefillDetected),
        }];
        let input = as_forecast_input(&cycles);
        assert_eq!(input[0].window_start, 18_500.0);
    }

    #[test]
    fn remaining_at_reset_never_goes_negative() {
        let cycle = CycleSummary {
            window_end: 1.0,
            window_start: None,
            raw_window_seconds: None,
            peak_used_percent: 100.0,
            last_used_percent: 100.0,
            observation_count: 1,
            first_seen_at: 0.0,
            last_seen_at: 1.0,
            completion: None,
        };
        assert_eq!(cycle.remaining_percent_at_reset(), 0.0);
    }
}

/// Check the inference against the native app's own inferred cycles.
///
/// `SubscriptionHistoryStore` is the Swift implementation of this algorithm,
/// and on a machine that has run the native app its output sits in
/// `~/.vibebar/subscription_history.json`. That makes it an oracle rather than
/// a snapshot: different code, same observations.
///
/// Native's `legacyTimelineMigration` cycles are excluded: they came from a
/// one-time import of an older file format, not from the inference at all.
///
/// The count is bracketed rather than matched. A long gap in the timeline does
/// not hide a boundary from the inference — the first observation after the gap
/// carries the whole reset advance — but it does collapse several resets into
/// one. So the floor is every boundary the observations bracket closely, and
/// the ceiling is every boundary native inferred over the same span. On this
/// developer's Mac one bucket has 61 of its 174 boundaries inside gaps
/// totalling 578 hours of app downtime and lands on its floor; the other five
/// buckets hit their ceiling exactly.
///
/// Ignored by default — it needs a machine with real history.
/// `cargo test -p vibebar-desktop-core native_timeline -- --ignored --nocapture`
#[cfg(test)]
mod real_data {
    use super::*;
    use crate::forecast::ObservationStore;
    use crate::paths::DataRoot;
    use std::collections::HashMap;

    /// Swift's `JSONEncoder` writes `Date` as seconds since 2001-01-01, so
    /// every timestamp in the native file is this far behind Unix time.
    const APPLE_EPOCH_OFFSET: f64 = 978_307_200.0;
    /// Two observations further apart than this could hide a whole boundary.
    const OBSERVABLE_GAP_SECONDS: f64 = 2_400.0;

    /// Native's inferred cycle boundaries per (account, bucket).
    fn native_cycles(path: &std::path::Path) -> HashMap<(String, String), Vec<f64>> {
        let mut out: HashMap<(String, String), Vec<f64>> = HashMap::new();
        let Ok(text) = std::fs::read_to_string(path) else {
            return out;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
            return out;
        };
        for sample in doc["samples"].as_array().into_iter().flatten() {
            if sample["completedAt"].is_null()
                || sample["completionReason"] == "legacyTimelineMigration"
            {
                continue;
            }
            let (Some(account), Some(bucket), Some(end)) = (
                sample["accountId"].as_str(),
                sample["bucketId"].as_str(),
                sample["windowEnd"].as_f64(),
            ) else {
                continue;
            };
            out.entry((account.to_string(), bucket.to_string()))
                .or_default()
                .push(end + APPLE_EPOCH_OFFSET);
        }
        for ends in out.values_mut() {
            ends.sort_by(f64::total_cmp);
        }
        out
    }

    /// Could a boundary at `at` have been seen from these observations?
    fn observable(points: &[DatedObservation], at: f64) -> bool {
        let index = points.partition_point(|point| point.sampled_at < at);
        let (Some(before), Some(after)) = (
            index.checked_sub(1).map(|i| points[i].sampled_at),
            points.get(index).map(|point| point.sampled_at),
        ) else {
            return false;
        };
        after - before <= OBSERVABLE_GAP_SECONDS
    }


    #[test]
    #[ignore = "compares stored spans against observed cadence"]
    fn is_a_short_span_wrong_or_just_short() {
        let home = std::env::var_os("HOME").unwrap();
        let native = std::path::PathBuf::from(&home).join(".vibebar").join("fill_timeline.sqlite3");
        if !native.is_file() { return; }
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = crate::providers::now_unix();
        let store = crate::forecast::ObservationStore::open(&root).unwrap();
        store.seed_from_native(&native, now);
        for (account, bucket) in store.distinct_series().unwrap() {
            let points = store.dated_observations(&account, &bucket, 0.0, now).unwrap();
            let (completed, _) = summarize(&points);
            if completed.len() < 6 { continue; }
            let window = points.iter().rev().find_map(|p| p.raw_window_seconds).unwrap_or(0) as f64;
            let mut stored_matches = 0usize;
            let mut window_matches = 0usize;
            let mut total = 0usize;
            for pair in completed.windows(2) {
                let observed = pair[1].window_end - pair[0].window_end;
                if !(observed.is_finite() && observed > 0.0) { continue; }
                let Some(start) = pair[1].window_start else { continue };
                let stored = pair[1].window_end - start;
                total += 1;
                // Within 25% of the interval the provider actually took.
                if (stored - observed).abs() <= observed * 0.25 { stored_matches += 1; }
                if (window - observed).abs() <= observed * 0.25 { window_matches += 1; }
            }
            if total == 0 { continue; }
            eprintln!(
                "{bucket}: window {:.0}h | of {total} cycles, stored start right {:.0}%, window-length right {:.0}%",
                window / 3600.0,
                100.0 * stored_matches as f64 / total as f64,
                100.0 * window_matches as f64 / total as f64,
            );
        }
    }

    #[test]
    #[ignore = "reads the developer's own ~/.vibebar"]
    fn the_inference_agrees_with_the_native_app() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let vibebar = std::path::PathBuf::from(&home).join(".vibebar");
        let native = vibebar.join("fill_timeline.sqlite3");
        if !native.is_file() {
            eprintln!("no native timeline at {}", native.display());
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at_non_demo(dir.path().join(".vibebar"));
        let now = crate::providers::now_unix();
        let store = ObservationStore::open(&root).expect("store");
        eprintln!(
            "adopted {} observations",
            store.seed_from_native(&native, now)
        );
        let oracle = native_cycles(&vibebar.join("subscription_history.json"));
        if oracle.is_empty() {
            eprintln!("no native subscription history to compare against");
            return;
        }

        let mut compared = 0usize;
        let mut disagreed = Vec::new();
        for (account, bucket) in store.distinct_series().expect("series") {
            let points = store
                .dated_observations(&account, &bucket, 0.0, now)
                .expect("observations");
            let (mine, open) = summarize(&points);
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                continue;
            };
            let Some(theirs) = oracle.get(&(account.clone(), bucket.clone())) else {
                continue;
            };
            let in_span = theirs
                .iter()
                .filter(|end| **end >= first.sampled_at && **end <= last.sampled_at);
            let (visible, hidden): (Vec<f64>, Vec<f64>) =
                in_span.partition(|end| observable(&points, **end));
            if visible.is_empty() {
                continue;
            }
            compared += 1;
            let (floor, ceiling) = (visible.len(), visible.len() + hidden.len());
            eprintln!(
                "{bucket}: mine {} within [{floor}, {ceiling}] ({} hidden by gaps){}",
                mine.len(),
                hidden.len(),
                match open {
                    Some(o) => format!(", open at {:.0}%", o.peak_used_percent),
                    None => String::new(),
                }
            );
            // Two boundaries either side of the span edges legitimately differ.
            const EDGE_SLACK: usize = 2;
            if mine.len() + EDGE_SLACK < floor || mine.len() > ceiling + EDGE_SLACK {
                disagreed.push(format!(
                    "{bucket}: mine {} outside [{floor}, {ceiling}]",
                    mine.len()
                ));
            }
            for cycle in &mine {
                assert!(cycle.window_end <= now);
                assert!((0.0..=100.0).contains(&cycle.peak_used_percent));
                assert!(cycle.observation_count > 0);
            }
        }
        assert!(compared > 0, "nothing was comparable");
        assert!(
            disagreed.is_empty(),
            "the two implementations disagree: {disagreed:?}"
        );
    }
}
