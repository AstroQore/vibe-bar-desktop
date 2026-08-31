import { QUOTA_BAR } from "./tokens";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Kept in sync with `vibebar_desktop_core::model`. */
export type ForecastVerdict =
  | "enough"
  | "surplus"
  | "watch"
  | "atRisk"
  | "learning";
export type ForecastConfidence = "learning" | "medium" | "high";

export interface QuotaForecast {
  verdict: ForecastVerdict;
  confidence: ForecastConfidence;
  confidenceScore: number;
  currentUsedPercent: number;
  plannedUsedPercent: number;
  /** May exceed 100: the quota is capped, the shortage is not. */
  projectedUsedPercent: number;
  projectedUsedLowerPercent: number;
  projectedUsedUpperPercent: number;
  targetRemainingPercent: number;
  /** Unix seconds, when usage is projected to reach the cap. */
  runOutAt?: number;
}

export interface QuotaBucket {
  id: string;
  title: string;
  shortLabel: string;
  usedPercent: number;
  resetAt?: number;
  rawWindowSeconds?: number;
  groupTitle?: string;
  /** Absent when there is not enough history yet — shown as such, never as
   *  a confident verdict. */
  forecast?: QuotaForecast;
}

/** Verdict wording, taken from the native app so one product does not
 *  describe the same state two different ways. */
export function forecastHeadline(f: QuotaForecast): string {
  switch (f.verdict) {
    case "atRisk":
      return "At risk · likely to run out before reset";
    case "watch":
      return "Watch · could run out before reset";
    case "surplus":
      return `Surplus · forecast ${Math.round(
        Math.max(0, 100 - f.projectedUsedPercent),
      )}% left at reset`;
    case "enough":
      return `Enough · forecast ${Math.round(
        Math.max(0, 100 - f.projectedUsedPercent),
      )}% left at reset`;
    case "learning":
      return "Learning · not enough history yet";
  }
}

/** The line under the headline: when it runs out, or that it lasts. */
export function forecastDetail(f: QuotaForecast, now: number): string | null {
  if (f.runOutAt != null) {
    const seconds = f.runOutAt - now;
    if (seconds <= 0) return "Estimated to be out now";
    return `Estimated to run out in ${formatDuration(seconds)}`;
  }
  if (f.verdict === "learning") return null;
  return "Projected to last until reset";
}

/** Severity class for the verdict line, matching the quota bar's palette. */
export function forecastSeverity(verdict: ForecastVerdict): string {
  switch (verdict) {
    case "atRisk":
      return "critical";
    case "watch":
      return "warning";
    case "learning":
      return "muted";
    default:
      return "ok";
  }
}

export type QuotaOrigin = "live" | "desktopCache" | "sharedCache" | "mixed";

export interface QuotaErrorPayload {
  kind: string;
  detail?: string;
}

export interface AccountQuota {
  accountId: string;
  tool: string;
  buckets: QuotaBucket[];
  plan?: string;
  /** Unix epoch seconds. */
  queriedAt: number;
  origin: QuotaOrigin;
  error?: QuotaErrorPayload;
}

export interface QuotaView {
  accounts: AccountQuota[];
  lastUpdated?: number;
  hasSharedData: boolean;
  isDemo: boolean;
}

export type SessionSource = "indexed" | "scanned";

export interface SessionRow {
  rowId?: number;
  provider: string;
  harness: string;
  sessionId: string;
  providerVariant?: string;
  title?: string;
  projectDir?: string;
  lastActiveAt?: number;
  /** Opaque backend-issued reference; never a filesystem path. */
  sessionRef: string;
  messageCount?: number;
  resumeCommand?: string;
  excerpt?: string;
}

export interface SessionListing {
  source: SessionSource;
  rows: SessionRow[];
  indexedTotal?: number;
  indexNote?: string;
}

export interface TranscriptMessage {
  role: "user" | "assistant" | "system" | "tool" | "other";
  text: string;
  timestamp?: string;
}

export interface TranscriptCursor {
  byteOffset: number;
  messageOffset: number;
  skipToNewline?: boolean;
}

export interface TranscriptPage {
  messages: TranscriptMessage[];
  /** Omitted when a safety limit truncates a very large transcript scan. */
  totalMessages?: number;
  offset: number;
  truncated: boolean;
  nextCursor?: TranscriptCursor;
}

export interface NativeAppPresence {
  installed: boolean;
  running: boolean;
  bundleId: string;
}

