//! The forecast itself, ported statement by statement from the native
//! `QuotaPaceForecast.compute`. Constants are deliberately inline rather than
//! named: every one of them appears in the Swift source at the same place,
//! and a reader comparing the two should be able to do it line by line.

use super::model::{
    CompletedCycle, Confidence, Diagnostics, ForecastInput, Observation, QuotaPaceForecast, Verdict,
};
use super::{activity_weight, clamp, date_after_accumulating, median, median_absolute_deviation};

struct RecentSlope {
    rate: Option<f64>,
    spread: f64,
    sample_count: usize,
}

/// Forecast one bucket, or `None` when the inputs cannot support one.
///
/// Returns `None` for a bucket with no reset time or window length, and for a
/// window whose remaining time exceeds its own length by more than 10% — that
/// combination means the reset time is stale or wrong, and a forecast built on
/// it would be confident nonsense.
pub fn compute(input: &ForecastInput) -> Option<QuotaPaceForecast> {
    if input.raw_window_seconds <= 0 {
        return None;
    }
    let duration = input.raw_window_seconds as f64;
    let reset_at = input.reset_at;

    // The native `QuotaWindowEvaluation.date` gate: a window whose reset has
    // already passed has no forecast unless the caller allows a grace period,
    // and Desktop does not. Without this a stale reset time would be
    // forecast against as though the cycle were still open.
    if reset_at <= input.now {
        return None;
    }
    let evaluation = input.now;

    let remaining_time = reset_at - evaluation;
    if remaining_time > duration * 1.1 {
        return None;
    }

    let window_start = reset_at - duration;
    let actual = clamp(input.used_percent, 0.0, 100.0);

    // With no activity heatmap the profile is uniform, so total and elapsed
    // activity are just the hours in the window and the hours so far.
    let total_activity = activity_weight(window_start, reset_at).max(0.001);
    let elapsed_activity = clamp(
        activity_weight(window_start, evaluation),
        0.0,
        total_activity,
    );
    let future_activity = (total_activity - elapsed_activity).max(0.0);
    let behavioral_progress = clamp(elapsed_activity / total_activity, 0.0, 1.0);

    // Observations belonging to the window now open. The 300s lead-in and 60s
    // trailing slack absorb clock skew between the sampler and the provider's
    // own reset boundary.
    let mut current_points: Vec<Observation> = input
        .observations
        .iter()
        .copied()
        .filter(|p| p.sampled_at >= window_start - 300.0 && p.sampled_at <= evaluation + 60.0)
        .collect();
    current_points.sort_by(|a, b| a.sampled_at.total_cmp(&b.sampled_at));

    let recent = recent_slope(&current_points);
    let completed = &input.completed_cycles;
    let historical_additions =
        historical_remaining_usage(completed, &input.observations, behavioral_progress);

    // No daily token history in this slice, so the trend multiplier is
    // exactly 1 — the native value when there is no baseline to compare to.
    let trend = 1.0_f64;

    let recent_projection = recent
        .rate
        .filter(|rate| *rate > 0.0)
        .map(|rate| actual + rate * future_activity);
    let historical_projection = median(&historical_additions).map(|m| actual + m * trend);

    // Three views of the same question, weighted by how much each is worth
    // believing. The weights are the native ones.
    let mut candidates: Vec<(f64, f64)> = Vec::new();
    if let Some(value) = recent_projection {
        let reliability = (recent.sample_count as f64 / 6.0).min(1.0);
        candidates.push((value, 0.52 * reliability));
    }
    if let Some(value) = historical_projection {
        let reliability = (historical_additions.len() as f64 / 5.0).min(1.0);
        candidates.push((value, 0.34 * reliability));
    }

    // A low-weight behavioural fallback is always retained. It is the old
    // linear model when no activity profile exists, and becomes the user's
    // real weekday/hour shape once a heatmap does.
    let fallback = if behavioral_progress > 0.015 {
        actual / behavioral_progress
    } else {
        actual
    };
    let behavioral_projection = fallback * trend;
    candidates.push((behavioral_projection, 0.14));

    let weight_sum: f64 = candidates.iter().map(|(_, w)| w).sum();
    let raw_projection = if weight_sum > 0.0 {
        candidates.iter().map(|(v, w)| v * w).sum::<f64>() / weight_sum
    } else {
        actual
    };
    // Usage cannot un-happen: the projection never dips below what is already
    // spent.
    let projected = actual.max(raw_projection);

    let observation_coverage = match (current_points.first(), current_points.last()) {
        (Some(first), Some(last)) => {
            let span = (last.sampled_at - first.sampled_at).max(0.0);
            let elapsed = (evaluation - window_start).max(1.0);
            let count_score = (current_points.len() as f64 / 10.0).min(1.0);
            let span_score = (span / elapsed).min(1.0);
            count_score * 0.65 + span_score * 0.35
        }
        _ => 0.0,
    };
    let history_coverage = (completed.len() as f64 / 5.0).min(1.0);
    let freshness = match current_points.last() {
        Some(last) => {
            // A short window is sampled far more often than a weekly one, so
            // "stale" has to be measured against the bucket's own cadence.
            let natural_slot: f64 = if input.raw_window_seconds <= 6 * 3_600 {
                5.0 * 60.0
            } else {
                3_600.0
            };
            clamp(
                1.0 - (evaluation - last.sampled_at) / (natural_slot * 3.0).max(60.0),
                0.0,
                1.0,
            )
        }
        None => 0.0,
    };
    // Activity coverage needs the heatmap this slice does not have, so its
    // 0.12 share of the score is unearned rather than assumed.
    let activity_coverage = 0.0_f64;
    let confidence_score = clamp(
        observation_coverage * 0.38
            + history_coverage * 0.30
            + freshness * 0.20
            + activity_coverage * 0.12,
        0.0,
        1.0,
    );
    let confidence = if confidence_score >= 0.72 {
        Confidence::High
    } else if confidence_score >= 0.35 {
        Confidence::Medium
    } else {
        Confidence::Learning
    };

    // Less certainty asks for a bigger reserve.
    let target_remaining = clamp(5.0 + (1.0 - confidence_score) * 8.0, 5.0, 13.0);
    let peaks: Vec<f64> = completed.iter().map(|c| c.peak_used_percent).collect();
    let historical_spread = median_absolute_deviation(&peaks) * 1.4826;
    let recent_spread = recent.spread * future_activity;
    let uncertainty = clamp(
        (18.0 * (1.0 - confidence_score)).max(4.0)
            + (historical_spread * 0.35 + recent_spread * 0.5).min(12.0),
        4.0,
        28.0,
    );
    let lower = actual.max(projected - uncertainty);
    let upper = projected + uncertainty;

    // A high remaining estimate alone is not waste: require both a material
    // median surplus and a pessimistic bound that still clears the target.
    let median_surplus = (100.0 - projected - target_remaining).max(0.0);
    let conservative_surplus = (100.0 - upper - target_remaining).max(0.0);

    let verdict = if projected >= 100.0 {
        Verdict::AtRisk
    } else if upper >= 100.0 {
        Verdict::Watch
    } else if confidence == Confidence::Learning {
        Verdict::Learning
    } else if median_surplus >= 25.0 && conservative_surplus >= 10.0 {
        Verdict::Surplus
    } else {
        Verdict::Enough
    };

    let target_used = 100.0 - target_remaining;
    let planned = clamp(target_used * behavioral_progress, 0.0, target_used);

    let run_out_at = if upper >= 100.0 && actual < 100.0 {
        if let Some(rate) = recent.rate.filter(|r| *r > 0.0) {
            let needed_weight = (100.0 - actual) / rate;
            date_after_accumulating(evaluation, needed_weight, reset_at)
        } else {
            let additional = projected - actual;
            if additional > 0.0 {
                let fraction = clamp((100.0 - actual) / additional, 0.0, 1.0);
                date_after_accumulating(evaluation, future_activity * fraction, reset_at)
            } else {
                None
            }
        }
    } else if actual >= 100.0 {
        // Already spent: it ran out now, not at some projected moment.
        Some(evaluation)
    } else {
        None
    };

    Some(QuotaPaceForecast {
        verdict,
        confidence,
        confidence_score,
        current_used_percent: actual,
        planned_used_percent: planned,
        projected_used_percent: projected,
        projected_used_lower_percent: lower,
        projected_used_upper_percent: upper,
        target_remaining_percent: target_remaining,
        run_out_at,
        completed_cycle_count: completed.len(),
        current_observation_count: current_points.len(),
        diagnostics: Diagnostics {
            recent_projection_used_percent: recent_projection,
            historical_projection_used_percent: historical_projection,
            behavioral_projection_used_percent: behavioral_projection,
            behavioral_progress_percent: behavioral_progress * 100.0,
            observation_coverage_percent: observation_coverage * 100.0,
            history_coverage_percent: history_coverage * 100.0,
            freshness_percent: freshness * 100.0,
            recent_sample_count: recent.sample_count,
            comparable_cycle_count: historical_additions.len(),
        },
    })
}

