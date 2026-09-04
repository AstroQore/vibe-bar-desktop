import { describe, expect, it } from "vitest";

import type { AccountQuota, PresentationSettings, QuotaForecast, QuotaView } from "../api";
import {
  arrange,
  fannedOffsets,
  flatten,
  forecastLine,
  railEvents,
  stripBands,
  stripDensity,
} from "./MiniQuota";

const NOW = 1_800_000_000;

function bucket(
  id: string,
  title: string,
  shortLabel: string,
  groupTitle?: string,
): AccountQuota["buckets"][number] {
  return {
    id,
    title,
    shortLabel,
    usedPercent: 40,
    resetAt: NOW + 86_400,
    rawWindowSeconds: 604_800,
    groupTitle,
  } as AccountQuota["buckets"][number];
}

function account(tool: string, buckets: AccountQuota["buckets"]): AccountQuota {
  return {
    accountId: `acct-${tool}`,
    tool,
    queriedAt: NOW - 60,
    origin: "live",
    buckets,
  } as AccountQuota;
}

function view(...accounts: AccountQuota[]): QuotaView {
  return { accounts, generatedAt: NOW, hasSharedData: true } as unknown as QuotaView;
}

const settings = { displayMode: "remaining", customLabels: {} } as unknown as
  PresentationSettings;

describe("arranging dials along the quota axis", () => {
  it("puts a provider's windows under one company and SubProvider", () => {
    const companies = arrange(
      view(
        account("claude", [
          bucket("five_hour", "5 Hours", "5 Hours"),
          bucket("weekly", "Weekly", "Weekly"),
        ]),
      ),
      settings,
      ["claude.five_hour", "claude.weekly"],
    );
    expect(companies).toHaveLength(1);
    expect(companies[0].name).toBe("Anthropic");
    expect(companies[0].subProviders).toHaveLength(1);
    expect(companies[0].subProviders[0].name).toBe("Claude");
    expect(companies[0].subProviders[0].groups[0].cells).toHaveLength(2);
  });

  /// Two SubProviders under one company is the whole reason for the middle
  /// level: Gemini Web and AntiGravity are both Google AI, and a reader
  /// cannot tell two "Weekly" dials apart without it.
  it("keeps two SubProviders of one company apart", () => {
    const companies = arrange(
      view(
        account("gemini", [bucket("weekly", "Weekly", "Weekly")]),
        account("antigravity", [bucket("gemini_weekly", "Weekly", "Weekly", "Gemini")]),
      ),
      settings,
      ["gemini.weekly", "antigravity.gemini_weekly"],
    );
    expect(companies).toHaveLength(1);
    expect(companies[0].name).toBe("Google AI");
    expect(companies[0].subProviders.map((s) => s.name)).toEqual([
      "Gemini Web",
      "AntiGravity",
    ]);
  });

  it("splits one SubProvider's model groups", () => {
    const companies = arrange(
      view(
        account("codex", [
          bucket("weekly", "Weekly", "Weekly"),
          bucket("gpt_5_3_codex_spark_weekly", "Weekly", "Spark Weekly",
            "GPT-5.3 Codex Spark"),
        ]),
      ),
      settings,
      ["codex.weekly", "codex.gpt_5_3_codex_spark_weekly"],
    );
    const groups = companies[0].subProviders[0].groups;
    expect(groups.map((g) => g.label)).toEqual(["All", "Spark"]);
  });

  /// The bucket that belongs to a SubProvider its account does not.
  it("files Grok Bot under its own SubProvider, not Cursor's", () => {
    const companies = arrange(
      view(
        account("cursor", [
          bucket("models", "Monthly", "Cursor", "Cursor Models"),
          bucket("grok_bot_weekly", "Weekly", "Grok Bot", "Grok Bot"),
        ]),
      ),
      settings,
      ["cursor.models", "cursor.grok_bot_weekly"],
    );
    expect(companies[0].subProviders.map((s) => s.name)).toEqual(["Cursor", "Grok Bot"]);
  });

  it("names a cell by its window, since the heading carries the group", () => {
    const companies = arrange(
      view(
        account("claude", [bucket("weekly_fable", "Weekly", "Fable Weekly", "Fable")]),
      ),
      settings,
      ["claude.weekly_fable"],
    );
    expect(companies[0].subProviders[0].groups[0].cells[0].label).toBe("Weekly");
  });

  it("names a cell from its bucket, not from the other client's menu bar", () => {
    // The override that used to win here was the native menu bar's per-field
    // label: a name chosen for a strip no platform this client runs on has,
    // reaching into a window that has nothing to do with it.
    const companies = arrange(
      view(account("claude", [bucket("weekly", "Weekly", "Weekly")])),
      settings,
      ["claude.weekly"],
    );
    expect(companies[0].subProviders[0].groups[0].cells[0].label).toBe("Weekly");
  });

  it("keeps the order the fields were chosen in", () => {
    const companies = arrange(
      view(
        account("claude", [bucket("weekly", "Weekly", "Weekly")]),
        account("codex", [bucket("weekly", "Weekly", "Weekly")]),
      ),
      settings,
      ["codex.weekly", "claude.weekly"],
    );
    expect(companies.map((c) => c.name)).toEqual(["OpenAI", "Anthropic"]);
  });

  it("skips a field naming a bucket that is not there", () => {
    const companies = arrange(
      view(account("claude", [bucket("weekly", "Weekly", "Weekly")])),
      settings,
      ["claude.weekly", "claude.gone", "malformed", "claude."],
    );
    expect(companies[0].subProviders[0].groups[0].cells).toHaveLength(1);
  });

  it("shows what is left or what is used, as the settings ask", () => {
    const fields = ["claude.weekly"];
    const data = view(account("claude", [bucket("weekly", "Weekly", "Weekly")]));
    const left = arrange(data, settings, fields);
    const used = arrange(
      data,
      { ...settings, displayMode: "used" } as PresentationSettings,
      fields,
    );
    expect(left[0].subProviders[0].groups[0].cells[0].value).toBe(60);
    expect(used[0].subProviders[0].groups[0].cells[0].value).toBe(40);
  });
});

