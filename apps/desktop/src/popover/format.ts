/**
 * The strings the native popover prints, reproduced word for word.
 *
 * Sources: `ResetCountdownFormatter`, `QuotaFreshnessLabel`,
 * `SubscriptionWindowProgress`, `QuotaForecastRow`, `OverviewCostSummaryCard`.
 * A label that reads "resets in 6d 18h" on one client and "6d 18h left" on
 * the other is two products; every format here is checked against the
 * native wording by test.
 */
import type { QuotaForecast } from "../api";

/** `ResetCountdownFormatter.string(from:now:)` — "6d 18h", "1d", "4h 40m", "59m", "<1m", "now". */
export function countdown(resetAt: number | undefined, now: number): string | null {
  if (resetAt === undefined || resetAt === null) return null;
  const total = Math.round(resetAt - now);
  if (total <= 0) return "now";
  const days = Math.floor(total / 86_400);
  const hours = Math.floor((total % 86_400) / 3_600);
  const minutes = Math.floor((total % 3_600) / 60);
  if (days >= 2) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (days === 1) return hours > 0 ? `1d ${hours}h` : "1d";
  if (hours >= 1) return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  if (minutes >= 1) return `${minutes}m`;
  return "<1m";
}

const SHORT_MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `ResetCountdownFormatter.absoluteTime` — "19:05" today, "Aug 31, 12:00" this year, "Aug 31, 2027, 12:00" otherwise. */
export function absoluteTime(at: number, now: number): string {
  const d = new Date(at * 1000);
  const n = new Date(now * 1000);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const time = `${hh}:${mm}`;
  const sameDay = d.getFullYear() === n.getFullYear() && d.getMonth() === n.getMonth() && d.getDate() === n.getDate();
  if (sameDay) return time;
  const month = SHORT_MONTHS[d.getMonth()];
  if (d.getFullYear() === n.getFullYear()) return `${month} ${d.getDate()}, ${time}`;
  return `${month} ${d.getDate()}, ${d.getFullYear()}, ${time}`;
}

/** Native's post-reset grace: a row keeps "resets in" for this long past the moment. */
export const POST_RESET_GRACE_SECONDS = 120;

export interface ResetStatus {
  isExpired: boolean;
  label: string;
}

/** `ResetCountdownFormatter.resetStatus` — "resets in 6d 18h · Aug 31, 12:00" or "reset passed · Aug 31, 12:00". */
export function resetStatus(resetAt: number | undefined, now: number): ResetStatus | null {
  if (resetAt === undefined || resetAt === null) return null;
  const absolute = absoluteTime(resetAt, now);
  if (now - resetAt > POST_RESET_GRACE_SECONDS) {
    return { isExpired: true, label: `reset passed · ${absolute}` };
  }
  const cd = countdown(resetAt, now);
  if (cd === null) return null;
  return { isExpired: false, label: `resets in ${cd} · ${absolute}` };
}

