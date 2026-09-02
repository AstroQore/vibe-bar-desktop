import { describe, expect, it } from "vitest";
import {
  absoluteTime, compactAge, countdown, forecastPrimaryText, forecastUseUpText,
  formatCost, formatTokens, paceEtaText, paceStageSummary, planBadgeLabel, resetStatus, staleLabel, updatedAgo, usagePace,
} from "./format";
import type { QuotaForecast } from "../api";

const NOW = Date.UTC(2026, 7, 24, 12, 0, 0) / 1000; // Aug 24 2026 12:00 UTC

describe("the native countdown wording", () => {
  it("prints days with hours, then hours with minutes, then minutes", () => {
    expect(countdown(NOW + 6 * 86_400 + 18 * 3_600 + 5 * 60, NOW)).toBe("6d 18h");
    expect(countdown(NOW + 2 * 86_400, NOW)).toBe("2d");
    expect(countdown(NOW + 86_400 + 9 * 3_600, NOW)).toBe("1d 9h");
    expect(countdown(NOW + 86_400, NOW)).toBe("1d");
    expect(countdown(NOW + 4 * 3_600 + 40 * 60, NOW)).toBe("4h 40m");
    expect(countdown(NOW + 3_600, NOW)).toBe("1h");
    expect(countdown(NOW + 59 * 60, NOW)).toBe("59m");
    expect(countdown(NOW + 30, NOW)).toBe("<1m");
    expect(countdown(NOW - 1, NOW)).toBe("now");
    expect(countdown(undefined, NOW)).toBeNull();
  });

  it("labels a row the way the popover does", () => {
    const at = NOW + 6 * 86_400 + 18 * 3_600;
    const status = resetStatus(at, NOW)!;
    expect(status.isExpired).toBe(false);
    expect(status.label).toMatch(/^resets in 6d 18h · Aug 3[01], \d\d:\d\d$/);
    const passed = resetStatus(NOW - 600, NOW)!;
    expect(passed.isExpired).toBe(true);
    expect(passed.label).toMatch(/^reset passed · \d\d:\d\d$/);
    // Inside the grace window it still counts down to "now".
    expect(resetStatus(NOW - 60, NOW)!.label).toMatch(/^resets in now/);
  });

  it("drops the date for today and adds the year outside this one", () => {
    expect(absoluteTime(NOW + 3_600, NOW)).toMatch(/^\d\d:\d\d$/);
    expect(absoluteTime(NOW + 2 * 86_400, NOW)).toMatch(/^Aug 26, \d\d:\d\d$/);
    expect(absoluteTime(NOW + 400 * 86_400, NOW)).toMatch(/^Sep \d+, 2027, \d\d:\d\d$/);
  });
});

describe("the native freshness wording", () => {
  it("counts up from just now", () => {
    expect(updatedAgo(NOW - 2, NOW)).toBe("Updated just now");
    expect(updatedAgo(NOW - 30, NOW)).toBe("Updated 30 seconds ago");
    expect(updatedAgo(NOW - 60, NOW)).toBe("Updated 1 minute ago");
    expect(updatedAgo(NOW - 18 * 60, NOW)).toBe("Updated 18 minutes ago");
    expect(updatedAgo(NOW - 3_600, NOW)).toBe("Updated 1 hour ago");
    expect(updatedAgo(NOW - 2 * 86_400, NOW)).toBe("Updated 2 days ago");
    expect(updatedAgo(undefined, NOW)).toBe("Never updated");
  });

  it("is quiet while fresh and orange once stale", () => {
    expect(staleLabel(NOW - 60, NOW - 60, undefined, 600, NOW)).toBeNull();
    expect(staleLabel(NOW - 19 * 60, NOW - 19 * 60, undefined, 600, NOW)).toBe("Stale · updated 19m ago");
    expect(staleLabel(undefined, NOW - 10, undefined, 600, NOW)).toBe("Stale · never updated");
    expect(staleLabel(NOW - 60, NOW - 10, "401 Unauthorized", 600, NOW))
      .toBe("Refresh failed 10s ago · data 1m old · 401 Unauthorized");
    expect(compactAge(45)).toBe("45s");
    expect(compactAge(8 * 3_600)).toBe("8h");
    expect(compactAge(3 * 86_400)).toBe("3d");
  });
});