export interface AppInfo {
  version: string;
  dataRoot: string;
  isDemo: boolean;
  nativeApp: NativeAppPresence;
}

export interface SkillInventoryRow {
  name: string;
  directory: string;
  description?: string;
  targets: string[];
  health: string;
  source: string;
}
export interface SkillsInventoryView {
  skills: SkillInventoryRow[];
  warnings: string[];
  scannedAt: number;
}

/** Read-only presentation preferences from the shared native settings file. */
export interface PresentationSettings {
  displayMode: string;
  refreshIntervalSeconds: number;
  menuBarColorBasis: string;
  selectedFieldIds: string[];
  customLabels: Record<string, string>;
  visibleCoreProviders?: string[];
  coreProviderOrder: string[];
  visibleMiscProviders?: string[];
  providerPlanLabels: Record<string, string>;
}

export interface StatusIncident {
  id: string;
  name: string;
  status: string;
  impact: string;
  createdAt?: number;
  updatedAt?: number;
}

export interface ProviderStatus {
  tool: string;
  indicator: string;
  description: string;
  updatedAt?: number;
  incidents: StatusIncident[];
}

export interface ServiceStatusView {
  providers: ProviderStatus[];
  updatedAt?: number;
}

export interface CostTotals {
  pricedCostMicros: number;
  tokens: number;
  requests: number;
}

export interface DailyCost extends CostTotals {
  day: string;
}

export interface ModelCost extends CostTotals {
  harness: string;
  model: string;
  unpricedEvents: number;
}

export interface CostView {
  today: CostTotals;
  last7Days: CostTotals;
  last30Days: CostTotals;
  allTime: CostTotals;
  daily: DailyCost[];
  models: ModelCost[];
  unpricedEvents: number;
  scannedFiles: number;
  malformedLines: number;
  truncated: boolean;
  scannedAt: number;
  pricingVersion: string;
}

export const QUOTA_EVENT = "vibebar://quota-updated";
export const MINI_SHOWN_EVENT = "vibebar://mini-shown";

export const api = {
  quotaView: () => invoke<QuotaView>("quota_view"),
  refreshQuota: () => invoke<QuotaView>("refresh_quota"),
  hideMini: () => invoke<void>("hide_mini"),
  appInfo: () => invoke<AppInfo>("app_info"),
  skillsInventory: () => invoke<SkillsInventoryView>("skills_inventory"),
  presentationSettings: () => invoke<PresentationSettings>("presentation_settings"),
  statusSnapshot: () => invoke<ServiceStatusView>("status_snapshot"),
  refreshStatus: () => invoke<ServiceStatusView>("refresh_status"),
  costView: () => invoke<CostView>("cost_view"),
  refreshCost: () => invoke<CostView>("refresh_cost"),
  sessionList: (limit = 100) => invoke<SessionListing>("session_list", { limit }),
  sessionSearch: (query: string, limit = 50) =>
    invoke<SessionListing>("session_search", { query, limit }),
  sessionTranscript: (
    sessionRef: string,
    offset = 0,
    limit = 50,
    cursor?: TranscriptCursor,
  ) =>
    invoke<TranscriptPage>("session_transcript", {
      sessionRef,
      offset,
      limit,
      cursor,
    }),
  onQuotaUpdated: (handler: (view: QuotaView) => void) =>
    listen<QuotaView>(QUOTA_EVENT, (event) => handler(event.payload)),
  onMiniShown: (handler: () => void) => listen<void>(MINI_SHOWN_EVENT, handler),
};

/** L1 company → L2 SubProvider naming, mirrored from the core crate so the
 *  UI groups providers exactly the way the native app does. */

/**
 * The quota bar's fill colour, from the shared token contract.
 *
 * The two display modes have different thresholds *and* different palettes —
 * `used` is teal at rest where `remaining` is green — so a bar coloured by the
 * remaining rule while showing a used percentage is the wrong colour twice.
 */
export function quotaBarColor(percent: number, showsUsed: boolean): string {
  if (showsUsed) {
    const { criticalAtOrAbove, warningAtOrAbove, critical, warning, ok } =
      QUOTA_BAR.used;
    if (percent >= criticalAtOrAbove) return critical;
    if (percent >= warningAtOrAbove) return warning;
    return ok;
  }
  const { criticalBelow, warningBelow, critical, warning, ok } =
    QUOTA_BAR.remaining;
  if (percent < criticalBelow) return critical;
  if (percent < warningBelow) return warning;
  return ok;
}