describe("the one line under a dial", () => {
  function forecast(overrides: Partial<QuotaForecast>): QuotaForecast {
    return {
      verdict: "enough",
      projectedUsedPercent: 62,
      ...overrides,
    } as QuotaForecast;
  }

  it("counts down to running out, and hedges when the verdict does", () => {
    const runOut = { runOutAt: NOW + 3 * 86_400 + 14 * 3600 };
    expect(forecastLine(forecast({ verdict: "atRisk", ...runOut }), NOW)).toBe(
      "out 3d 14h",
    );
    expect(forecastLine(forecast({ verdict: "watch", ...runOut }), NOW)).toBe(
      "may run out 3d 14h",
    );
  });

  /// A run-out time already in the past is not news about the future; the
  /// verdict is what is left to say.
  it("falls back to the verdict once the run-out time has passed", () => {
    expect(forecastLine(forecast({ verdict: "atRisk", runOutAt: NOW - 60 }), NOW)).toBe(
      "risk",
    );
  });

  it("says how much is expected to survive, when something is", () => {
    expect(forecastLine(forecast({ verdict: "enough" }), NOW)).toBe("38% left");
    expect(forecastLine(forecast({ verdict: "surplus" }), NOW)).toBe(
      "surplus · 38% left",
    );
    expect(forecastLine(forecast({ verdict: "learning" }), NOW)).toBe(
      "learning · 38% left",
    );
  });

  /// Projected usage may exceed 100 — the shortage's size is not capped even
  /// though the bar is — and "-80% left" would be nonsense.
  it("never reports negative capacity", () => {
    expect(
      forecastLine(forecast({ verdict: "enough", projectedUsedPercent: 180 }), NOW),
    ).toBe("0% left");
  });
});

describe("flattening the tree for the layouts that page or tile", () => {
  const data = view(
    account("codex", [bucket("five_hour", "5 Hours", "5 Hours"), bucket("weekly", "Weekly", "Weekly")]),
    account("claude", [
      bucket("weekly", "Weekly", "Weekly"),
      bucket("weekly_opus", "Weekly", "Opus Weekly", "Opus"),
    ]),
  );

  /// `arrange` gathers each company's buckets together, so the tree cannot be
  /// walked to recover the order the user picked. Focus pages through these in
  /// that order, which is native's rule.
  it("follows the order the fields were chosen in, not the tree's", () => {
    const fields = ["codex.weekly", "claude.weekly", "codex.five_hour"];
    const companies = arrange(data, settings, fields);

    expect(flatten(companies).map((entry) => entry.cell.id)).toEqual([
      "codex.weekly",
      "codex.five_hour",
      "claude.weekly",
    ]);
    expect(flatten(companies, fields).map((entry) => entry.cell.id)).toEqual(fields);
  });

  /// A bucket the order does not mention — a field found at runtime that the
  /// saved selection predates — still has to appear.
  it("keeps a bucket the order does not name, after the ones it does", () => {
    const fields = ["codex.weekly", "claude.weekly"];
    const companies = arrange(data, settings, [...fields, "claude.weekly_opus"]);

    expect(flatten(companies, fields).map((entry) => entry.cell.id)).toEqual([
      "codex.weekly",
      "claude.weekly",
      "claude.weekly_opus",
    ]);
  });

  /// Without the group the flat layouts show two of a SubProvider's buckets
  /// as the same "Weekly".
  it("carries the model group, which the tree keeps in its heading", () => {
    const companies = arrange(data, settings, ["claude.weekly", "claude.weekly_opus"]);
    const entries = flatten(companies);

    expect(entries.map((entry) => entry.groupLabel)).toEqual(["All", "Opus"]);
    expect(entries.map((entry) => entry.subProvider)).toEqual(["Claude", "Claude"]);
  });
});