describe("the native cost wording", () => {
  it("formats money and tokens like the Cost card", () => {
    expect(formatCost(0)).toBe("$0.00");
    expect(formatCost(12.345)).toBe("$12.35");
    expect(formatCost(2979.4)).toBe("$2979");
    expect(formatCost(undefined)).toBe("—");
    expect(formatTokens(42)).toBe("42");
    expect(formatTokens(12_500)).toBe("12.5k");
    expect(formatTokens(755_370_000)).toBe("755.37M");
    expect(formatTokens(46_090_000_000)).toBe("46.09B");
  });
});

describe("the native forecast wording", () => {
  const forecast = (verdict: QuotaForecast["verdict"], projectedUsedPercent = 60, runOutAt?: number): QuotaForecast => ({
    verdict, confidence: "medium", confidenceScore: 0.5, currentUsedPercent: 40, plannedUsedPercent: 50,
    projectedUsedPercent, projectedUsedLowerPercent: projectedUsedPercent - 10,
    projectedUsedUpperPercent: projectedUsedPercent + 10, targetRemainingPercent: 10, runOutAt,
  });

  it("says what the bar cannot", () => {
    expect(forecastPrimaryText(forecast("atRisk"), "remaining")).toBe("At risk · likely to run out before reset");
    expect(forecastPrimaryText(forecast("surplus", 0), "remaining")).toBe("Surplus · forecast 100% left at reset");
    expect(forecastPrimaryText(forecast("enough", 14), "remaining")).toBe("Enough · forecast 86% left at reset");
    expect(forecastPrimaryText(forecast("enough", 14), "used")).toBe("Enough · forecast 14% used at reset");
    expect(forecastPrimaryText(forecast("learning", 50), "remaining")).toBe("Learning · about 50% left at reset");
  });

  it("estimates the run-out, or says it lasts", () => {
    expect(forecastUseUpText(forecast("atRisk", 90, NOW + 3 * 86_400 + 2 * 3_600), NOW)).toBe("Estimated to run out in 3d 2h");
    expect(forecastUseUpText(forecast("watch", 90, NOW + 5 * 3_600 + 39 * 60), NOW)).toBe("Could run out in 5h 39m");
    expect(forecastUseUpText(forecast("surplus"), NOW)).toBe("Projected to last until reset");
    expect(forecastUseUpText(forecast("atRisk"), NOW)).toBe("Expected to run out before reset");
  });
});

describe("the native pace row", () => {
  it("compares the fill with the clock and says so in native's words", () => {
    // Halfway through a 7-day window with 45% used: 5% behind the clock.
    const half = { usedPercent: 45, resetAt: NOW + 3.5 * 86_400, rawWindowSeconds: 7 * 86_400 };
    const pace = usagePace(half, NOW)!;
    expect(paceStageSummary(pace)).toBe("5% in reserve");
    expect(paceEtaText(pace, NOW)).toBe("Lasts until reset");
    // 80% used at the halfway mark: 30% ahead, and it will not last.
    const hot = usagePace({ ...half, usedPercent: 80 }, NOW)!;
    expect(paceStageSummary(hot)).toBe("30% in deficit");
    expect(paceEtaText(hot, NOW)).toMatch(/^Runs out in /);
    // On pace within two points.
    expect(paceStageSummary(usagePace({ ...half, usedPercent: 51 }, NOW)!)).toBe("On pace");
    // A fresh window with backfilled usage is not a pace.
    expect(usagePace({ usedPercent: 20, resetAt: NOW + 7 * 86_400, rawWindowSeconds: 7 * 86_400 }, NOW)).toBeNull();
  });
});

describe("the plan badge", () => {
  it("prefixes the brand the way native does", () => {
    expect(planBadgeLabel("codex", "pro")).toBe("ChatGPT Pro");
    expect(planBadgeLabel("claude", "max")).toBe("Claude Max");
    expect(planBadgeLabel("gemini", "free")).toBe("Google AI Free");
    expect(planBadgeLabel("antigravity", "antigravity_starter_quota")).toBe("Google AI Antigravity Starter Quota");
    expect(planBadgeLabel("grok", "supergrok_heavy")).toBe("SuperGrok Heavy");
    expect(planBadgeLabel("cursor", "ultra")).toBe("Ultra");
    expect(planBadgeLabel("codex", "ChatGPT Pro")).toBe("ChatGPT Pro");
    expect(planBadgeLabel("codex", undefined)).toBeNull();
    expect(planBadgeLabel("codex", "pro", { codex: "Work" })).toBe("Work");
  });
});
