//! Forecast inputs and results.
//!
//! Field names and string values match the native `QuotaPaceForecast` so the
//! shared vectors decode into both lanes unchanged.

use serde::{Deserialize, Serialize};

/// Is the bucket going to last, and what does that cost in unused capacity?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Projected to finish the cycle with a sensible reserve.
    Enough,
    /// Projected to finish with capacity to spare — paid for, unlikely to be
    /// used. Not an instruction to manufacture work.
    Surplus,
    /// The median says it lasts, but the pessimistic bound reaches the cap.
    Watch,
    /// The median itself reaches the cap before the reset.
    AtRisk,
    /// Too little evidence to say. Never dressed up as a real verdict.
    Learning,
}

/// How much the verdict should be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Learning,
    Medium,
    High,
}

/// One stored quota observation. Mirrors the native `FillTimelinePoint` and
/// the `fill_points` row the native app writes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    /// Unix seconds.
    pub sampled_at: f64,
    /// 0–100.
    pub used_percent: f64,
}

/// A completed quota cycle: how high usage peaked before the provider reset
/// it. Mirrors the native `SubscriptionWindowSample` fields the forecast
/// actually reads.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedCycle {
    /// Unix seconds, from what the provider reported at the time.
    pub window_start: f64,
    /// Unix seconds, the observed refill time.
    pub window_end: f64,
    /// Unix seconds of the last observation that belonged to this cycle.
    ///
    /// Not derivable from the boundaries, and not the same as `window_end`:
    /// the reading that detected the refill is stamped between them and
    /// belongs to the cycle it opened. Bounding on time cannot separate the
    /// two, because the observation is written by a different call with its
    /// own clock reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<f64>,
    pub peak_used_percent: f64,
}

/// Everything one forecast needs. Bundled so the shared vectors have exactly
/// one shape to describe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastInput {
    /// Current usage, 0–100.
    pub used_percent: f64,
    /// Unix seconds when the provider refills this bucket.
    pub reset_at: f64,
    /// Length of the quota window in seconds.
    pub raw_window_seconds: i64,
    /// Unix seconds; the moment the forecast is computed for.
    pub now: f64,
    #[serde(default)]
    pub observations: Vec<Observation>,
    #[serde(default)]
    pub completed_cycles: Vec<CompletedCycle>,
}

/// Explainable inputs, retained so a surface can show its work instead of
/// presenting a black-box verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_projection_used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical_projection_used_percent: Option<f64>,
    pub behavioral_projection_used_percent: f64,
    pub behavioral_progress_percent: f64,
    pub observation_coverage_percent: f64,
    pub history_coverage_percent: f64,
    pub freshness_percent: f64,
    pub recent_sample_count: usize,
    pub comparable_cycle_count: usize,
}

/// The forecast for one independently resettable bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaPaceForecast {
    pub verdict: Verdict,
    pub confidence: Confidence,
    pub confidence_score: f64,
    pub current_used_percent: f64,
    pub planned_used_percent: f64,
    /// Median projected demand at reset. **May exceed 100**: the visible quota
    /// is capped but the shortage severity is not, and clamping here would
    /// erase the difference between just short and hopelessly short.
    pub projected_used_percent: f64,
    pub projected_used_lower_percent: f64,
    pub projected_used_upper_percent: f64,
    pub target_remaining_percent: f64,
    /// Unix seconds when usage is projected to reach the cap, when it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_out_at: Option<f64>,
    pub completed_cycle_count: usize,
    pub current_observation_count: usize,
    pub diagnostics: Diagnostics,
}

impl QuotaPaceForecast {
    /// Capacity projected to go unused, never negative.
    pub fn projected_remaining_percent(&self) -> f64 {
        (100.0 - self.projected_used_percent).max(0.0)
    }

    /// Remaining-at-reset interval, low first.
    pub fn projected_remaining_range(&self) -> (f64, f64) {
        (
            (100.0 - self.projected_used_upper_percent).max(0.0),
            (100.0 - self.projected_used_lower_percent).max(0.0),
        )
    }
}
