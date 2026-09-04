/**
 * The data behind docs/screenshots/popover-overview.png, as this client's
 * types: four companies, AntiGravity's model groups, a Codex Spark group, a
 * Claude Fable group, a month of cost, four status feeds. Used by the preview
 * page and by tests that hold the Overview against the native screenshot.
 */
import type { AccountQuota, CostView, PresentationSettings, QuotaBucket, QuotaForecast, QuotaView, ServiceStatusView } from "../api";

export const FIXTURE_NOW = Date.UTC(2026, 7, 24, 17, 30, 0) / 1000; // 2026-08-24 17:30Z

const H = 3_600, D = 86_400;

function forecast(verdict: QuotaForecast["verdict"], used: number, runOutIn?: number): QuotaForecast {
  const projected = verdict === "atRisk" ? Math.min(100, used + 30) : verdict === "surplus" ? Math.max(0, used - 5) : used + 8;
  return {
    verdict, confidence: verdict === "learning" ? "learning" : "medium", confidenceScore: 0.62,
    currentUsedPercent: used, plannedUsedPercent: used + 10, projectedUsedPercent: projected,
    projectedUsedLowerPercent: Math.max(0, projected - 9), projectedUsedUpperPercent: Math.min(100, projected + 9),
    targetRemainingPercent: 10, runOutAt: runOutIn ? FIXTURE_NOW + runOutIn : undefined,
  };
}

function bucket(id: string, title: string, used: number, resetIn: number, window: number, f?: QuotaForecast, groupTitle?: string): QuotaBucket {
  return { id, title, shortLabel: title, usedPercent: used, resetAt: FIXTURE_NOW + resetIn, rawWindowSeconds: window, groupTitle, forecast: f };
}

const stale = FIXTURE_NOW - 19 * 60;

export const FIXTURE_VIEW: QuotaView = {
  accounts: [
    {
      accountId: "codex-demo", tool: "codex", plan: "ChatGPT Pro", queriedAt: stale, origin: "sharedCache",
      buckets: [
        bucket("weekly", "Weekly", 9, 6 * D + 18 * H, 7 * D, forecast("atRisk", 9, 3 * D + 2 * H)),
        bucket("spark_five_hour", "5 Hours", 0, 4 * H + 40 * 60, 5 * H, forecast("surplus", 0), "GPT-5.3 Codex Spark"),
        bucket("spark_weekly", "Weekly", 61, 6 * D + 18 * H + 4 * 60, 7 * D, forecast("atRisk", 61, 5 * H + 39 * 60), "GPT-5.3 Codex Spark"),
      ],
    },
    {
      accountId: "claude-demo", tool: "claude", plan: "Claude Max", queriedAt: stale, origin: "sharedCache",
      buckets: [
        bucket("five_hour", "5 Hours", 9, 59 * 60, 5 * H, forecast("surplus", 9)),
        bucket("weekly", "Weekly", 55, 3 * D + 10 * H, 7 * D, forecast("atRisk", 55, D + 12 * H)),
        bucket("weekly_fable", "Weekly", 33, 3 * D + 10 * H, 7 * D, forecast("atRisk", 33, 3 * D + H), "Fable"),
      ],
    },
    {
      accountId: "gemini-demo", tool: "gemini", plan: "Google AI Ultra", queriedAt: stale, origin: "sharedCache",
      buckets: [
        bucket("five_hour", "5 Hours", 0, H + 35 * 60, 5 * H, forecast("surplus", 0)),
        bucket("weekly", "Weekly", 14, D + 9 * H, 7 * D, forecast("surplus", 14)),
      ],
    },
    {
      accountId: "antigravity-demo", tool: "antigravity", plan: "Google AI Ultra", queriedAt: stale, origin: "sharedCache",
      buckets: [
        bucket("gemini_five_hour", "5 Hours", 0, 4 * H + 25 * 60, 5 * H, forecast("surplus", 0), "Gemini models"),
        bucket("gemini_weekly", "Weekly", 8, D + 8 * H, 7 * D, forecast("surplus", 8), "Gemini models"),
        bucket("claude_five_hour", "5 Hours", 3, H + 46 * 60, 5 * H, forecast("surplus", 3), "Claude and GPT models"),
        bucket("claude_weekly", "Weekly", 1, 6 * D + 21 * H, 7 * D, forecast("surplus", 1), "Claude and GPT models"),
      ],
    },
    {
      accountId: "grok-demo", tool: "grok", plan: "SuperGrok Heavy", queriedAt: stale, origin: "sharedCache",
      buckets: [bucket("weekly", "Weekly", 4, 3 * D + 9 * H, 7 * D, forecast("surplus", 4))],
    },
  ] as AccountQuota[],
  lastUpdated: FIXTURE_NOW - 18 * 60,
  hasSharedData: true,
  isDemo: true,
};

