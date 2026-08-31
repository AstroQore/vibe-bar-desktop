import { describe, expect, it } from "vitest";

import type { QuotaForecast } from "./api";
import {
  forecastConfidence,
  forecastDetail,
  forecastHeadline,
  formatCountdown,
  formatDuration,
  quotaBarColor,
} from "./api";
import { QUOTA_BAR } from "./tokens";

function forecast(overrides: Partial<QuotaForecast> = {}): QuotaForecast {
  return {
    verdict: "enough",
    confidence: "medium",
    confidenceScore: 0.7,
    currentUsedPercent: 40,
    plannedUsedPercent: 45,
    projectedUsedPercent: 62,
    projectedUsedLowerPercent: 55,
    projectedUsedUpperPercent: 74,
    targetRemainingPercent: 10,
    completedCycleCount: 5,
    currentObservationCount: 30,
    diagnostics: {
      behavioralProjectionUsedPercent: 60,
      behavioralProgressPercent: 50,
      observationCoveragePercent: 80,
      historyCoveragePercent: 70,
      freshnessPercent: 95,
      recentSampleCount: 12,
      comparableCycleCount: 5,
    },
    ...overrides,
  } as QuotaForecast;
}

describe("what colour a quota bar is", () => {
  /// The two display modes have different thresholds *and* different palettes.
  /// Colouring a used percentage by the remaining rule was wrong twice over,
  /// and it shipped in two surfaces before anyone noticed.
  it("uses each mode's own palette", () => {
    expect(quotaBarColor(95, false)).toBe(QUOTA_BAR.remaining.ok);
    expect(quotaBarColor(95, true)).toBe(QUOTA_BAR.used.critical);
  });

  it("reads a low remaining percentage as critical and a low used one as fine", () => {
    expect(quotaBarColor(5, false)).toBe(QUOTA_BAR.remaining.critical);
    expect(quotaBarColor(5, true)).toBe(QUOTA_BAR.used.ok);
  });

  it("puts the boundaries where the contract does", () => {
    expect(quotaBarColor(QUOTA_BAR.remaining.warningBelow, false)).toBe(
      QUOTA_BAR.remaining.ok,
    );
    expect(quotaBarColor(QUOTA_BAR.remaining.warningBelow - 0.01, false)).toBe(
      QUOTA_BAR.remaining.warning,
    );
    expect(quotaBarColor(QUOTA_BAR.used.criticalAtOrAbove, true)).toBe(
      QUOTA_BAR.used.critical,
    );
  });
});

describe("how long is left", () => {
  it("drops minutes once there are days to report", () => {
    expect(formatDuration(3 * 86_400 + 2 * 3600 + 30 * 60)).toBe("3d 2h");
    expect(formatDuration(5 * 3600 + 39 * 60)).toBe("5h 39m");
    expect(formatDuration(12 * 60)).toBe("12m");
  });

  it("says a bucket is resetting rather than counting down past zero", () => {
    expect(formatCountdown(Date.now() / 1000 - 60)).toBe("resetting");
    expect(formatCountdown(undefined)).toBe("");
  });
});

describe("what a forecast says", () => {
  it("names the verdict and what it means for the reader", () => {
    expect(forecastHeadline(forecast({ verdict: "atRisk" }))).toContain("run out");
    expect(forecastHeadline(forecast({ verdict: "surplus" }))).toContain("Surplus");
    expect(forecastHeadline(forecast({ verdict: "learning" }))).toContain("Learning");
  });

  /// Projected usage may exceed 100: the visible quota is capped but the size
  /// of the shortfall is not, and clamping would erase the difference between
  /// just short and hopelessly short.
  it("does not present a shortfall as though the bar were merely full", () => {
    const detail = forecastDetail(
      forecast({ verdict: "atRisk", projectedUsedPercent: 180 }),
      Date.now() / 1000,
    );
    expect(detail ?? "").not.toContain("100%");
  });
});

describe("how much a verdict should be trusted", () => {
  it("says so in the words the native app uses", () => {
    expect(forecastConfidence(forecast({ confidence: "high" }))).toBe("High confidence");
    expect(forecastConfidence(forecast({ confidence: "medium" }))).toBe(
      "Medium confidence",
    );
  });

  /// "Learning" twice on two lines is not two pieces of information.
  it("stays quiet when the verdict already said it", () => {
    expect(
      forecastConfidence(forecast({ verdict: "learning", confidence: "learning" })),
    ).toBeNull();
  });

  /// A confident-sounding verdict resting on thin evidence is the one case
  /// this line exists for.
  it("says so when a real verdict rests on thin evidence", () => {
    expect(
      forecastConfidence(forecast({ verdict: "surplus", confidence: "learning" })),
    ).toBe("Learning");
  });
});
