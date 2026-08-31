//! Quota forecasting: will this bucket survive until the provider refills it,
//! and if so how much capacity goes unused?
//!
//! A port of the native app's `QuotaPaceForecast`. The two implementations
//! must agree to the byte on the same history — a client that disagrees with
//! the other about whether a quota is at risk is worse than one that says
//! nothing — so the shared vectors in `contracts/forecast-v1.json` are
//! checked by both.
//!
//! Quota observations are the only source of consumption. Token history
//! contributes calendar weights (when someone tends to work), never a
//! token-to-quota conversion.
//!
//! ## What this slice does not carry yet
//!
//! The native forecast weights time by a weekday×hour activity heatmap and
//! scales the historical projection by a token-volume trend. Desktop has
//! neither yet, and the native implementation has an explicit, tested path
//! for their absence: with no heatmap, activity weight is simply elapsed
//! hours, and with no daily token history the trend multiplier is exactly 1.
//! This port implements that path. The vectors pin it, so adding the heatmap
//! later is a visible change to both lanes rather than a silent divergence.

mod compute;
mod model;
mod timeline;

pub use compute::compute;
pub use model::{Confidence, Diagnostics, ForecastInput, Observation, QuotaPaceForecast, Verdict};
pub use timeline::{ObservationStore, StoredObservation};

/// Uniform activity weight between two instants, in hours.
///
/// The native `ActivityProfile` reduces to exactly this when no heatmap is
/// available: `end.timeIntervalSince(start) / 3600`. Keeping it behind a
/// function rather than inlining the division is what lets the heatmap
/// version land later without touching every call site.
pub(crate) fn activity_weight(from: f64, to: f64) -> f64 {
    if to <= from {
        return 0.0;
    }
    (to - from) / 3_600.0
}

/// The instant reached by accumulating `weight` hours of activity after
/// `start`, never past `limit`. The uniform counterpart of the native
/// profile's `date(after:accumulating:noLaterThan:)`.
pub(crate) fn date_after_accumulating(start: f64, weight: f64, limit: f64) -> Option<f64> {
    if !weight.is_finite() || weight <= 0.0 {
        return None;
    }
    let reached = start + weight * 3_600.0;
    Some(reached.min(limit))
}

pub(crate) fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    if value.is_nan() {
        return lower;
    }
    value.max(lower).min(upper)
}

/// Median of a sample, or `None` when it is empty.
///
/// Even-length samples average the two middle values, matching Swift's
/// `median` helper — an off-by-one here would move every projection.
pub(crate) fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    } else {
        Some(sorted[mid])
    }
}

/// Median absolute deviation. Callers scale by 1.4826 to read it as a
/// standard deviation, exactly as the native implementation does.
pub(crate) fn median_absolute_deviation(values: &[f64]) -> f64 {
    let Some(centre) = median(values) else {
        return 0.0;
    };
    let deviations: Vec<f64> = values.iter().map(|v| (v - centre).abs()).collect();
    median(&deviations).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_matches_swift_for_both_parities() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0]), Some(3.0));
        // Even length averages the two middle values rather than picking one.
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&[5.0, 1.0, 3.0]), Some(3.0));
    }

    #[test]
    fn mad_is_the_median_of_absolute_deviations() {
        // median = 3; deviations = [2,1,0,1,2]; median of those = 1.
        assert_eq!(median_absolute_deviation(&[1.0, 2.0, 3.0, 4.0, 5.0]), 1.0);
        assert_eq!(median_absolute_deviation(&[]), 0.0);
    }

    #[test]
    fn uniform_activity_weight_is_elapsed_hours() {
        assert_eq!(activity_weight(0.0, 3_600.0), 1.0);
        assert_eq!(activity_weight(0.0, 1_800.0), 0.5);
        // A non-advancing or reversed span carries no weight.
        assert_eq!(activity_weight(100.0, 100.0), 0.0);
        assert_eq!(activity_weight(100.0, 50.0), 0.0);
    }

    #[test]
    fn accumulating_stops_at_the_limit() {
        assert_eq!(date_after_accumulating(0.0, 2.0, 10_000.0), Some(7_200.0));
        // The reset is a ceiling: run-out cannot be reported past it.
        assert_eq!(date_after_accumulating(0.0, 5.0, 7_200.0), Some(7_200.0));
        assert_eq!(date_after_accumulating(0.0, 0.0, 10.0), None);
    }
}