/// Consumption rate from consecutive observations, as percent per hour.
///
/// Only forward, plausible steps count: a negative delta is the provider
/// resetting the bucket, and a jump over 45 points in one step is a data
/// artefact rather than someone's afternoon.
fn recent_slope(points: &[Observation]) -> RecentSlope {
    if points.len() < 2 {
        return RecentSlope {
            rate: None,
            spread: 0.0,
            sample_count: 0,
        };
    }
    let start = points.len().saturating_sub(18);
    let recent = &points[start..];
    let mut slopes: Vec<f64> = Vec::new();
    for pair in recent.windows(2) {
        let delta = pair[1].used_percent - pair[0].used_percent;
        if !(0.0..=45.0).contains(&delta) {
            continue;
        }
        let activity = activity_weight(pair[0].sampled_at, pair[1].sampled_at);
        if activity <= 0.002 {
            continue;
        }
        slopes.push(delta / activity);
    }
    match median(&slopes) {
        Some(rate) => RecentSlope {
            rate: Some(rate),
            spread: median_absolute_deviation(&slopes) * 1.4826,
            sample_count: slopes.len(),
        },
        None => RecentSlope {
            rate: None,
            spread: 0.0,
            sample_count: 0,
        },
    }
}

/// For each completed cycle, how much more was spent after the point in that
/// cycle that matches where this one stands now.
///
/// When a comparable moment exists within 22% of the current progress, the
/// answer is measured. Otherwise it is estimated by scaling that cycle's peak
/// by the progress left — a weaker signal, deliberately still included so a
/// sparse history is not silently discarded.
fn historical_remaining_usage(
    cycles: &[CompletedCycle],
    observations: &[Observation],
    current_progress: f64,
) -> Vec<f64> {
    cycles
        .iter()
        .filter_map(|cycle| {
            // The stored start tracks what the provider reported, and
            // measurement says to leave it alone: several buckets refill far
            // more often than their stated window, so a short span is usually
            // the truth. Against the interval between observed refills the
            // stored start is right for 86-100% of cycles on every bucket,
            // where reconstructing it from the window length managed 14% on
            // the worst.
            let start = cycle.window_start;
            if cycle.window_end <= start {
                return None;
            }
            let total = activity_weight(start, cycle.window_end).max(0.001);
            // Bounded by the last observation this cycle absorbed, not by its
            // end. The reading that detected the refill belongs to the cycle it
            // opened, and it is stamped a shade *before* the recorded end — the
            // timeline and the history are written by separate calls, each
            // taking its own clock reading — so no bound on time separates
            // them. Without this a 60% to 5% refill doubled the answer, because
            // a nearly-finished window looks for its comparison at exactly the
            // progress that stray reading occupies.
            let observation_end = cycle.last_seen_at.unwrap_or(cycle.window_end);
            let matching = observations
                .iter()
                .filter(|p| p.sampled_at >= start && p.sampled_at <= observation_end)
                .map(|p| {
                    let progress = clamp(activity_weight(start, p.sampled_at) / total, 0.0, 1.0);
                    ((progress - current_progress).abs(), p.used_percent)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));

            match matching {
                Some((distance, used)) if distance <= 0.22 => {
                    Some((cycle.peak_used_percent - used).max(0.0))
                }
                _ => Some((cycle.peak_used_percent * (1.0 - current_progress)).max(0.0)),
            }
        })
        .collect()
}