describe("the rail's seven-day horizon", () => {
  function entryAt(id: string, secondsAhead: number, used: number) {
    const b = bucket(id, "Weekly", "Weekly");
    return {
      cell: {
        id: `claude.${id}`,
        bucket: { ...b, resetAt: NOW + secondsAhead, usedPercent: used },
        label: "Weekly",
        value: 100 - used,
        showsUsed: false,
      },
      company: "Anthropic",
      tool: "claude",
      subProvider: "Claude",
      groupLabel: null,
    } as unknown as Parameters<typeof railEvents>[0][number];
  }

  it("keeps only what refills inside the horizon, soonest first", () => {
    const events = railEvents(
      [
        entryAt("far", 9 * 86_400, 50),
        entryAt("soon", 3600, 50),
        entryAt("mid", 3 * 86_400, 50),
      ],
      NOW,
    );
    expect(events.map((e) => e.entry.cell.id)).toEqual(["claude.soon", "claude.mid"]);
  });

  /// A reset already behind us is not a refill to come.
  it("drops a reset in the past", () => {
    expect(railEvents([entryAt("gone", -60, 50)], NOW)).toHaveLength(0);
  });

  /// A bucket nothing has been spent from has no refill to draw, and a marker
  /// of no height would claim something happens when nothing does.
  it("drops a bucket with nothing to come back", () => {
    expect(railEvents([entryAt("untouched", 3600, 0)], NOW)).toHaveLength(0);
    expect(railEvents([entryAt("barely", 3600, 1)], NOW)).toHaveLength(1);
  });

  it("reads the gain and what is left from the same bucket", () => {
    const [event] = railEvents([entryAt("half", 3600, 63)], NOW);
    expect(event.gain).toBe(63);
    expect(event.remaining).toBe(37);
  });
});

describe("fanning markers that would overlap", () => {
  const at = (fraction: number) => ({ fraction }) as Parameters<typeof fannedOffsets>[0][number];

  /// Two quotas refilling minutes apart are one blob otherwise, and the lane's
  /// whole job is saying how many are coming and when.
  it("spreads markers that land in one slot, centred on it", () => {
    const offsets = fannedOffsets([at(0.5), at(0.5), at(0.5)], 536);
    expect(offsets).toEqual([-9, 0, 9]);
  });

  /// Markers that do not collide keep their position — except at the very
  /// ends, where a 7-wide bar centred on the edge would hang half off the
  /// lane. The inset belongs here rather than in a clamp at draw time: a
  /// clamp applied after the fanning is what used to put whole groups back
  /// on one x.
  it("leaves a marker alone unless the lane's edge is in the way", () => {
    expect(fannedOffsets([at(0.25), at(0.5), at(0.75)], 536)).toEqual([0, 0, 0]);
    expect(fannedOffsets([at(0), at(0.5), at(1)], 536)).toEqual([4, 0, -4]);
  });

  it("moves nothing on its own", () => {
    expect(fannedOffsets([at(0.42)], 536)).toEqual([0]);
  });
});

describe("fanned groups at the ends of the lane", () => {
  const at = (fraction: number) => ({ fraction }) as Parameters<typeof fannedOffsets>[0][number];
  const positions = (fractions: number[], width = 536) => {
    const events = fractions.map(at);
    const offsets = fannedOffsets(events, width);
    return events.map((e, i) => Math.round(width * e.fraction + offsets[i]));
  };

  /// Clamping each marker on its own puts the whole group back on one x —
  /// exactly where fanning is needed: several quotas resetting within the hour.
  it("keeps a group at the near end spread out", () => {
    const xs = positions([0, 0, 0]);
    expect(new Set(xs).size).toBe(3);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(4);
  });

  it("keeps a group at the far end spread out", () => {
    const xs = positions([1, 1, 1]);
    expect(new Set(xs).size).toBe(3);
    expect(Math.max(...xs)).toBeLessThanOrEqual(536 - 4);
  });

  /// And the spacing itself survives the shift.
  it("moves the group without squashing it", () => {
    const xs = positions([0, 0, 0]);
    expect(xs[1] - xs[0]).toBe(9);
    expect(xs[2] - xs[1]).toBe(9);
  });
});

