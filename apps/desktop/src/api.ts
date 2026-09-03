import { QUOTA_BAR } from "./tokens";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { FIXTURE_NOW_SECONDS, fixtureInvoke, inTauri } from "./fixtures/invoke";

/** Outside Tauri the fixtures are the data, and their clock is the clock:
 *  relative times and reset countdowns then read as the fixtures intend. */
export function fixtureNow(): number | undefined {
  return inTauri() ? undefined : FIXTURE_NOW_SECONDS;
}

/** `invoke`, or the fixture answers when the page runs outside Tauri. */
function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return inTauri() ? tauriInvoke<T>(command, args) : fixtureInvoke<T>(command, args);
}

/** `listen`, or a no-op outside Tauri: nothing emits there. */
function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return inTauri() ? tauriListen<T>(event, handler) : Promise.resolve(() => undefined);
}

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
  /** The account whose read produced this bucket when a merged card folds
   *  several routes; history is recorded under it. */
  sourceAccountId?: string;
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

/**
 * How much the verdict should be trusted, in the native app's words.
 *
 * A verdict without it overstates itself: the whole point of saying
 * `Learning` rather than a number is that the evidence does not support one
 * yet, and `Medium` says the same thing more quietly. Null where the verdict
 * already carries the answer — `Learning` twice on two lines is not two
 * pieces of information.
 */