/** `ResetCountdownFormatter.updatedAgo` — "Updated just now" … "Updated 18 minutes ago". */
export function updatedAgo(at: number | undefined, now: number): string {
  if (at === undefined || at === null) return "Never updated";
  const interval = Math.floor(now - at);
  if (interval < 5) return "Updated just now";
  if (interval < 60) return `Updated ${interval} seconds ago`;
  const minutes = Math.floor(interval / 60);
  if (minutes < 60) return minutes === 1 ? "Updated 1 minute ago" : `Updated ${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? "Updated 1 hour ago" : `Updated ${hours} hours ago`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "Updated 1 day ago" : `Updated ${days} days ago`;
}

/** `QuotaFreshnessLabel.compactAge` — "45s", "2m", "8h", "3d". */
export function compactAge(seconds: number): string {
  const s = Math.round(Math.max(0, seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}

/**
 * `QuotaFreshnessLabel.describe` — the orange line under a SubProvider row.
 * Null when the reading is fresh and nothing failed. `staleAfter` is native's
 * `max(300, refreshIntervalSeconds * 2)`.
 */
export function staleLabel(
  lastSuccessAt: number | undefined,
  lastAttemptAt: number | undefined,
  errorMessage: string | undefined,
  staleAfter: number,
  now: number,
): string | null {
  if (lastSuccessAt === undefined && lastAttemptAt === undefined) return null;
  const successAge = lastSuccessAt === undefined ? undefined : Math.max(0, now - lastSuccessAt);
  const isStale = successAge === undefined ? true : successAge >= Math.max(0, staleAfter);
  const failure = errorMessage?.trim() || undefined;
  if (!isStale && !failure) return null;
  const dataPhrase = successAge === undefined ? "no cached data" : `data ${compactAge(successAge)} old`;
  if (failure) {
    const attemptAge = lastAttemptAt === undefined ? undefined : Math.max(0, now - lastAttemptAt);
    const attemptPhrase = attemptAge !== undefined && attemptAge >= 5
      ? `Refresh failed ${compactAge(attemptAge)} ago`
      : "Refresh failed";
    return `${attemptPhrase} · ${dataPhrase} · ${shortened(failure)}`;
  }
  if (successAge === undefined) return "Stale · never updated";
  return `Stale · updated ${compactAge(successAge)} ago`;
}

function shortened(text: string, max = 80): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

export function staleAfterSeconds(refreshIntervalSeconds: number): number {
  return Math.max(300, refreshIntervalSeconds * 2);
}

/** `OverviewCostSummaryCard.formatCost` — "$0.00", "$12.34", "$2979". */
export function formatCost(value: number | undefined | null): string {
  if (value === undefined || value === null || !Number.isFinite(value)) return "—";
  if (value < 0.01) return "$0.00";
  if (value < 100) return `$${value.toFixed(2)}`;
  return `$${Math.round(value)}`;
}

/** `OverviewCostSummaryCard.formatTokens` — "2.16B", "755.37M", "12.5k", "42". */
export function formatTokens(tokens: number | undefined | null): string {
  if (tokens === undefined || tokens === null || !Number.isFinite(tokens)) return "—";
  if (tokens < 1_000) return `${Math.round(tokens)}`;
  if (tokens < 1_000_000) return `${(tokens / 1_000).toFixed(1)}k`;
  if (tokens < 1_000_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
  return `${(tokens / 1_000_000_000).toFixed(2)}B`;
}

export type DisplayMode = "remaining" | "used";

/** `QuotaForecastRow.primaryText` — "At risk · likely to run out before reset". */
export function forecastPrimaryText(forecast: QuotaForecast, mode: DisplayMode): string {
  const left = Math.round(100 - forecast.projectedUsedPercent);
  const used = Math.round(forecast.projectedUsedPercent);
  const value = mode === "remaining" ? `${left}% left` : `${used}% used`;
  switch (forecast.verdict) {
    case "enough": return `Enough · forecast ${value} at reset`;
    case "surplus": return `Surplus · forecast ${value} at reset`;
    case "watch": return `Watch · forecast ${value} at reset`;
    case "atRisk": return "At risk · likely to run out before reset";
    default: return `Learning · about ${value} at reset`;
  }
}

/** `QuotaForecastRow.useUpText` — "Estimated to run out in 3d 2h" / "Projected to last until reset". */
export function forecastUseUpText(forecast: QuotaForecast, now: number): string {
  if (forecast.runOutAt !== undefined && forecast.runOutAt !== null) {
    const cd = countdown(forecast.runOutAt, now);
    if (cd !== null) {
      return forecast.verdict === "watch" ? `Could run out in ${cd}` : `Estimated to run out in ${cd}`;
    }
  }
  switch (forecast.verdict) {
    case "watch": return "Use-up time uncertain · may run short before reset";
    case "atRisk": return "Expected to run out before reset";
    default: return "Projected to last until reset";
  }
}

/** `QuotaPaceForecast.confidenceLabel`. */
export function forecastConfidenceLabel(forecast: QuotaForecast): string {
  switch (forecast.confidence) {
    case "high": return "High confidence";
    case "medium": return "Medium confidence";
    default: return "Learning";
  }
}

/** `QuotaForecastPalette` — the verdict colour, also in the token contract. */
export function verdictColor(verdict: QuotaForecast["verdict"]): string | null {
  switch (verdict) {
    case "enough": return "#33B37A";
    case "surplus": return "#338FE0";
    case "watch": return "#F59E33";
    case "atRisk": return "#F25252";
    default: return null;
  }
}

export type PaceStage = "onTrack" | "slightlyAhead" | "ahead" | "farAhead" | "slightlyBehind" | "behind" | "farBehind";

export interface UsagePace {
  stage: PaceStage;
  /** actual − expected: positive means burning faster than linear. */
  deltaPercent: number;
  expectedUsedPercent: number;
  willLastToReset: boolean;
  etaSeconds: number | null;
}

/**
 * `UsagePace.compute`: where an evenly spent window would be, against where
 * this one is. Null outside a window, or for a fresh window with backfilled
 * usage, as native.
 */
export function usagePace(bucket: { usedPercent: number; resetAt?: number; rawWindowSeconds?: number }, now: number): UsagePace | null {
  if (bucket.resetAt === undefined || !bucket.rawWindowSeconds || bucket.rawWindowSeconds <= 0) return null;
  const duration = bucket.rawWindowSeconds;
  const timeUntilReset = bucket.resetAt - now;
  if (timeUntilReset > duration) return null;
  const elapsed = Math.max(0, Math.min(duration, duration - timeUntilReset));
  const expected = Math.max(0, Math.min(100, (elapsed / duration) * 100));
  const actual = Math.max(0, Math.min(100, bucket.usedPercent));
  if (elapsed === 0 && actual > 0) return null;
  const delta = actual - expected;
  const abs = Math.abs(delta);
  const stage: PaceStage = abs <= 2 ? "onTrack"
    : abs <= 6 ? (delta >= 0 ? "slightlyAhead" : "slightlyBehind")
    : abs <= 12 ? (delta >= 0 ? "ahead" : "behind")
    : delta >= 0 ? "farAhead" : "farBehind";
  let willLastToReset = true;
  let etaSeconds: number | null = null;
  if (elapsed > 0 && actual > 0) {
    const rate = actual / elapsed;
    if (rate > 0) {
      const candidate = (100 - actual) / rate;
      if (candidate < timeUntilReset) {
        willLastToReset = false;
        etaSeconds = candidate;
      }
    }
  }
  return { stage, deltaPercent: delta, expectedUsedPercent: expected, willLastToReset, etaSeconds };
}

/** `UsagePace.stageSummary` — "On pace", "5% in deficit", "8% in reserve". */
export function paceStageSummary(pace: UsagePace): string {
  const value = Math.round(Math.abs(pace.deltaPercent));
  switch (pace.stage) {
    case "onTrack": return "On pace";
    case "slightlyAhead": case "ahead": case "farAhead": return `${value}% in deficit`;
    default: return `${value}% in reserve`;
  }
}

/** `UsagePaceRow.etaText` — "Lasts until reset" / "Runs out in 1h 30m". */
export function paceEtaText(pace: UsagePace, now: number): string | null {
  if (pace.willLastToReset) return "Lasts until reset";
  if (pace.etaSeconds === null || pace.etaSeconds <= 0) return null;
  const cd = countdown(now + pace.etaSeconds, now);
  return cd === null ? null : `Runs out in ${cd}`;
}

/** `UsagePaceRow.color`: deficit warms with distance; reserve stays quiet. */
export function paceColor(pace: UsagePace): string {
  switch (pace.stage) {
    case "onTrack": return "#33B37A";
    case "slightlyAhead": return "#F5B033";
    case "ahead": return "#F78C33";
    case "farAhead": return "#F25252";
    default: return "var(--pv-text-secondary)";
  }
}

const PLAN_WORDS: Record<string, string> = {
  pro: "Pro", plus: "Plus", max: "Max", free: "Free", ultra: "Ultra", team: "Team", enterprise: "Enterprise",
  business: "Business", starter: "Starter", quota: "Quota", heavy: "Heavy", lite: "Lite", go: "Go", edu: "Edu",
};

/** `ProviderPlanDisplay.codexDisplayName`: split on `_`/`-`/space, case each word. */
function planWords(raw: string | undefined | null): string | null {
  const trimmed = raw?.trim();
  if (!trimmed) return null;
  const parts = trimmed.split(/[_\-\s]+/).filter(Boolean);
  if (parts.length === 0) return trimmed;
  return parts.map((w) => PLAN_WORDS[w.toLowerCase()] ?? (w.length <= 3 && w === w.toUpperCase() ? w : w[0].toUpperCase() + w.slice(1))).join(" ");
}

function prefixed(plan: string | null, brand: string): string | null {
  if (!plan) return null;
  return plan.toLowerCase().startsWith(brand.toLowerCase()) ? plan : `${brand} ${plan}`;
}

/**
 * `AppSettings.planBadgeLabel`: the user's override wins, then the brand
 * prefix native adds for the core providers — "ChatGPT Pro", "Claude Max",
 * "Google AI Free" — with Grok's product names spelled as xAI spells them.
 */
export function planBadgeLabel(tool: string, rawPlan: string | undefined | null, overrides?: Record<string, string> | null): string | null {
  const override = overrides?.[tool]?.trim();
  if (override) return override;
  switch (tool) {
    case "codex": return prefixed(planWords(rawPlan), "ChatGPT");
    case "claude": return prefixed(planWords(rawPlan), "Claude");
    case "gemini": case "antigravity": return prefixed(planWords(rawPlan), "Google AI");
    case "grok": {
      const display = planWords(rawPlan);
      if (!display) return null;
      switch (display.replace(/\s+/g, "").toLowerCase()) {
        case "supergrokheavy": return "SuperGrok Heavy";
        case "supergrok": return "SuperGrok";
        case "supergroklite": return "SuperGrok Lite";
        default: return display;
      }
    }
    default: return planWords(rawPlan);
  }
}