/**
 * Where a used-percentage sits along the bar, in whichever direction it is
 * drawn.
 *
 * Every mark on the bar goes through this one transform. The forecast supplies
 * everything as *used*, and a surface showing what is left is the same numbers
 * mirrored — computing that per mark is how a median ends up outside its own
 * confidence band.
 */
export function barPosition(usedPercent: number, showsUsed: boolean): number {
  const used = Number.isFinite(usedPercent) ? usedPercent : 0;
  return Math.min(100, Math.max(0, showsUsed ? used : 100 - used));
}

/**
 * The three marks a forecast puts on the bar, already in bar coordinates.
 *
 * `low` and `high` come back in drawing order however the bar is mirrored, and
 * the median is held inside them: a marker outside its own interval is a claim
 * the forecast never made.
 */
export function forecastMarks(
  forecast: QuotaForecast,
  showsUsed: boolean,
): { pace: number; median: number; low: number; high: number } {
  const median = barPosition(forecast.projectedUsedPercent, showsUsed);
  const first = barPosition(forecast.projectedUsedLowerPercent, showsUsed);
  const second = barPosition(forecast.projectedUsedUpperPercent, showsUsed);
  const low = Math.min(first, second);
  const high = Math.max(first, second);
  return {
    pace: barPosition(forecast.plannedUsedPercent, showsUsed),
    median: Math.min(high, Math.max(low, median)),
    low,
    high,
  };
}

export function formatRelative(unixSeconds?: number): string {
  if (!unixSeconds) return "never";
  const deltaSeconds = Math.round(Date.now() / 1000 - unixSeconds);
  if (deltaSeconds < 60) return "just now";
  if (deltaSeconds < 3600) return `${Math.floor(deltaSeconds / 60)}m ago`;
  if (deltaSeconds < 86400) return `${Math.floor(deltaSeconds / 3600)}h ago`;
  return `${Math.floor(deltaSeconds / 86400)}d ago`;
}

/** `3d 2h`, `5h 39m`, `12m` — the same shape the reset countdown uses, and
 *  the same the native app prints beside a run-out estimate. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (hours >= 24) {
    const days = Math.floor(hours / 24);
    return `${days}d ${hours % 24}h`;
  }
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

/**
 * How long until a moment, with no lead-in.
 *
 * Separate from `formatCountdown` because the mini window puts this under a
 * dial where "resets in" would not fit, and reads it again after "out" for a
 * run-out time — where "resets in" would be the wrong words entirely. Empty
 * once the moment has passed, so a caller can fall back rather than print a
 * countdown to something already over.
 */
export function formatRemaining(until?: number, now = Date.now() / 1000): string {
  if (!until) return "";
  const remaining = until - now;
  return remaining > 0 ? formatDuration(remaining) : "";
}

export function formatCountdown(resetAt?: number): string {
  if (!resetAt) return "";
  const spelled = formatRemaining(resetAt);
  return spelled ? `resets in ${spelled}` : "resetting";
}

const API_KEY_ENV: Record<string, string> = {
  zai: "Z_AI_API_KEY",
  minimax: "MINIMAX_CODING_API_KEY or MINIMAX_API_KEY",
  openRouter: "OPENROUTER_API_KEY",
  warp: "WARP_API_KEY or WARP_TOKEN",
};

export function describeError(error: QuotaErrorPayload, tool?: string): string {
  const apiKey = tool ? API_KEY_ENV[tool] : undefined;
  switch (error.kind) {
    case "noCredential":
      if (apiKey) return `Set ${apiKey} and refresh.`;
      return "Not signed in — run the provider's CLI login.";
    case "needsLogin":
      if (apiKey) return `The configured ${apiKey} was rejected. Update it and refresh.`;
      return "Credential rejected — sign in again with the provider's CLI.";
    case "rateLimited":
      return "Rate limited by the provider. Try again shortly.";
    case "network":
      return `Network error${error.detail ? `: ${error.detail}` : ""}`;
    case "timedOut":
      return "The provider did not respond in time.";
    case "notImplemented":
      return "No adapter in this build yet.";
    case "parseFailure":
      return `Unexpected response${error.detail ? `: ${error.detail}` : ""}`;
    default:
      return error.detail ?? "Unknown error";
  }
}