export function forecastConfidence(f: QuotaForecast): string | null {
  switch (f.confidence) {
    case "high":
      return "High confidence";
    case "medium":
      return "Medium confidence";
    default:
      return f.verdict === "learning" ? null : "Learning";
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
  /** The model the index recorded for the session, when it knows one. */
  model?: string;
  title?: string;
  /** The index's one-line summary of the session, when it wrote one. */
  summary?: string;
  projectDir?: string;
  lastActiveAt?: number;
  /** Opaque backend-issued reference; never a filesystem path. */
  sessionRef: string;
  messageCount?: number;
  resumeCommand?: string;
  excerpt?: string;
}

export interface SessionDeleteReport {
  sessionRef: string;
  deleted: boolean;
  /** Why it was not deleted, in the deleter's words. */
  reason?: string;
}

export interface HarnessCount {
  harness: string;
  provider: string;
  count: number;
}
export interface SessionListingQuery {
  query?: string;
  /** Raw provider ids (`codex`, `claude`, ...); unset means every provider. */
  providers?: string[];
  harnesses?: string[];
  since?: number;
  offset?: number;
  limit?: number;
}
export interface SessionListing {
  /** Sessions per harness across the store (bounded), independent of the
   *  query's own filters — the Harness menu's counts. */
  harnessCounts: HarnessCount[];
  source: SessionSource;
  rows: SessionRow[];
  indexedTotal?: number;
  indexNote?: string;  /** Where the next page starts in the index; absent when exhausted, scanned, or searched. */
  nextOffset?: number;
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
  /** Whether the setup assistant should open: `show` on a fresh install, `skip` once completed. */
  onboarding: "show" | "skip" | "markCompleted";
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
  /** The mini-window layout. All seven of native's now: "regular",
   *  "compact", "ledger", "tile", "focus", "rail" or "strip". */
  miniDisplayMode: string;
  /** "roomy" | "twoLine" | "narrow" — the strip's density, which native
   *  stores per window rather than per layout. */
  miniStripDensity: string;
  /** "main" | "dev" — which release channel this machine follows. Shared with
   *  the native client, so choosing Dev in either window applies to both. */
  updateChannel: string;
  /** The menu bar item as the shared settings describe it. */
  menuBar: { isVisible: boolean; showTitle: boolean; layout: string };
  /** Misc provider instances the user configured; credentials never ride along. */
  miscProviderInstances: { id: string; tool: string; name: string; isVisible: boolean }[];
  /** "compact" | "regular" | "spacious" — the popover's density, native's
   *  `popoverDensity`. Missing on files written before it was read here. */
  popoverDensity?: string;
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

/** Spend grouped by the company that bills for it — a different axis from
 *  `ModelCost.harness`, which is where the request ran. Two harnesses can bill
 *  one company, so the two are never mixed in a single list. */
export interface ProviderCost {
  company: string;
  pricedCostMicros: number;
  tokens: number;
  requests: number;
  unpricedEvents: number;
}

export interface CostView {
  today: CostTotals;
  last7Days: CostTotals;
  last30Days: CostTotals;
  allTime: CostTotals;
  daily: DailyCost[];
  models: ModelCost[];
  providers: ProviderCost[];
  unpricedEvents: number;
  scannedFiles: number;
  malformedLines: number;
  truncated: boolean;
  scannedAt: number;
  pricingVersion: string;
  /** Privacy mode is on, so nothing was read. Empty windows here mean "not
   *  looked at", not "nothing was spent" — a different statement, and the one
   *  a reader would otherwise take from a row of zeroes. */
  privacySuppressed: boolean;
}

/** Usage Stats: the retained per-request ledger, filtered and folded by the
 *  core. Harness labels are `ToolType` tool names ("Codex", "Claude Code"). */
export type TrendBucket = "hour" | "day" | "week";
export interface UsageStatsQuery {
  rangeStart?: number;
  rangeEnd?: number;
  /** Unset = every harness; empty = none (the All chip is a switch). */
  harnesses?: string[];
  models?: string[];
  granularity?: TrendBucket;
  requestLimit?: number;
}
export interface UsageSummary {
  requests: number;
  freshInput: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  totalTokens: number;
  costMicros: number | null;
  unpricedRequests: number;
  cacheHitRate: number;
}
export interface TrendPoint {
  bucketStart: number;
  requests: number;
  freshInput: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  totalTokens: number;
  costMicros: number;
}
export interface ProviderTrend {
  harness: string;
  company: string;
  points: TrendPoint[];
}
export interface TrendSeries {
  bucket: TrendBucket;
  points: TrendPoint[];
  providers: ProviderTrend[];
}
export interface GroupStat {
  name: string;
  company: string;
  requests: number;
  freshInput: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  totalTokens: number;
  costMicros: number;
  unpricedRequests: number;
}
export interface RequestRow {
  time: number;
  harness: string;
  company: string;
  model: string;
  tier: string | null;
  freshInput: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  totalTokens: number;
  costMicros: number | null;
  sessionId: string | null;
}
export interface ChipGroup {
  company: string;
  harnesses: string[];
}
export interface UsageStatsView {
  ledgerAvailable: boolean;
  privacySuppressed: boolean;
  scannedAt: number;
  rangeStart: number;
  rangeEnd: number;
  summary: UsageSummary;
  trend: TrendSeries;
  granularity: { hour: boolean; day: boolean; week: boolean };
  harnesses: GroupStat[];
  providers: GroupStat[];
  models: GroupStat[];
  requests: RequestRow[];
  totalRequests: number;
  availableModels: string[];
  chipGroups: ChipGroup[];
}
/** One observed quota cycle, as the forecast store keeps it. */
export interface QuotaCycle {
  windowEnd: number;
  windowStart?: number;
  peakUsedPercent: number;
  lastUsedPercent: number;
  observationCount: number;
  firstSeenAt: number;
  lastSeenAt: number;
  completion?: "refillDetected" | "scheduledReset";
  resetKind: string;
  intervalSeconds?: number;
}
export interface ResetHistory {
  completed: QuotaCycle[];
  current: QuotaCycle | null;
}
/** One row of the effective price table, USD per million tokens. */
export interface EffectiveModelPricingRow {
  provider: string;
  company: string;
  subProvider: string;
  model: string;
  displayLabel?: string | null;
  inputPerMillion: number;
  outputPerMillion: number;
  cacheReadPerMillion?: number | null;
  cacheWritePerMillion?: number | null;
  thresholdTokens?: number | null;
  inputAboveThresholdPerMillion?: number | null;
  outputAboveThresholdPerMillion?: number | null;
  cacheReadAboveThresholdPerMillion?: number | null;
  cacheWriteAboveThresholdPerMillion?: number | null;
  fastMultiplier?: number | null;
}
/** The menu bar health watchdog's last report — the native
 *  `MenuBarHealthReport`, for the part that does not need AppKit. */
export interface MenuBarHealthReport {
  state: "checking" | "healthy" | "blocked" | "unavailable";
  message: string;
  checkedAt: number;
  needsFullDiskAccess: boolean;
  alertsEnabled: boolean;
  autoRepairEnabled: boolean;
  repairCommand?: string | null;
}
export const MENU_BAR_HEALTH_EVENT = "vibebar://menu-bar-health";
export const UPDATE_EVENT = "vibebar://update-available";
export const QUOTA_EVENT = "vibebar://quota-updated";
export const MINI_SHOWN_EVENT = "vibebar://mini-shown";
export const SETTINGS_EVENT = "vibebar://settings-changed";

export interface PendingUpdate {
  version: string;
  id: number;
}

export const api = {
  quotaView: () => invoke<QuotaView>("quota_view"),
  refreshQuota: () => invoke<QuotaView>("refresh_quota"),
  hideMini: () => invoke<void>("hide_mini"),
  appInfo: () => invoke<AppInfo>("app_info"),
  skillsInventory: () => invoke<SkillsInventoryView>("skills_inventory"),
  /** Reveal a skill directory in the file manager; only paths inside the
   *  shared skill library are accepted. */
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  menuBarHealth: () => invoke<MenuBarHealthReport>("menu_bar_health"),
  menuBarCheckNow: () => invoke<MenuBarHealthReport>("menu_bar_check_now"),
  menuBarRepair: () => invoke<MenuBarHealthReport>("menu_bar_repair"),
  onMenuBarHealth: (handler: (report: MenuBarHealthReport) => void) =>
    listen<MenuBarHealthReport>(MENU_BAR_HEALTH_EVENT, (event) => handler(event.payload)).then((unlisten) => unlisten),
  autostartEnabled: () => invoke<boolean>("autostart_enabled"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  pricingEffective: () => invoke<EffectiveModelPricingRow[]>("pricing_effective"),
  /** Open a project link; only https links to github.com are accepted. */
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  presentationSettings: () => invoke<PresentationSettings>("presentation_settings"),
  statusSnapshot: () => invoke<ServiceStatusView>("status_snapshot"),
  refreshStatus: () => invoke<ServiceStatusView>("refresh_status"),
  costView: () => invoke<CostView>("cost_view"),
  refreshCost: () => invoke<CostView>("refresh_cost"),
  usageStats: (query: UsageStatsQuery) => invoke<UsageStatsView>("usage_stats", { query }),
  quotaCycles: (accountId: string, bucketId: string) =>
    invoke<ResetHistory>("quota_cycles", { accountId, bucketId }),
  sessionListing: (query: SessionListingQuery) => invoke<SessionListing>("session_listing", { query }),
  openInTerminal: (command: string, terminal: "terminal" | "iterm2") =>
    invoke<void>("open_in_terminal", { command, terminal }),
  sessionList: (limit = 100) => invoke<SessionListing>("session_list", { limit }),
  sessionSearch: (query: string, limit = 50) =>
    invoke<SessionListing>("session_search", { query, limit }),
  /** Delete whole sessions by reference; only after the person confirmed. */
  sessionDelete: (sessionRefs: string[]) => invoke<SessionDeleteReport[]>("session_delete", { sessionRefs }),
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
  /** Save shared settings. Returns them as they read afterwards, which is not
   *  necessarily what was asked for: the file is shared with the native app,
   *  and a value it changed in between wins over a stale idea of it here. */
  /** The writable keys as they sit in the shared file, raw — for editing a
   *  nested object (menu bar item, cost data, mini window) whole. */
  sharedSettingsRaw: () => invoke<Record<string, unknown>>("shared_settings_raw"),
  /** The assistant finished or was skipped; the shared flag both clients honour. */
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  saveSharedSettings: (changes: Record<string, unknown>) =>
    invoke<PresentationSettings>("save_shared_settings", { changes }),
  /** Tell the shell how large the mini window's content is, so it can fit the
   *  window to it. */
  resizeMini: (width: number, height: number) =>
    invoke<void>("resize_mini", { width, height }),
  /** Show or hide the mini window, as the popover's Mini button does. */
  toggleMini: () => invoke<void>("toggle_mini"),
  /** Bring the main window up on a page — the popover's Workbench and
   *  Settings buttons. */
  showMainWindow: (page: string) => invoke<void>("show_main_window", { page }),
  /** The popover lost focus; native's transient popover closes. */
  hidePopover: () => invoke<void>("hide_popover"),
  /** Tell the shell how large the popover's content is. */
  resizePopover: (width: number, height: number) =>
    invoke<void>("resize_popover", { width, height }),
  /** The version waiting on this machine's channel, or null. Checking never
   *  installs — `installUpdate` is the step that does, and only when asked.
   *  The `id` names this answer: two checks can be in flight at once, and
   *  installing has to mean the one that was shown. */
  checkForUpdate: () => invoke<PendingUpdate | null>("check_for_update"),
  /** What the scheduled daily check found and is holding, if anything. */
  pendingUpdate: () => invoke<PendingUpdate | null>("pending_update"),
  /** Every check's result, `null` when it found nothing (or the find was withdrawn). */
  onUpdateAvailable: (handler: (update: PendingUpdate | null) => void) =>
    listen<PendingUpdate | null>(UPDATE_EVENT, (event) => handler(event.payload)).then((unlisten) => unlisten),
  /** Installs what that check found and restarts into it. */
  installUpdate: (id: number) => invoke<void>("install_update", { id }),
  onQuotaUpdated: (handler: (view: QuotaView) => void) =>
    listen<QuotaView>(QUOTA_EVENT, (event) => handler(event.payload)),
  /** The popover asked the main window to open on a page. */
  onNavigate: (handler: (page: string) => void) =>
    listen<string>("navigate", (event) => handler(event.payload)),
  /** The shared settings file changed. The payload names the settings chosen
   *  here that now hold the other client's value, and is null when nothing
   *  was lost — much the commoner case. */
  onSettingsChanged: (handler: (replacedKeys: string[] | null) => void) =>
    listen<string[] | null>(SETTINGS_EVENT, (event) => handler(event.payload)),
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

export function formatRelative(unixSeconds?: number, now?: number): string {
  if (!unixSeconds) return "never";
  const deltaSeconds = Math.round((now ?? Date.now() / 1000) - unixSeconds);
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
