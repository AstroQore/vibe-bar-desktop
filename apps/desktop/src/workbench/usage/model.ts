/**
 * The Usage Stats page's pure logic: range presets, granularity choices,
 * refresh cadence, and the number formatting the native page uses. The
 * components render what this file decides so the rules stay testable.
 */
import type { TrendBucket, TrendPoint, UsageStatsQuery } from "../../api";

export type RangePreset = "all" | "today" | "day1" | "day7" | "day14" | "day30" | "custom";

export const RANGE_PRESETS: ReadonlyArray<{ id: RangePreset; title: string }> = [
  { id: "all", title: "All" },
  { id: "today", title: "Today" },
  { id: "day1", title: "24 h" },
  { id: "day7", title: "7 d" },
  { id: "day14", title: "14 d" },
  { id: "day30", title: "30 d" },
  { id: "custom", title: "Custom" },
];

export interface CustomRange {
  start: number;
  end: number;
}

/** The query window a preset stands for, in unix seconds. `undefined` bounds
 *  mean "open" — the core closes them with now and the earliest event. */
export function presetRange(
  preset: RangePreset,
  now: number,
  custom?: CustomRange,
): { rangeStart?: number; rangeEnd?: number } {
  switch (preset) {
    case "all":
      return {};
    case "today": {
      const start = new Date(now * 1000);
      start.setHours(0, 0, 0, 0);
      return { rangeStart: start.getTime() / 1000, rangeEnd: now };
    }
    case "day1":
      return { rangeStart: now - 86_400, rangeEnd: now };
    case "day7":
      return { rangeStart: now - 7 * 86_400, rangeEnd: now };
    case "day14":
      return { rangeStart: now - 14 * 86_400, rangeEnd: now };
    case "day30":
      return { rangeStart: now - 30 * 86_400, rangeEnd: now };
    case "custom":
      return custom
        ? { rangeStart: Math.min(custom.start, custom.end), rangeEnd: Math.max(custom.start, custom.end) }
        : { rangeStart: now - 7 * 86_400, rangeEnd: now };
  }
}

export const GRANULARITY_OPTIONS: ReadonlyArray<{ id: TrendBucket | null; title: string }> = [
  { id: null, title: "Auto" },
  { id: "hour", title: "Hourly" },
  { id: "day", title: "Daily" },
  { id: "week", title: "Weekly" },
];

export function granularityTitle(bucket: TrendBucket | null): string {
  return GRANULARITY_OPTIONS.find((option) => option.id === bucket)?.title ?? "Auto";
}

export function bucketDescription(bucket: TrendBucket): string {
  switch (bucket) {
    case "hour":
      return "Hourly buckets";
    case "day":
      return "Local calendar days";
    case "week":
      return "Local calendar weeks";
  }
}

export const REFRESH_INTERVALS = [0, 5, 10, 30, 60] as const;
export type RefreshInterval = (typeof REFRESH_INTERVALS)[number];

export function refreshTitle(interval: RefreshInterval): string {
  return interval === 0 ? "Off" : `${interval}s`;
}

export function refreshMenuTitle(interval: RefreshInterval): string {
  return interval === 0 ? "Off" : `Every ${interval}s`;
}

/** Token kinds keep the native chart colours: system blue/green/orange/purple. */
export const KIND_COLORS = {
  input: "#3478F6",
  output: "#34C759",
  cacheWrite: "#FF9500",
  cacheRead: "#AF52DE",
} as const;

export const KIND_LABELS = {
  input: "Input",
  output: "Output",
  cacheWrite: "Cache write",
  cacheRead: "Cache read",
} as const;

/** The distribution palette, in the native card order. */
export const DISTRIBUTION_PALETTE = [
  "rgb(77, 199, 189)",
  "rgb(140, 102, 235)",
  "rgb(245, 158, 51)",
  "rgb(237, 102, 102)",
  "rgb(87, 158, 245)",
  "rgb(66, 189, 140)",
  "rgb(148, 140, 181)",
] as const;

export function paletteColor(index: number): string {
  return DISTRIBUTION_PALETTE[index % DISTRIBUTION_PALETTE.length];
}

