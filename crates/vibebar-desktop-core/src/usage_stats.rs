//! The Usage Stats query: the per-request events the cost scanner retains,
//! filtered and folded the way the Workbench page reads them. Everything
//! here is local-time bucketed because the page speaks in the user's days.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::cost::{priced_cost_micros, UsageEvent};
use crate::model::ToolType;

/// The most buckets a trend chart draws interactively; past this the
/// granularity coarsens, matching the native page's picker.
pub const MAX_TREND_BUCKETS: usize = 1_200;
const DEFAULT_REQUEST_LIMIT: usize = 200;
const MAX_REQUEST_LIMIT: usize = 2_000;
const FALLBACK_RANGE_SECONDS: f64 = 30.0 * 86_400.0;
/// Events stamped later than this past `now` are clock skew or corruption,
/// not usage; the cost aggregation refuses them the same way.
const FUTURE_TOLERANCE_SECONDS: f64 = 300.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendBucket {
    Hour,
    #[default]
    Day,
    Week,
}

impl TrendBucket {
    /// The bucket a range reads best in when the caller has no preference.
    pub fn recommended(duration_seconds: f64) -> Self {
        if duration_seconds <= 86_400.0 {
            TrendBucket::Hour
        } else if duration_seconds <= 45.0 * 86_400.0 {
            TrendBucket::Day
        } else {
            TrendBucket::Week
        }
    }

    fn coarser(self) -> Option<Self> {
        match self {
            TrendBucket::Hour => Some(TrendBucket::Day),
            TrendBucket::Day => Some(TrendBucket::Week),
            TrendBucket::Week => None,
        }
    }

    fn seconds(self) -> f64 {
        match self {
            TrendBucket::Hour => 3_600.0,
            TrendBucket::Day => 86_400.0,
            TrendBucket::Week => 7.0 * 86_400.0,
        }
    }

    /// Whether a range fits the interactive budget at this granularity.
    pub fn fits(self, duration_seconds: f64) -> bool {
        ((duration_seconds / self.seconds()).ceil() as usize) < MAX_TREND_BUCKETS
    }

    /// The local-time start of the bucket holding `timestamp`.
    pub fn floor(self, timestamp: f64) -> Option<f64> {
        let local = local_time(timestamp)?;
        if self == TrendBucket::Hour {
            // Whole hours from the epoch in the local offset: aligned to the
            // local clock and defined inside a DST gap too.
            let offset = f64::from(local.offset().local_minus_utc());
            return Some(((timestamp + offset) / 3_600.0).floor() * 3_600.0 - offset);
        }
        let naive = match self {
            TrendBucket::Hour => unreachable!(),
            TrendBucket::Day => local.date_naive().and_time(NaiveTime::MIN),
            TrendBucket::Week => {
                let date = local.date_naive();
                let back = i64::from(date.weekday().num_days_from_monday());
                (date - Duration::days(back)).and_time(NaiveTime::MIN)
            }
        };
        from_local_naive(naive)
    }

    /// The start of the bucket after the one starting at `start`. Hours are
    /// fixed spans, so they advance in absolute time and never land in a
    /// DST gap; days and weeks advance by calendar date and, when local
    /// midnight does not exist that day, take the first hour that does.
    pub fn next(self, start: f64) -> Option<f64> {
        if self == TrendBucket::Hour {
            return Some(start + 3_600.0);
        }
        let naive = local_time(start)?.naive_local();
        let days = if self == TrendBucket::Day { 1 } else { 7 };
        let date = naive.date() + Duration::days(days);
        from_local_naive(date.and_time(NaiveTime::MIN))
            .or_else(|| from_local_naive(date.and_time(NaiveTime::MIN) + Duration::hours(1)))
    }
}

fn local_time(timestamp: f64) -> Option<DateTime<Local>> {
    DateTime::<Utc>::from_timestamp_millis((timestamp * 1_000.0).round() as i64)
        .map(|date| date.with_timezone(&Local))
}

