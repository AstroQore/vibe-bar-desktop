import { describe, expect, it } from "vitest";
import { density, overviewDensity, popoverDensity } from "./theme";

describe("the density profiles are the native ones", () => {
  it("carries Theme.swift's numbers", () => {
    const r = density("regular");
    expect([r.popoverPaddingH, r.popoverPaddingV, r.interSectionSpacing, r.cardPadding, r.cardSpacing])
      .toEqual([16, 14, 14, 14, 10]);
    expect([r.bucketRowSpacing, r.bucketGroupSpacing, r.popoverWidth, r.cardCornerRadius]).toEqual([6, 12, 420, 14]);
    expect([r.titleFontSize, r.subtitleFontSize, r.bucketTitleFontSize, r.bucketPercentFontSize]).toEqual([16, 12, 13, 13]);
    expect([r.resetCountdownFontSize, r.bucketBarHeight, r.segmentedFontSize]).toEqual([11, 12, 12]);
    expect([r.headerHeight, r.overviewSummaryHeight, r.overviewCostChartHeight]).toEqual([40, 178, 190]);
    expect(density("compact").popoverWidth).toBe(360);
    expect(density("spacious").popoverWidth).toBe(500);
  });

  it("doubles the width for the overview, as native does", () => {
    expect(overviewDensity("compact").popoverWidth).toBe(860);
    expect(overviewDensity("regular").popoverWidth).toBe(960);
    expect(overviewDensity("spacious").popoverWidth).toBe(1120);
    expect(overviewDensity("regular").cardPadding).toBe(14);
  });

  it("falls back to regular for a profile this build does not know", () => {
    expect(popoverDensity("spacious")).toBe("spacious");
    expect(popoverDensity("wide")).toBe("regular");
    expect(popoverDensity(undefined)).toBe("regular");
  });
});
