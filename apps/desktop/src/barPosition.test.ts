import { describe, expect, it } from "vitest";

import type { QuotaForecast } from "./api";
import { barPosition, forecastMarks } from "./api";

function forecast(overrides: Partial<QuotaForecast>): QuotaForecast {
  return {
    plannedUsedPercent: 50,
    projectedUsedPercent: 60,
    projectedUsedLowerPercent: 52,
    projectedUsedUpperPercent: 71,
    ...overrides,
  } as QuotaForecast;
}

describe("where a number sits on the bar", () => {
  it("mirrors when the bar shows what is left", () => {
    expect(barPosition(30, true)).toBe(30);
    expect(barPosition(30, false)).toBe(70);
  });

  it("keeps a mark on the bar when the forecast runs past the end", () => {
    // Projected usage may exceed 100 — the shortfall's size is not capped —
    // but a mark cannot be drawn off the end of the track.
    expect(barPosition(180, true)).toBe(100);
    expect(barPosition(180, false)).toBe(0);
  });
});

describe("the marks a forecast puts on the bar", () => {
  it("returns the interval in drawing order whichever way the bar runs", () => {
    const used = forecastMarks(forecast({}), true);
    const left = forecastMarks(forecast({}), false);
    expect(used.low).toBeLessThan(used.high);
    expect(left.low).toBeLessThan(left.high);
  });

  it("mirrors the whole set together", () => {
    const used = forecastMarks(forecast({}), true);
    const left = forecastMarks(forecast({}), false);
    expect(left.median).toBeCloseTo(100 - used.median, 10);
    expect(left.low).toBeCloseTo(100 - used.high, 10);
    expect(left.high).toBeCloseTo(100 - used.low, 10);
  });

  /// A marker outside its own interval is a claim the forecast never made.
  it("holds the median inside its interval, even when clipping moves one end", () => {
    const marks = forecastMarks(
      forecast({
        projectedUsedPercent: 180,
        projectedUsedLowerPercent: 90,
        projectedUsedUpperPercent: 260,
      }),
      true,
    );
    expect(marks.median).toBeGreaterThanOrEqual(marks.low);
    expect(marks.median).toBeLessThanOrEqual(marks.high);
  });

  it("holds it inside in the mirrored direction too", () => {
    const marks = forecastMarks(
      forecast({
        projectedUsedPercent: 180,
        projectedUsedLowerPercent: 90,
        projectedUsedUpperPercent: 260,
      }),
      false,
    );
    expect(marks.median).toBeGreaterThanOrEqual(marks.low);
    expect(marks.median).toBeLessThanOrEqual(marks.high);
  });
});
