import { describe, expect, it } from "vitest";
import type { QuotaBucket, QuotaForecast, QuotaView } from "../../api";
import {
  calendarEntries,
  entriesByDay,
  laneEvents,
  miniForecastLine,
  monthGrid,
  monthStartOf,
  riskBadge,
  riskRows,
  subDailyEvents,
  subProviderCycles,
} from "./model";

const NOW = 1_756_800_000;

function bucket(id: string, title: string, used: number, resetIn: number, windowSeconds: number, extra: Partial<QuotaBucket> = {}): QuotaBucket {
  return { id, title, shortLabel: title, usedPercent: used, resetAt: NOW + resetIn, rawWindowSeconds: windowSeconds, ...extra };
}

function forecast(verdict: QuotaForecast["verdict"], projectedUsedPercent: number, runOutAt?: number): QuotaForecast {
  return {
    verdict,
    confidence: "medium",
    confidenceScore: 0.6,
    currentUsedPercent: 50,
    plannedUsedPercent: 60,
    projectedUsedPercent,
    projectedUsedLowerPercent: projectedUsedPercent - 5,
    projectedUsedUpperPercent: projectedUsedPercent + 5,
    targetRemainingPercent: 0,
    runOutAt,
  } as QuotaForecast;
}

const view: QuotaView = {
  accounts: [
    {
      accountId: "oauth-codex",
      tool: "codex",
      plan: "Pro",
      queriedAt: NOW - 60,
      origin: "own" as never,
      buckets: [
        bucket("five_hour", "5 hour", 40, 2 * 3_600, 5 * 3_600),
        bucket("weekly", "Weekly", 90, 3 * 86_400, 7 * 86_400, { forecast: forecast("atRisk", 104, NOW + 86_400) }),
      ],
    },
    {
      accountId: "oauth-claude",
      tool: "claude",
      queriedAt: NOW - 60,
      origin: "own" as never,
      buckets: [
        bucket("five_hour", "5 hour", 10, 3_600, 5 * 3_600),
        bucket("seven_day", "Weekly", 30, 5 * 86_400, 7 * 86_400, { forecast: forecast("enough", 55) }),
        bucket("seven_day_opus", "Opus", 88, 5 * 86_400, 7 * 86_400, { groupTitle: "Opus", forecast: forecast("watch", 97) }),
      ],
    },
  ],
  hasSharedData: true,
  isDemo: true,
};

describe("cycle cards", () => {
  const cycles = subProviderCycles(view, null, NOW);

  it("groups each account's buckets by SubProvider and group title, headlined by the longest window", () => {
    // Accounts come in the settings' visible-provider order, like the popover.
    expect(cycles.map((c) => c.name)).toEqual(["Claude", "Claude · Opus", "ChatGPT Agentic"]);
    const codex = cycles[2];
    expect(codex.headline.id).toBe("weekly");
    expect(codex.forecast?.verdict).toBe("atRisk");
    expect(codex.plan).toBe("Pro");
    expect(cycles[0].buckets.map((b) => b.id)).toEqual(["five_hour", "seven_day"]);
    expect(cycles[1].headline.id).toBe("seven_day_opus");
  });

  it("reads the forecast in a few words", () => {
    expect(miniForecastLine(forecast("atRisk", 104, NOW + 3_600 * 5), NOW)).toMatch(/^out /);
    expect(miniForecastLine(forecast("watch", 97, NOW + 3_600 * 5), NOW)).toMatch(/^may run out /);
    expect(miniForecastLine(forecast("enough", 55), NOW)).toBe("45% left");
    expect(miniForecastLine(forecast("surplus", 30), NOW)).toBe("surplus · 70% left");
    expect(miniForecastLine(forecast("learning", 60), NOW)).toBe("learning · 40% left");
  });
});

