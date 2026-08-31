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
            if cycle.window_end <= cycle.window_start {
                return None;
            }
            let total = activity_weight(cycle.window_start, cycle.window_end).max(0.001);
            let matching = observations
                .iter()
                .filter(|p| p.sampled_at >= cycle.window_start && p.sampled_at <= cycle.window_end)
                .map(|p| {
                    let progress = clamp(
                        activity_weight(cycle.window_start, p.sampled_at) / total,
                        0.0,
                        1.0,
                    );
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
