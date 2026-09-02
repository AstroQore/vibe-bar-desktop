/**
 * The popover's density profiles, copied number for number from the native
 * client's `Theme.Density` (Sources/VibeBarApp/Views/Theme.swift).
 *
 * Three profiles, one card recipe. Density — not material — is what separates
 * the surfaces natively, so a card in the popover and the same card in the
 * Workbench are one object seen at two magnifications. Every value here is
 * the native one; if one of them looks wrong, check Theme.swift before
 * changing it here, because a card that is 14 points padded on one client
 * and 12 on the other reads as two applications.
 */
export type PopoverDensity = "compact" | "regular" | "spacious";

export interface Density {
  profile: PopoverDensity;
  popoverPaddingH: number;
  popoverPaddingV: number;
  interSectionSpacing: number;
  cardPadding: number;
  cardSpacing: number;
  bucketRowSpacing: number;
  bucketGroupSpacing: number;
  /** The single-provider popover width; the Overview doubles it. */
  popoverWidth: number;
  cardCornerRadius: number;
  titleFontSize: number;
  subtitleFontSize: number;
  bucketTitleFontSize: number;
  bucketPercentFontSize: number;
  resetCountdownFontSize: number;
  bucketBarHeight: number;
  segmentedFontSize: number;
  /** Derived, native's `Density.headerHeight` and friends. */
  headerHeight: number;
  overviewSummaryHeight: number;
  overviewCostChartHeight: number;
}

const BASE: Record<PopoverDensity, Omit<Density, "profile" | "headerHeight" | "overviewSummaryHeight" | "overviewCostChartHeight">> = {
  compact: {
    popoverPaddingH: 12, popoverPaddingV: 10,
    interSectionSpacing: 10, cardPadding: 10, cardSpacing: 8,
    bucketRowSpacing: 5, bucketGroupSpacing: 8,
    popoverWidth: 360, cardCornerRadius: 12,
    titleFontSize: 14, subtitleFontSize: 11,
    bucketTitleFontSize: 12, bucketPercentFontSize: 12,
    resetCountdownFontSize: 10, bucketBarHeight: 10,
    segmentedFontSize: 11,
  },
  regular: {
    popoverPaddingH: 16, popoverPaddingV: 14,
    interSectionSpacing: 14, cardPadding: 14, cardSpacing: 10,
    bucketRowSpacing: 6, bucketGroupSpacing: 12,
    popoverWidth: 420, cardCornerRadius: 14,
    titleFontSize: 16, subtitleFontSize: 12,
    bucketTitleFontSize: 13, bucketPercentFontSize: 13,
    resetCountdownFontSize: 11, bucketBarHeight: 12,
    segmentedFontSize: 12,
  },
  spacious: {
    popoverPaddingH: 20, popoverPaddingV: 18,
    interSectionSpacing: 18, cardPadding: 16, cardSpacing: 12,
    bucketRowSpacing: 8, bucketGroupSpacing: 14,
    popoverWidth: 500, cardCornerRadius: 16,
    titleFontSize: 18, subtitleFontSize: 13,
    bucketTitleFontSize: 14, bucketPercentFontSize: 14,
    resetCountdownFontSize: 12, bucketBarHeight: 14,
    segmentedFontSize: 13,
  },
};

const HEADER_HEIGHT: Record<PopoverDensity, number> = { compact: 34, regular: 40, spacious: 48 };
const SUMMARY_HEIGHT: Record<PopoverDensity, number> = { compact: 148, regular: 178, spacious: 210 };
const COST_CHART_HEIGHT: Record<PopoverDensity, number> = { compact: 154, regular: 190, spacious: 230 };
/** The Overview lays every provider out as a two-column waterfall, so it needs
 *  roughly twice the width of a single-provider popover. Native's numbers. */
const WORKSPACE_WIDTH: Record<PopoverDensity, number> = { compact: 860, regular: 960, spacious: 1120 };

export function density(profile: PopoverDensity): Density {
  return {
    profile,
    ...BASE[profile],
    headerHeight: HEADER_HEIGHT[profile],
    overviewSummaryHeight: SUMMARY_HEIGHT[profile],
    overviewCostChartHeight: COST_CHART_HEIGHT[profile],
  };
}

/** Native's `Theme.overviewDensity(for:)`: the base profile at workspace width. */
export function overviewDensity(profile: PopoverDensity): Density {
  return { ...density(profile), popoverWidth: WORKSPACE_WIDTH[profile] };
}

/** Native's `Theme.detailDensity(for:)` — the same widths today. */
export function detailDensity(profile: PopoverDensity): Density {
  return overviewDensity(profile);
}

export function popoverDensity(name: string | undefined | null): PopoverDensity {
  return name === "compact" || name === "spacious" ? name : "regular";
}

/** Native's card recipe (Theme.Card): fill of the tertiary background at 0.6,
 *  a 0.5pt separator stroke at 0.4, no shadow, no material. */
export const CARD = { fillOpacity: 0.6, strokeOpacity: 0.4, hairlineWidth: 0.5 } as const;