describe("run-out risk", () => {
  it("lists uneasy or nearly empty buckets, soonest run-out first", () => {
    const rows = riskRows(subProviderCycles(view, null, NOW));
    expect(rows.map((r) => `${r.cycle.name}/${r.bucket.id}:${r.badge}`)).toEqual(["ChatGPT Agentic/weekly:RISK", "Claude · Opus/seven_day_opus:WATCH"]);
  });

  it("badges by the forecast verdict, not the raw remaining", () => {
    expect(riskBadge(0.5, null)).toBe("OUT");
    expect(riskBadge(10, forecast("atRisk", 100))).toBe("RISK");
    expect(riskBadge(10, forecast("watch", 95))).toBe("WATCH");
    expect(riskBadge(10, null)).toBe("LOW");
    expect(riskBadge(10, forecast("enough", 60))).toBe("LOW");
  });
});

describe("calendar", () => {
  const monthStart = monthStartOf(NOW);
  const monthEnd = monthStartOf(NOW, 1);

  it("keys history by the bucket's reporting account on a merged card", () => {
    const merged = { ...view, accounts: [{ ...view.accounts[0], buckets: [bucket("weekly", "Weekly", 50, 3 * 86_400, 7 * 86_400, { sourceAccountId: "cli-codex" })] }] };
    const history = { "cli-codex:weekly": [{ windowEnd: NOW - 86_400, peakUsedPercent: 60, lastUsedPercent: 60, observationCount: 4, firstSeenAt: NOW - 8 * 86_400, lastSeenAt: NOW - 86_400, resetKind: "scheduled" }] };
    const entries = calendarEntries(merged, null, history, monthStart, monthEnd, NOW);
    const expectedPast = NOW - 86_400 >= monthStart ? 1 : 0;
    expect(entries.filter((e) => e.kind === "past")).toHaveLength(expectedPast);
  });

  it("lists completed cycles in the month and every scheduled reset", () => {
    const history = {
      "oauth-codex:weekly": [
        { windowEnd: NOW - 4 * 86_400, peakUsedPercent: 80, lastUsedPercent: 77, observationCount: 12, firstSeenAt: NOW - 11 * 86_400, lastSeenAt: NOW - 4 * 86_400, resetKind: "scheduled" },
        { windowEnd: NOW - 60 * 86_400, peakUsedPercent: 50, lastUsedPercent: 50, observationCount: 3, firstSeenAt: NOW - 67 * 86_400, lastSeenAt: NOW - 60 * 86_400, resetKind: "scheduled" },
      ],
    };
    const entries = calendarEntries(view, null, history, monthStart, monthEnd, NOW);
    const inMonth = (at: number) => at >= monthStart && at < monthEnd;
    const expectedNext = view.accounts.flatMap((a) => a.buckets).filter((b) => b.resetAt && inMonth(b.resetAt)).length;
    const expectedPast = inMonth(NOW - 4 * 86_400) ? 1 : 0;
    expect(entries.filter((e) => e.kind === "next")).toHaveLength(expectedNext);
    expect(entries.filter((e) => e.kind === "past")).toHaveLength(expectedPast);
    if (expectedPast) expect(entries.find((e) => e.kind === "past")?.gainPercent).toBe(77);
    expect(entries.every((e, i) => i === 0 || entries[i - 1].at <= e.at)).toBe(true);
    const byDay = entriesByDay(entries, monthStart);
    expect([...byDay.values()].flat()).toHaveLength(entries.length);
  });

  it("lays the month out Sunday-first", () => {
    const grid = monthGrid(new Date(2026, 7, 1).getTime() / 1000);
    expect(grid.weekdays[0]).toBe("Sun");
    expect(grid.dayCount).toBe(31);
    expect(grid.leadingBlanks).toBe(6);
  });
});

describe("lanes", () => {
  it("keeps resets inside the horizon and picks the sub-daily ones", () => {
    const week = laneEvents(view, null, NOW, 7);
    expect(week).toHaveLength(5);
    expect(week[0].id).toBe("oauth-claude:five_hour");
    const day = subDailyEvents(laneEvents(view, null, NOW, 1));
    expect(day.map((e) => e.id)).toEqual(["oauth-claude:five_hour", "oauth-codex:five_hour"]);
  });
});