#[cfg(test)]
mod cycle_span_tests {
    use super::*;
    use crate::forecast::model::{CompletedCycle, ForecastInput};

    const WEEK: i64 = 7 * 86_400;

    fn ramp(from: f64, to: f64, start: f64, end: f64, count: usize) -> Vec<Observation> {
        (0..count)
            .map(|i| {
                let t = i as f64 / (count - 1) as f64;
                Observation {
                    sampled_at: start + (end - start) * t,
                    used_percent: from + (to - from) * t,
                }
            })
            .collect()
    }

    /// The observation that detected a refill belongs to the cycle that
    /// follows. It is stamped exactly at the end, which is where a
    /// nearly-finished current window looks for its comparison, so an
    /// end-inclusive filter let a 60% to 5% refill double the answer.
    ///
    /// Stated as an A/B: adding a sample from the next cycle must not change
    /// what this one contributes.
    #[test]
    fn a_reading_from_the_next_cycle_does_not_change_this_one() {
        let week = WEEK as f64;
        let now = 1_800_000_000.0;
        // Current window nearly over, so the closest comparable progress is
        // exactly where the stray sample sits.
        let reset = now + week / 40.0;
        let start = reset - week;
        let end = start - week;

        let current = ramp(0.0, 55.0, start, start + week * 23.0 / 24.0, 24);
        let history = ramp(0.0, 60.0, end - week, end - week / 12.0, 12);
        // Stamped a shade before the cycle's end, which is how it arrives in
        // production: the timeline and the history are written by separate
        // calls, each taking its own clock reading.
        let after_refill = Observation {
            sampled_at: end - 0.4,
            used_percent: 5.0,
        };

        let projection = |observations: Vec<Observation>| {
            compute(&ForecastInput {
                used_percent: 55.0,
                reset_at: reset,
                raw_window_seconds: WEEK,
                now,
                observations,
                completed_cycles: vec![CompletedCycle {
                    window_start: end - week,
                    window_end: end,
                    last_seen_at: Some(end - week / 12.0),
                    peak_used_percent: 60.0,
                }],
            })
            .expect("forecast")
            .diagnostics
            .historical_projection_used_percent
            .expect("a comparable cycle")
        };

        let base: Vec<Observation> = current.iter().chain(history.iter()).copied().collect();
        let mut with_stray = base.clone();
        with_stray.push(after_refill);

        let without = projection(base);
        let with = projection(with_stray);
        assert!(
            (with - without).abs() < 1e-9,
            "a reading from the next cycle changed what the previous one \
             contributed: {without} became {with}"
        );
    }
}
