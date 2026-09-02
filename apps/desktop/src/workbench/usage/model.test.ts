import { describe, expect, it } from "vitest";
import {
  clampWindow,
  compactTokens,
  compactUSD,
  countSummary,
  formatMicroUSD,
  formatPercent,
  modelSummary,
  periodTitle,
  populatedPeriods,
  presetRange,
  toggleCompany,
  toggleHarness,
} from "./model";

describe("number formatting matches the native page", () => {
  it("compacts tokens with the native widths", () => {
    expect(compactTokens(999)).toBe("999");
    expect(compactTokens(1_234)).toBe("1.2k");
    expect(compactTokens(3_400_000)).toBe("3.40M");
    expect(compactTokens(19_380_000_000)).toBe("19.38B");
    expect(compactTokens(-1_500)).toBe("-1.5k");
  });

  it("compacts dollars one width class per magnitude", () => {
    expect(compactUSD(0)).toBe("$0.00");
    expect(compactUSD(4_000)).toBe("<$0.01");
    expect(compactUSD(4_800_000)).toBe("$4.80");
    expect(compactUSD(48_000_000)).toBe("$48.0");
    expect(compactUSD(480_000_000)).toBe("$480");
    expect(compactUSD(4_800_000_000)).toBe("$4.8k");
    expect(compactUSD(480_000_000_000)).toBe("$480k");
    expect(compactUSD(4_800_000_000_000)).toBe("$4.8M");
  });

  it("never rounds a real charge down to $0.00", () => {
    expect(formatMicroUSD(0)).toBe("$0.00");
    expect(formatMicroUSD(4_000)).toBe("<$0.01");
    expect(formatMicroUSD(-4_000)).toBe("-<$0.01");
    expect(formatMicroUSD(12_345_678)).toBe("$12.35");
    expect(formatMicroUSD(-2_500_000)).toBe("-$2.50");
  });

  it("formats ratios as percentages with a dash for nothing", () => {
    expect(formatPercent(0.4237)).toBe("42.4%");
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(Number.NaN)).toBe("—");
  });
});

describe("range presets", () => {
  const now = 1_756_800_000;

  it("leaves All open on both ends", () => {
    expect(presetRange("all", now)).toEqual({});
  });

  it("starts Today at local midnight", () => {
    const range = presetRange("today", now);
    const start = new Date(range.rangeStart! * 1000);
    expect(start.getHours()).toBe(0);
    expect(start.getMinutes()).toBe(0);
    expect(range.rangeEnd).toBe(now);
    expect(now - range.rangeStart!).toBeLessThanOrEqual(86_400);
  });

  it("counts back whole days for the rolling presets", () => {
    expect(presetRange("day1", now)).toEqual({ rangeStart: now - 86_400, rangeEnd: now });
    expect(presetRange("day30", now)).toEqual({ rangeStart: now - 30 * 86_400, rangeEnd: now });
  });

  it("orders a custom range even when the user typed it backwards", () => {
    expect(presetRange("custom", now, { start: now, end: now - 100 })).toEqual({
      rangeStart: now - 100,
      rangeEnd: now,
    });
  });
});

describe("harness selection", () => {
  const all = ["Codex", "Claude Code", "Gemini Web"];

  it("collapses back to every harness when the last one is re-added", () => {
    const without = toggleHarness(null, all, "Codex");
    expect(without).toEqual(["Claude Code", "Gemini Web"]);
    expect(toggleHarness(without, all, "Codex")).toBeNull();
  });

  it("can select nothing, which the core treats as no harness", () => {
    let selected: string[] | null = null;
    for (const harness of all) selected = toggleHarness(selected, all, harness);
    expect(selected).toEqual([]);
  });

  it("flips a company group as one switch", () => {
    const google = ["Gemini Web", "AntiGravity"];
    const every = ["Codex", ...google];
    expect(toggleCompany(null, every, google)).toEqual(["Codex"]);
    expect(toggleCompany(["Codex", "Gemini Web"], every, google)).toBeNull();
  });
});

describe("tables", () => {
  it("labels periods by bucket", () => {
    const monday = new Date(2026, 7, 24, 17, 0, 0).getTime() / 1000;
    expect(periodTitle("hour", monday)).toBe("Mon Aug 24 17:00");
    expect(periodTitle("day", monday)).toBe("Monday Aug 24");
    expect(periodTitle("week", monday)).toBe("Week of Aug 24");
  });

  it("keeps only periods that recorded something", () => {
    const point = (requests: number, totalTokens: number) => ({
      bucketStart: 0,
      requests,
      freshInput: 0,
      output: 0,
      cacheRead: 0,
      cacheCreation: 0,
      totalTokens,
      costMicros: 0,
    });
    expect(populatedPeriods([point(0, 0), point(1, 0), point(0, 5)])).toHaveLength(2);
  });

  it("summarises counts the way the native footer does", () => {
    const base = { periods: 3, bucket: "day" as const, loadedRequests: 200, totalRequests: 1_250, providers: 1, models: 2 };
    expect(countSummary("periods", base)).toBe("3 active days");
    expect(countSummary("periods", { ...base, periods: 1 })).toBe("1 active day");
    expect(countSummary("requests", base)).toBe("200 of 1,250 requests");
    expect(countSummary("requests", { ...base, loadedRequests: 1_250 })).toBe("1,250 requests");
    expect(countSummary("providers", base)).toBe("1 provider");
    expect(countSummary("models", base)).toBe("2 models");
  });

  it("summarises the model pick", () => {
    expect(modelSummary(null, ["a", "b"])).toBe("All");
    expect(modelSummary([], ["a", "b"])).toBe("None");
    expect(modelSummary(["a"], ["a", "b"])).toBe("a");
    expect(modelSummary(["a", "b"], ["a", "b", "c"])).toBe("2 of 3");
  });
});

describe("chart window", () => {
  const domain = { start: 0, end: 10 * 86_400 };

  it("enforces the two-bucket floor and stays inside the domain", () => {
    expect(clampWindow({ start: 5 * 86_400, end: 5 * 86_400 + 10 }, domain, "day")).toEqual({
      start: 5 * 86_400,
      end: 7 * 86_400,
    });
    expect(clampWindow({ start: 9 * 86_400, end: 12 * 86_400 }, domain, "day")).toEqual({
      start: 7 * 86_400,
      end: 10 * 86_400,
    });
  });

  it("never exceeds the domain when the window is wider than it", () => {
    expect(clampWindow({ start: -5, end: 20 * 86_400 }, domain, "hour")).toEqual(domain);
  });
});