/** `1_234` → `1.2k`, `3_400_000` → `3.40M`, `19_380_000_000` → `19.38B`. */
export function compactTokens(tokens: number): string {
  const sign = tokens < 0 ? "-" : "";
  const magnitude = Math.abs(tokens);
  if (magnitude < 1_000) return `${Math.trunc(tokens)}`;
  if (magnitude < 1_000_000) return `${sign}${(magnitude / 1_000).toFixed(1)}k`;
  if (magnitude < 1_000_000_000) return `${sign}${(magnitude / 1_000_000).toFixed(2)}M`;
  return `${sign}${(magnitude / 1_000_000_000).toFixed(2)}B`;
}

export function formatTokenCount(tokens: number): string {
  return `${compactTokens(tokens)} tok`;
}

/** Grouped digits for table cells: `1234567` → `1,234,567`. */
export function groupedNumber(value: number): string {
  return Math.trunc(value).toLocaleString("en-US");
}

/** `$12.34`, with amounts below half a cent shown as `<$0.01` rather than `$0.00`. */
export function formatMicroUSD(micros: number, precision = 2): string {
  const value = micros / 1_000_000;
  const magnitude = Math.abs(value);
  if (micros !== 0 && magnitude < 10 ** -precision / 2) {
    const smallest = (10 ** -precision).toFixed(precision);
    return `${value < 0 ? "-" : ""}<$${smallest}`;
  }
  return value < 0 ? `-$${magnitude.toFixed(precision)}` : `$${magnitude.toFixed(precision)}`;
}

/** `$4.80`, `$48.0`, `$480`, `$4.8k`, `$480k`, `$4.8M` — one width class per magnitude. */
export function compactUSD(micros: number): string {
  const sign = micros < 0 ? "-" : "";
  const value = Math.abs(micros) / 1_000_000;
  if (micros !== 0 && value < 0.005) return `${sign}<$0.01`;
  if (value < 10) return `${sign}$${value.toFixed(2)}`;
  if (value < 100) return `${sign}$${value.toFixed(1)}`;
  if (value < 1_000) return `${sign}$${value.toFixed(0)}`;
  if (value < 100_000) return `${sign}$${(value / 1_000).toFixed(1)}k`;
  if (value < 1_000_000) return `${sign}$${(value / 1_000).toFixed(0)}k`;
  return `${sign}$${(value / 1_000_000).toFixed(1)}M`;
}

export function formatPercent(ratio: number | null | undefined, precision = 1): string {
  if (ratio == null || !Number.isFinite(ratio)) return "—";
  return `${(ratio * 100).toFixed(precision)}%`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_LONG = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

function pad(value: number): string {
  return value < 10 ? `0${value}` : `${value}`;
}

/** `Aug 24` — the filters bar's range summary parts. */
export function shortDate(at: number): string {
  const date = new Date(at * 1000);
  return `${MONTHS[date.getMonth()]} ${date.getDate()}`;
}

/** `Aug 24 17:30` — hour buckets and chart tooltips. */
export function shortDateTime(at: number): string {
  const date = new Date(at * 1000);
  return `${shortDate(at)} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** `Aug 24 17:30:05` — the requests table's time column. */
export function timestamp(at: number): string {
  const date = new Date(at * 1000);
  return `${shortDateTime(at)}:${pad(date.getSeconds())}`;
}

/** The Periods table's row label per bucket: `Mon Aug 24 17:00`, `Monday Aug 24`, `Week of Aug 24`. */
export function periodTitle(bucket: TrendBucket, start: number): string {
  const date = new Date(start * 1000);
  switch (bucket) {
    case "hour":
      return `${WEEKDAYS[date.getDay()]} ${shortDate(start)} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
    case "day":
      return `${WEEKDAYS_LONG[date.getDay()]} ${shortDate(start)}`;
    case "week":
      return `Week of ${shortDate(start)}`;
  }
}

export function periodUnit(bucket: TrendBucket): string {
  return bucket === "hour" ? "hour" : bucket === "day" ? "day" : "week";
}

/** The x-axis tick label per bucket. */
export function axisLabel(bucket: TrendBucket, start: number): string {
  const date = new Date(start * 1000);
  if (bucket === "hour") return `${pad(date.getHours())}:00`;
  return shortDate(start);
}

/** What the range menu shows next to the preset: `All time` or `Aug 17 – Aug 24`. */
export function rangeSummary(preset: RangePreset, start: number, end: number): string {
  if (preset === "all") return "All time";
  return `${shortDate(start)} – ${shortDate(end)}`;
}

