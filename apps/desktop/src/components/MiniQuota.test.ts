import { describe, expect, it } from "vitest";

import type { AccountQuota, PresentationSettings, QuotaForecast, QuotaView } from "../api";
import { arrange, forecastLine } from "./MiniQuota";

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

  it("lets a chosen label win over the derived one", () => {
    const companies = arrange(
      view(account("claude", [bucket("weekly", "Weekly", "Weekly")])),
      { ...settings, customLabels: { "claude.weekly": "Work" } } as PresentationSettings,
      ["claude.weekly"],
    );
    expect(companies[0].subProviders[0].groups[0].cells[0].label).toBe("Work");
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