fn from_local_naive(naive: chrono::NaiveDateTime) -> Option<f64> {
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|date| date.timestamp() as f64)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageStatsQuery {
    /// Unix seconds; both unset means all recorded time.
    pub range_start: Option<f64>,
    pub range_end: Option<f64>,
    /// Harness labels (`ToolType::hierarchy().tool`); unset means every
    /// harness, empty means none — the native All chip is a switch.
    pub harnesses: Option<Vec<String>>,
    pub models: Option<Vec<String>>,
    pub granularity: Option<TrendBucket>,
    pub request_limit: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub requests: u64,
    pub fresh_input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total_tokens: u64,
    /// Unset when nothing in range carried a price, so the page shows "—"
    /// rather than a $0 that reads as free.
    pub cost_micros: Option<i64>,
    pub unpriced_requests: u64,
    /// Share of the input side served from cache.
    pub cache_hit_rate: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub bucket_start: f64,
    pub requests: u64,
    pub fresh_input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total_tokens: u64,
    pub cost_micros: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTrend {
    pub harness: String,
    pub company: String,
    pub points: Vec<TrendPoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendSeries {
    pub bucket: TrendBucket,
    pub points: Vec<TrendPoint>,
    pub providers: Vec<ProviderTrend>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GranularityAvailability {
    pub hour: bool,
    pub day: bool,
    pub week: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupStat {
    /// The harness, company, or model this row folds.
    pub name: String,
    /// The billing company for harness rows; empty otherwise.
    pub company: String,
    pub requests: u64,
    pub fresh_input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total_tokens: u64,
    pub cost_micros: i64,
    pub unpriced_requests: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRow {
    pub time: f64,
    pub harness: String,
    pub company: String,
    pub model: String,
    pub tier: Option<String>,
    pub fresh_input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total_tokens: u64,
    pub cost_micros: Option<i64>,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipGroup {
    pub company: String,
    pub harnesses: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatsView {
    pub ledger_available: bool,
    pub privacy_suppressed: bool,
    pub scanned_at: f64,
    pub range_start: f64,
    pub range_end: f64,
    pub summary: UsageSummary,
    pub trend: TrendSeries,
    pub granularity: GranularityAvailability,
    pub harnesses: Vec<GroupStat>,
    pub providers: Vec<GroupStat>,
    pub models: Vec<GroupStat>,
    pub requests: Vec<RequestRow>,
    pub total_requests: u64,
    pub available_models: Vec<String>,
    pub chip_groups: Vec<ChipGroup>,
}

impl UsageStatsView {
    /// Privacy mode is on: the page must say "not looked at", never zeroes.
    pub fn suppressed(now: f64) -> Self {
        Self {
            ledger_available: false,
            privacy_suppressed: true,
            scanned_at: now,
            range_start: now,
            range_end: now,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Accumulator {
    requests: u64,
    fresh_input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
    cost_micros: i64,
    priced: u64,
}

impl Accumulator {
    fn add(&mut self, event: &UsageEvent, cost: Option<i64>) {
        self.requests += 1;
        self.fresh_input = self.fresh_input.saturating_add(event.input);
        self.output = self.output.saturating_add(event.output);
        self.cache_read = self.cache_read.saturating_add(event.cache_read);
        self.cache_creation = self
            .cache_creation
            .saturating_add(event.cache_creation_5m)
            .saturating_add(event.cache_creation_1h);
        if let Some(cost) = cost {
            self.cost_micros = self.cost_micros.saturating_add(cost);
            self.priced += 1;
        }
    }

    fn total_tokens(&self) -> u64 {
        self.fresh_input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_creation)
    }

    fn point(&self, bucket_start: f64) -> TrendPoint {
        TrendPoint {
            bucket_start,
            requests: self.requests,
            fresh_input: self.fresh_input,
            output: self.output,
            cache_read: self.cache_read,
            cache_creation: self.cache_creation,
            total_tokens: self.total_tokens(),
            cost_micros: self.cost_micros,
        }
    }

    fn group(&self, name: &str, company: &str) -> GroupStat {
        GroupStat {
            name: name.to_string(),
            company: company.to_string(),
            requests: self.requests,
            fresh_input: self.fresh_input,
            output: self.output,
            cache_read: self.cache_read,
            cache_creation: self.cache_creation,
            total_tokens: self.total_tokens(),
            cost_micros: self.cost_micros,
            unpriced_requests: self.requests - self.priced,
        }
    }

    fn summary(&self) -> UsageSummary {
        let input_side = self
            .fresh_input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_creation);
        UsageSummary {
            requests: self.requests,
            fresh_input: self.fresh_input,
            output: self.output,
            cache_read: self.cache_read,
            cache_creation: self.cache_creation,
            total_tokens: self.total_tokens(),
            cost_micros: (self.priced > 0).then_some(self.cost_micros),
            unpriced_requests: self.requests - self.priced,
            cache_hit_rate: if input_side == 0 {
                0.0
            } else {
                self.cache_read as f64 / input_side as f64
            },
        }
    }
}

fn harness_label(tool: ToolType) -> &'static str {
    tool.hierarchy().tool
}

fn company_label(tool: ToolType) -> &'static str {
    tool.hierarchy().vendor
}

/// Resolve the requested granularity against the interactive budget,
/// coarsening until the range fits.
pub fn resolve_bucket(requested: Option<TrendBucket>, duration_seconds: f64) -> TrendBucket {
    let mut bucket = requested.unwrap_or_else(|| TrendBucket::recommended(duration_seconds));
    while !bucket.fits(duration_seconds) {
        match bucket.coarser() {
            Some(coarser) => bucket = coarser,
            None => break,
        }
    }
    bucket
}

fn bucket_starts(bucket: TrendBucket, range_start: f64, range_end: f64) -> Vec<f64> {
    let mut starts = Vec::new();
    let Some(mut start) = bucket.floor(range_start) else {
        return starts;
    };
    while start < range_end && starts.len() <= MAX_TREND_BUCKETS {
        starts.push(start);
        match bucket.next(start) {
            Some(next) if next > start => start = next,
            _ => break,
        }
    }
    starts
}

/// Fold `events` for the page. `now` closes an open-ended range; the
/// earliest event opens one.
pub(crate) fn query(events: &[UsageEvent], query: &UsageStatsQuery, now: f64, scanned_at: f64) -> UsageStatsView {
    let range_end = query.range_end.unwrap_or(now);
    let earliest = events
        .iter()
        .map(|event| event.date)
        .fold(None, |min: Option<f64>, date| Some(min.map_or(date, |m| m.min(date))));
    let mut range_start = query
        .range_start
        .or(earliest)
        .unwrap_or(range_end - FALLBACK_RANGE_SECONDS);
    if range_start >= range_end {
        range_start = range_end - 3_600.0;
    }
    let duration = range_end - range_start;

    let harness_filter: Option<HashSet<&str>> = query
        .harnesses
        .as_ref()
        .map(|list| list.iter().map(String::as_str).collect());
    let model_filter: Option<HashSet<&str>> = query
        .models
        .as_ref()
        .map(|list| list.iter().map(String::as_str).collect());

    let latest_accepted = (now + FUTURE_TOLERANCE_SECONDS).min(range_end);
    let in_range = |event: &UsageEvent| event.date >= range_start && event.date < latest_accepted;
    let in_harness = |event: &UsageEvent| {
        harness_filter
            .as_ref()
            .is_none_or(|set| set.contains(harness_label(event.tool)))
    };

    let mut available_models: Vec<String> = events
        .iter()
        .filter(|event| in_range(event) && in_harness(event))
        .map(|event| event.model.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    available_models.sort_unstable_by_key(|model| model.to_lowercase());

    let bucket = resolve_bucket(query.granularity, duration);
    let starts = bucket_starts(bucket, range_start, range_end);
    let mut totals: BTreeMap<i64, Accumulator> = BTreeMap::new();
    let mut per_provider: BTreeMap<ToolType, BTreeMap<i64, Accumulator>> = BTreeMap::new();
    let mut summary = Accumulator::default();
    let mut harnesses: BTreeMap<ToolType, Accumulator> = BTreeMap::new();
    let mut companies: HashMap<&'static str, Accumulator> = HashMap::new();
    let mut models: HashMap<&str, Accumulator> = HashMap::new();
    let mut rows: Vec<(f64, &UsageEvent, Option<i64>)> = Vec::new();

    for event in events.iter().filter(|event| {
        in_range(event)
            && in_harness(event)
            && model_filter
                .as_ref()
                .is_none_or(|set| set.contains(event.model.as_str()))
    }) {
        let cost = priced_cost_micros(event);
        summary.add(event, cost);
        harnesses.entry(event.tool).or_default().add(event, cost);
        companies
            .entry(company_label(event.tool))
            .or_default()
            .add(event, cost);
        models.entry(event.model.as_str()).or_default().add(event, cost);
        if let Some(start) = bucket.floor(event.date) {
            let key = start as i64;
            totals.entry(key).or_default().add(event, cost);
            per_provider
                .entry(event.tool)
                .or_default()
                .entry(key)
                .or_default()
                .add(event, cost);
        }
        rows.push((event.date, event, cost));
    }

    let points: Vec<TrendPoint> = starts
        .iter()
        .map(|&start| {
            totals
                .get(&(start as i64))
                .copied()
                .unwrap_or_default()
                .point(start)
        })
        .collect();
    let providers_trend: Vec<ProviderTrend> = per_provider
        .iter()
        .map(|(tool, buckets)| ProviderTrend {
            harness: harness_label(*tool).to_string(),
            company: company_label(*tool).to_string(),
            points: starts
                .iter()
                .map(|&start| {
                    buckets
                        .get(&(start as i64))
                        .copied()
                        .unwrap_or_default()
                        .point(start)
                })
                .collect(),
        })
        .collect();

    let mut harness_stats: Vec<GroupStat> = harnesses
        .iter()
        .map(|(tool, acc)| acc.group(harness_label(*tool), company_label(*tool)))
        .collect();
    let mut provider_stats: Vec<GroupStat> = companies
        .iter()
        .map(|(company, acc)| acc.group(company, ""))
        .collect();
    let mut model_stats: Vec<GroupStat> = models
        .iter()
        .map(|(model, acc)| acc.group(model, ""))
        .collect();
    let by_tokens = |a: &GroupStat, b: &GroupStat| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| b.requests.cmp(&a.requests))
            .then_with(|| a.name.cmp(&b.name))
    };
    harness_stats.sort_by(by_tokens);
    provider_stats.sort_by(by_tokens);
    model_stats.sort_by(by_tokens);

    let total_requests = rows.len() as u64;
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let limit = query
        .request_limit
        .unwrap_or(DEFAULT_REQUEST_LIMIT)
        .clamp(1, MAX_REQUEST_LIMIT);
    let requests: Vec<RequestRow> = rows
        .iter()
        .take(limit)
        .map(|(_, event, cost)| RequestRow {
            time: event.date,
            harness: harness_label(event.tool).to_string(),
            company: company_label(event.tool).to_string(),
            model: event.model.clone(),
            tier: event.service_tier.clone(),
            fresh_input: event.input,
            output: event.output,
            cache_read: event.cache_read,
            cache_creation: event.cache_creation_5m.saturating_add(event.cache_creation_1h),
            total_tokens: event.tokens(),
            cost_micros: *cost,
            session_id: event.session_id.clone(),
        })
        .collect();

    UsageStatsView {
        ledger_available: true,
        privacy_suppressed: false,
        scanned_at,
        range_start,
        range_end,
        summary: summary.summary(),
        trend: TrendSeries {
            bucket,
            points,
            providers: providers_trend,
        },
        granularity: GranularityAvailability {
            hour: TrendBucket::Hour.fits(duration),
            day: TrendBucket::Day.fits(duration),
            week: TrendBucket::Week.fits(duration),
        },
        harnesses: harness_stats,
        providers: provider_stats,
        models: model_stats,
        requests,
        total_requests,
        available_models,
        chip_groups: chip_groups(events),
    }
}

/// Every harness that has ever recorded a request, grouped under its
/// billing company in `ToolType` order — the filter bar's chip rows.
pub(crate) fn chip_groups(events: &[UsageEvent]) -> Vec<ChipGroup> {
    let mut seen: HashSet<ToolType> = HashSet::new();
    let mut tools: Vec<ToolType> = events
        .iter()
        .map(|event| event.tool)
        .filter(|tool| seen.insert(*tool))
        .collect();
    tools.sort();
    let mut groups: Vec<ChipGroup> = Vec::new();
    for tool in tools {
        let company = company_label(tool);
        let harness = harness_label(tool).to_string();
        match groups.iter_mut().find(|group| group.company == company) {
            Some(group) => group.harnesses.push(harness),
            None => groups.push(ChipGroup {
                company: company.to_string(),
                harnesses: vec![harness],
            }),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn event(tool: ToolType, date: f64, model: &str, input: u64, cache_read: u64, output: u64) -> UsageEvent {
        UsageEvent {
            tool,
            date,
            model: model.to_string(),
            input,
            cache_read,
            output,
            cache_creation_5m: 0,
            cache_creation_1h: 0,
            service_tier: None,
            session_id: Some("session-1".to_string()),
            message_id: None,
            request_id: None,
            is_sidechain: false,
            is_parent_path: false,
            source_key: Arc::from("fixture"),
        }
    }

    #[test]
    fn recommended_bucket_follows_the_native_thresholds() {
        assert_eq!(TrendBucket::recommended(3_600.0), TrendBucket::Hour);
        assert_eq!(TrendBucket::recommended(86_400.0), TrendBucket::Hour);
        assert_eq!(TrendBucket::recommended(86_401.0), TrendBucket::Day);
        assert_eq!(TrendBucket::recommended(45.0 * 86_400.0), TrendBucket::Day);
        assert_eq!(TrendBucket::recommended(46.0 * 86_400.0), TrendBucket::Week);
    }

    #[test]
    fn resolve_coarsens_past_the_interactive_budget() {
        let sixty_days = 60.0 * 86_400.0;
        assert_eq!(resolve_bucket(Some(TrendBucket::Hour), sixty_days), TrendBucket::Day);
        assert_eq!(resolve_bucket(Some(TrendBucket::Hour), 2.0 * 86_400.0), TrendBucket::Hour);
        let ten_years = 3_650.0 * 86_400.0;
        assert_eq!(resolve_bucket(Some(TrendBucket::Day), ten_years), TrendBucket::Week);
        assert_eq!(resolve_bucket(None, ten_years), TrendBucket::Week);
    }

    #[test]
    fn buckets_are_local_calendar_aligned_and_contiguous() {
        let now = 1_756_800_000.0;
        for bucket in [TrendBucket::Hour, TrendBucket::Day, TrendBucket::Week] {
            let start = bucket.floor(now).unwrap();
            assert!(start <= now);
            let next = bucket.next(start).unwrap();
            assert!(next > now, "{bucket:?}: next bucket must start after now");
            assert_eq!(bucket.floor(start + 1.0).unwrap(), start);
            assert_eq!(bucket.floor(next).unwrap(), next);
        }
        let week_start = TrendBucket::Week.floor(now).unwrap();
        assert_eq!(local_time(week_start).unwrap().weekday(), chrono::Weekday::Mon);
        assert_eq!(local_time(week_start).unwrap().time(), NaiveTime::MIN);
    }

    #[test]
    fn summary_and_groups_fold_the_filtered_events() {
        let now = 1_756_800_000.0;
        let events = vec![
            event(ToolType::Codex, now - 100.0, "gpt-5", 1_000, 500, 200),
            event(ToolType::Claude, now - 200.0, "claude-opus-4-1", 2_000, 1_000, 400),
            event(ToolType::Claude, now - 300.0, "claude-opus-4-1", 10, 0, 10),
            event(ToolType::Gemini, now - 40.0 * 86_400.0, "gemini-2.5-pro", 5, 5, 5),
        ];
        let view = query(
            &events,
            &UsageStatsQuery {
                range_start: Some(now - 86_400.0),
                range_end: Some(now),
                ..Default::default()
            },
            now,
            now,
        );
        assert!(view.ledger_available);
        assert_eq!(view.summary.requests, 3);
        assert_eq!(view.summary.fresh_input, 3_010);
        assert_eq!(view.summary.cache_read, 1_500);
        assert_eq!(view.summary.output, 610);
        assert_eq!(view.summary.total_tokens, 5_120);
        assert!((view.summary.cache_hit_rate - 1_500.0 / 4_510.0).abs() < 1e-9);
        assert_eq!(view.trend.bucket, TrendBucket::Hour);
        assert_eq!(view.harnesses[0].name, "Claude Code");
        assert_eq!(view.harnesses[0].company, "Anthropic");
        assert_eq!(view.harnesses[0].requests, 2);
        assert_eq!(view.providers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Anthropic", "OpenAI"]);
        assert_eq!(view.models[0].name, "claude-opus-4-1");
        assert_eq!(view.available_models, ["claude-opus-4-1", "gpt-5"]);
        assert_eq!(view.total_requests, 3);
        assert_eq!(view.requests[0].model, "gpt-5", "newest first");
        assert_eq!(
            view.chip_groups,
            vec![
                ChipGroup { company: "OpenAI".into(), harnesses: vec!["Codex".into()] },
                ChipGroup { company: "Anthropic".into(), harnesses: vec!["Claude Code".into()] },
                ChipGroup { company: "Google AI".into(), harnesses: vec!["Gemini Web".into()] },
            ],
            "chip groups cover every ingested harness, not just the range"
        );
        let point_total: u64 = view.trend.points.iter().map(|p| p.total_tokens).sum();
        assert_eq!(point_total, view.summary.total_tokens, "trend points sum to the summary");
        let provider_total: u64 = view
            .trend
            .providers
            .iter()
            .flat_map(|p| p.points.iter().map(|point| point.total_tokens))
            .sum();
        assert_eq!(provider_total, view.summary.total_tokens);
        assert!(view.trend.points.len() >= 24 && view.trend.points.len() <= 26);
        assert!(view.granularity.hour && view.granularity.day && view.granularity.week);
    }

    #[test]
    fn harness_and_model_filters_narrow_and_cascade() {
        let now = 1_756_800_000.0;
        let events = vec![
            event(ToolType::Codex, now - 100.0, "gpt-5", 100, 0, 0),
            event(ToolType::Claude, now - 200.0, "claude-opus-4-1", 200, 0, 0),
        ];
        let only_claude = query(
            &events,
            &UsageStatsQuery { harnesses: Some(vec!["Claude Code".into()]), ..Default::default() },
            now,
            now,
        );
        assert_eq!(only_claude.summary.requests, 1);
        assert_eq!(only_claude.available_models, ["claude-opus-4-1"], "models cascade from the harness pick");
        let nothing = query(
            &events,
            &UsageStatsQuery { harnesses: Some(vec![]), ..Default::default() },
            now,
            now,
        );
        assert_eq!(nothing.summary.requests, 0);
        assert!(nothing.available_models.is_empty());
        let by_model = query(
            &events,
            &UsageStatsQuery { models: Some(vec!["gpt-5".into()]), ..Default::default() },
            now,
            now,
        );
        assert_eq!(by_model.summary.requests, 1);
        assert_eq!(by_model.available_models.len(), 2, "the model menu keeps every model in range");
    }

    #[test]
    fn open_range_starts_at_the_earliest_event_and_caps_requests() {
        let now = 1_756_800_000.0;
        let events: Vec<UsageEvent> = (0..10)
            .map(|i| event(ToolType::Codex, now - 3_600.0 * f64::from(i) - 1.0, "gpt-5", 1, 0, 0))
            .collect();
        let view = query(&events, &UsageStatsQuery { request_limit: Some(3), ..Default::default() }, now, now);
        assert_eq!(view.range_start, now - 3_600.0 * 9.0 - 1.0);
        assert_eq!(view.range_end, now);
        assert_eq!(view.total_requests, 10);
        assert_eq!(view.requests.len(), 3);
        assert_eq!(view.requests[0].time, now - 1.0);
        assert!(view.summary.cost_micros.is_some(), "gpt-5 is priced");
    }

    #[test]
    fn empty_ledger_yields_a_fallback_window_and_no_buckets_beyond_it() {
        let now = 1_756_800_000.0;
        let view = query(&[], &UsageStatsQuery::default(), now, 0.0);
        assert_eq!(view.range_end - view.range_start, FALLBACK_RANGE_SECONDS);
        assert_eq!(view.summary, UsageSummary::default());
        assert!(view.summary.cost_micros.is_none());
        assert_eq!(view.trend.bucket, TrendBucket::Day);
        assert!(view.trend.points.len() >= 30 && view.trend.points.len() <= 32);
        assert!(view.chip_groups.is_empty());
    }

    #[test]
    fn future_dated_events_are_refused_even_inside_a_future_range() {
        let now = 1_756_800_000.0;
        let events = vec![
            event(ToolType::Codex, now - 10.0, "gpt-5", 100, 0, 0),
            event(ToolType::Codex, now + 3_600.0, "gpt-5", 100, 0, 0),
        ];
        let view = query(
            &events,
            &UsageStatsQuery { range_start: Some(now - 86_400.0), range_end: Some(now + 86_400.0), ..Default::default() },
            now,
            now,
        );
        assert_eq!(view.summary.requests, 1, "an event an hour in the future is skew, not usage");
        assert_eq!(view.total_requests, 1);
    }

    #[test]
    fn hourly_buckets_stay_contiguous_across_a_year() {
        // Every hour of a year, including any DST transitions in the local zone.
        let start = TrendBucket::Hour.floor(1_735_689_600.0).unwrap();
        let mut at = start;
        for _ in 0..(366 * 24) {
            let next = TrendBucket::Hour.next(at).unwrap();
            assert_eq!(next - at, 3_600.0);
            assert_eq!(TrendBucket::Hour.floor(at + 1.0).unwrap(), at, "floor is idempotent inside the bucket");
            at = next;
        }
        let mut day = TrendBucket::Day.floor(1_735_689_600.0).unwrap();
        for _ in 0..366 {
            let next = TrendBucket::Day.next(day).unwrap();
            assert!(next > day && next - day <= 25.0 * 3_600.0 && next - day >= 23.0 * 3_600.0);
            day = next;
        }
    }

    #[test]
    fn suppressed_view_is_explicit_about_not_looking() {
        let view = UsageStatsView::suppressed(5.0);
        assert!(!view.ledger_available);
        assert!(view.privacy_suppressed);
        assert_eq!(view.summary.requests, 0);
    }
}