export const FIXTURE_SETTINGS = {
  displayMode: "remaining", refreshIntervalSeconds: 300, popoverDensity: "regular",
  coreProviderOrder: ["codex", "claude", "gemini", "grok"], providerPlanLabels: {},
  miniDisplayMode: "regular", miniStripDensity: "roomy", updateChannel: "dev",
} as unknown as PresentationSettings;

function totals(cost: number, tokens: number, requests: number) {
  return { pricedCostMicros: Math.round(cost * 1_000_000), tokens, requests };
}

const daily: CostView["daily"] = [];
for (let i = 0; i < 30; i += 1) {
  const day = new Date((FIXTURE_NOW - i * D) * 1000);
  const key = `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, "0")}-${String(day.getDate()).padStart(2, "0")}`;
  const cost = i === 0 ? 2292 : i === 1 ? 694 : i === 7 ? 2979 : 700 + ((i * 37) % 900);
  const tokens = i === 0 ? 2_160_000_000 : i === 1 ? 755_370_000 : i === 7 ? 3_190_000_000 : 600_000_000 + ((i * 91) % 500) * 1_000_000;
  daily.push({ ...totals(cost, tokens, 1200 + i * 7), day: key });
}

export const FIXTURE_COST: CostView = {
  today: totals(2292, 2_160_000_000, 1240),
  allTime: totals(91_775, 46_090_000_000, 88_000),
  last7Days: totals(12_033, 12_790_000_000, 9_000),
  last30Days: totals(28_948, 28_530_000_000, 31_000),
  daily,
  models: [
    { ...totals(9_800, 9_530_000_000, 4_000), harness: "Codex", model: "gpt-5.3-codex", unpricedEvents: 0 },
    { ...totals(9_100, 9_500_000_000, 3_900), harness: "Claude Code", model: "claude-opus-5", unpricedEvents: 0 },
    { ...totals(4_200, 4_760_000_000, 1_800), harness: "Gemini CLI", model: "gemini-3-pro", unpricedEvents: 0 },
    { ...totals(4_100, 4_750_000_000, 1_700), harness: "AntiGravity", model: "gemini-3-pro", unpricedEvents: 0 },
  ],
  providers: [],
  unpricedEvents: 0, scannedFiles: 412, malformedLines: 0, truncated: false,
  scannedAt: FIXTURE_NOW - 18 * 60, pricingVersion: "demo", privacySuppressed: false,
};

export const FIXTURE_STATUS: ServiceStatusView = {
  providers: [
    { tool: "codex", indicator: "none", description: "All Systems Operational", incidents: [] },
    { tool: "claude", indicator: "none", description: "All Systems Operational", incidents: [] },
    { tool: "gemini", indicator: "none", description: "All services operational", incidents: [] },
    { tool: "grok", indicator: "none", description: "All services operational", incidents: [] },
  ],
  updatedAt: FIXTURE_NOW - 60,
} as unknown as ServiceStatusView;