describe("markers that collide across slot boundaries", () => {
  const width = 536;
  const at = (x: number) => ({ fraction: x / width }) as Parameters<typeof fannedOffsets>[0][number];
  const positions = (xs: number[]) => {
    const events = xs.map(at);
    const offsets = fannedOffsets(events, width);
    return events.map((e, i) => width * e.fraction + offsets[i]);
  };

  /// The bars are 7 wide and a step is 9, so landing in *neighbouring* slots
  /// is not separation. Grouping by a rounded slot id left these overlapped.
  it("separates two markers a fraction of a pixel apart", () => {
    const [a, b] = positions([94.4, 94.6]);
    expect(Math.abs(b - a)).toBeGreaterThanOrEqual(7);
  });

  it("separates two pairs that straddle a boundary", () => {
    const xs = positions([94.4, 94.6, 95.1, 95.4]);
    const sorted = [...xs].sort((l, r) => l - r);
    for (let i = 1; i < sorted.length; i++) {
      expect(sorted[i] - sorted[i - 1]).toBeGreaterThanOrEqual(7);
    }
  });

  /// And markers genuinely far apart are still left where they belong.
  it("leaves markers that do not collide alone", () => {
    expect(positions([100, 200, 300])).toEqual([100, 200, 300]);
  });
});

describe("a shifted group must not land on the marker beside it", () => {
  const width = 536;
  const at = (x: number) => ({ fraction: x / width }) as Parameters<typeof fannedOffsets>[0][number];

  /// Centres at 0, 8, 17: the first two cluster and get pushed inward off the
  /// edge, which used to put them at 4 and 13 — four pixels from the 17 that
  /// had been classified as a separate group, so two 7-wide bars overlapped.
  it("keeps every marker a bar apart after the group is moved inward", () => {
    const events = [0, 8, 17].map(at);
    const offsets = fannedOffsets(events, width);
    const xs = events.map((e, i) => width * e.fraction + offsets[i]).sort((l, r) => l - r);
    for (let i = 1; i < xs.length; i++) {
      expect(xs[i] - xs[i - 1]).toBeGreaterThanOrEqual(7);
    }
    expect(xs[0]).toBeGreaterThanOrEqual(4);
  });
});

describe("the strip's bands", () => {
  // Native computes these before drawing, and the shell sizes the window from
  // them; a wrap decided by CSS instead would be measured at one width and
  // drawn at another.
  it("fits a full band before wrapping, and pads a partial one to full width", () => {
    // (1180 - 14 - 26 + 8) / (132 + 8) = 8 roomy cells to a band.
    expect(stripBands(8, "roomy")).toMatchObject({ perBand: 8, bands: 1 });
    expect(stripBands(9, "roomy")).toMatchObject({ perBand: 8, bands: 2 });
    // Nine cells is as wide as sixteen: the width is a whole band either way.
    expect(stripBands(9, "roomy").width).toBe(stripBands(16, "roomy").width);
  });

  it("never exceeds the row width it wraps at", () => {
    for (const density of ["roomy", "twoLine", "narrow"]) {
      for (const count of [1, 5, 16, 17, 40, 128]) {
        expect(stripBands(count, density).width).toBeLessThanOrEqual(1180);
      }
    }
  });

  it("narrows the cell rather than the band count", () => {
    // 96-point cells fit eleven where 132-point ones fit eight.
    expect(stripBands(20, "narrow").perBand).toBe(11);
    expect(stripBands(20, "roomy").perBand).toBe(8);
  });

  it("wraps twoLine by the column, so two entries share one slot", () => {
    // Sixteen entries are eight columns: one band, same as eight roomy cells.
    expect(stripBands(16, "twoLine")).toMatchObject({ perBand: 8, bands: 1 });
    expect(stripBands(17, "twoLine").bands).toBe(2);
    // An odd count still occupies the whole column.
    expect(stripBands(15, "twoLine")).toMatchObject({ perBand: 8, bands: 1 });
  });

  it("is two bands tall plus the gap between them", () => {
    const one = stripBands(8, "roomy");
    const two = stripBands(9, "roomy");
    expect(one.height).toBe(40);
    expect(two.height).toBe(40 * 2 + 2);
  });

  it("has no size when there is nothing to draw", () => {
    expect(stripBands(0, "roomy").bands).toBe(0);
    expect(stripBands(0, "roomy").height).toBe(0);
  });

  it("falls back to roomy for a density this build does not know", () => {
    expect(stripDensity("spacious")).toBe("roomy");
    expect(stripDensity(undefined)).toBe("roomy");
    expect(stripBands(9, "spacious")).toMatchObject(stripBands(9, "roomy"));
  });
});