/** The models menu's detail: `All`, one name, or a count. */
export function modelSummary(selected: string[] | null, available: string[]): string {
  if (selected === null) return "All";
  if (selected.length === 0) return "None";
  if (selected.length === 1) return selected[0];
  return `${selected.length} of ${available.length}`;
}

/** Bucketed periods that recorded anything — the Periods table's rows. */
export function populatedPeriods(points: TrendPoint[]): TrendPoint[] {
  return points.filter((point) => point.requests > 0 || point.totalTokens > 0);
}

export function countSummary(
  breakdown: Breakdown,
  view: { periods: number; bucket: TrendBucket; loadedRequests: number; totalRequests: number; providers: number; models: number },
): string {
  switch (breakdown) {
    case "periods":
      return `${view.periods} active ${periodUnit(view.bucket)}${view.periods === 1 ? "" : "s"}`;
    case "requests":
      return view.loadedRequests < view.totalRequests
        ? `${groupedNumber(view.loadedRequests)} of ${groupedNumber(view.totalRequests)} requests`
        : `${groupedNumber(view.totalRequests)} request${view.totalRequests === 1 ? "" : "s"}`;
    case "providers":
      return `${view.providers} provider${view.providers === 1 ? "" : "s"}`;
    case "models":
      return `${view.models} model${view.models === 1 ? "" : "s"}`;
  }
}

export type Breakdown = "periods" | "requests" | "providers" | "models";
export const BREAKDOWNS: ReadonlyArray<{ id: Breakdown; title: string }> = [
  { id: "periods", title: "Periods" },
  { id: "requests", title: "Requests" },
  { id: "providers", title: "Providers" },
  { id: "models", title: "Models" },
];

/** Assemble the core query from the page's filter state. */
export function buildQuery(state: {
  preset: RangePreset;
  custom?: CustomRange;
  harnesses: string[] | null;
  models: string[] | null;
  granularity: TrendBucket | null;
  requestLimit: number;
  now: number;
}): UsageStatsQuery {
  const range = presetRange(state.preset, state.now, state.custom);
  return {
    ...range,
    harnesses: state.harnesses ?? undefined,
    models: state.models ?? undefined,
    granularity: state.granularity ?? undefined,
    requestLimit: state.requestLimit,
  };
}

/** Toggle one harness in the selection; `null` means every harness. */
export function toggleHarness(selected: string[] | null, all: string[], harness: string): string[] | null {
  const current = selected ?? all;
  const next = current.includes(harness) ? current.filter((h) => h !== harness) : [...current, harness];
  const covered = all.every((h) => next.includes(h));
  return covered ? null : all.filter((h) => next.includes(h));
}

/** The company chip flips its whole group on or off. */
export function toggleCompany(selected: string[] | null, all: string[], group: string[]): string[] | null {
  const current = selected ?? all;
  const every = group.every((h) => current.includes(h));
  const next = every ? current.filter((h) => !group.includes(h)) : [...new Set([...current, ...group])];
  const covered = all.every((h) => next.includes(h));
  return covered ? null : all.filter((h) => next.includes(h));
}

/** The chart's visible window inside the loaded points, kept to the native
 *  two-bucket floor and clamped to the domain. */
export interface ChartWindow {
  start: number;
  end: number;
}

export function bucketSeconds(bucket: TrendBucket): number {
  return bucket === "hour" ? 3_600 : bucket === "day" ? 86_400 : 7 * 86_400;
}

export function clampWindow(window: ChartWindow, domain: ChartWindow, bucket: TrendBucket): ChartWindow {
  const minimum = 2 * bucketSeconds(bucket);
  let span = Math.max(minimum, window.end - window.start);
  span = Math.min(span, domain.end - domain.start);
  let start = Math.max(domain.start, Math.min(window.start, domain.end - span));
  const end = Math.min(domain.end, start + span);
  start = Math.max(domain.start, end - span);
  return { start, end };
}

/** The brush's span presets — `6h`, `24h`, `3d`, `7d` — that fit the domain. */
export const WINDOW_SPANS: ReadonlyArray<{ title: string; seconds: number }> = [
  { title: "6h", seconds: 6 * 3_600 },
  { title: "24h", seconds: 24 * 3_600 },
  { title: "3d", seconds: 3 * 86_400 },
  { title: "7d", seconds: 7 * 86_400 },
];