#[cfg(test)]
mod contract_tests {
    use super::compute;
    use super::model::{ForecastInput, Observation};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        evaluation_fractions: Vec<f64>,
        numeric_tolerance: f64,
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: Input,
        expected: Vec<serde_json::Value>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Input {
        raw_window_seconds: i64,
        reset_at: f64,
        observations: Vec<Observation>,
    }

    /// The vectors in `docs/contracts/forecast-v1.json` were produced by the
    /// native Swift implementation, so this is an independent oracle rather
    /// than a snapshot of this port. A verdict or confidence that differs at
    /// all, or a number that differs by more than the stated tolerance, means
    /// the two clients would tell the same user different things about the
    /// same quota — the failure this whole contract exists to prevent.
    #[test]
    fn matches_the_native_forecast_on_the_shared_vectors() {
        let raw = include_str!("../../../../docs/contracts/forecast-v1.json");
        let doc: Doc = serde_json::from_str(raw).expect("contract parses");
        let mut checked = 0usize;

        for case in &doc.cases {
            let window = case.input.raw_window_seconds as f64;
            let mut expectations = case.expected.iter();

            for frac in &doc.evaluation_fractions {
                let now = case.input.reset_at - window * (1.0 - frac);
                let visible: Vec<Observation> = case
                    .input
                    .observations
                    .iter()
                    .copied()
                    .filter(|p| p.sampled_at <= now)
                    .collect();
                let Some(last) = visible.last().copied() else {
                    continue;
                };
                let got = compute(&ForecastInput {
                    used_percent: last.used_percent,
                    reset_at: case.input.reset_at,
                    raw_window_seconds: case.input.raw_window_seconds,
                    now,
                    observations: visible,
                    completed_cycles: vec![],
                });
                let Some(got) = got else { continue };
                let want = expectations
                    .next()
                    .unwrap_or_else(|| panic!("{}: more results than expectations", case.name));

                let want_str = |k: &str| want[k].as_str().unwrap_or_default().to_string();
                let want_num = |k: &str| want[k].as_f64().unwrap_or_default();
                let ctx = format!("{} @ {:.0}%", case.name, frac * 100.0);

                assert_eq!(
                    serde_json::to_value(got.verdict).unwrap().as_str().unwrap(),
                    want_str("verdict"),
                    "{ctx}: verdict"
                );
                assert_eq!(
                    serde_json::to_value(got.confidence)
                        .unwrap()
                        .as_str()
                        .unwrap(),
                    want_str("confidence"),
                    "{ctx}: confidence"
                );
                for (label, got_v, want_v) in [
                    (
                        "confidenceScore",
                        got.confidence_score,
                        want_num("confidenceScore"),
                    ),
                    (
                        "projected",
                        got.projected_used_percent,
                        want_num("projected"),
                    ),
                    ("lower", got.projected_used_lower_percent, want_num("lower")),
                    ("upper", got.projected_used_upper_percent, want_num("upper")),
                    ("target", got.target_remaining_percent, want_num("target")),
                    ("planned", got.planned_used_percent, want_num("planned")),
                    (
                        "runOutAt",
                        got.run_out_at.unwrap_or(-1.0),
                        want_num("runOutAt"),
                    ),
                ] {
                    assert!(
                        (got_v - want_v).abs() <= doc.numeric_tolerance,
                        "{ctx}: {label} {got_v} vs native {want_v}"
                    );
                }
                assert_eq!(
                    got.current_observation_count as f64,
                    want_num("observationCount"),
                    "{ctx}: observationCount"
                );
                assert_eq!(
                    got.diagnostics.recent_sample_count as f64,
                    want_num("recentSampleCount"),
                    "{ctx}: recentSampleCount"
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 16,
            "expected the full vector set, checked {checked}"
        );
    }
}
