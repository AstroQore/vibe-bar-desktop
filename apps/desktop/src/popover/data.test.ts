import { describe, expect, it } from "vitest";
import { bucketGroups, companySections, costSummary, overviewDescriptors, statusState, upcomingResets, usageMix } from "./data";
import { FIXTURE_COST, FIXTURE_NOW, FIXTURE_SETTINGS, FIXTURE_VIEW } from "./fixture";

describe("the overview's cards, in native's order", () => {
  it("declares summaries, then the auxiliaries, then one quota card per visible company", () => {
    const ids = overviewDescriptors(FIXTURE_VIEW, FIXTURE_SETTINGS, FIXTURE_COST).map((d) => d.id);
    expect(ids).toEqual([
      "overview-summary-cost", "overview-summary-status", "overview-upcoming-resets", "overview-usage-mix",
      "overview-quota:codex", "overview-quota:claude", "overview-quota:gemini", "overview-quota:grok",
    ]);
  });

  it("leaves the usage mix out when there is no cost data", () => {
    const ids = overviewDescriptors(FIXTURE_VIEW, FIXTURE_SETTINGS, null).map((d) => d.id);
    expect(ids).not.toContain("overview-usage-mix");
  });

  it("groups buckets the way native's bucketContent does", () => {
    const antigravity = FIXTURE_VIEW.accounts.find((a) => a.tool === "antigravity")!;
    expect(bucketGroups(antigravity.buckets).map((g) => [g.title, g.buckets.length])).toEqual([
      ["Gemini models", 2], ["Claude and GPT models", 2],
    ]);
    const codex = FIXTURE_VIEW.accounts.find((a) => a.tool === "codex")!;
    expect(bucketGroups(codex.buckets).map((g) => g.title)).toEqual([null, "GPT-5.3 Codex Spark"]);
  });
});

describe("the cost card's twelve numbers", () => {
  it("folds yesterday and the peak day from the daily rows", () => {
    const s = costSummary(FIXTURE_COST, FIXTURE_NOW)!;
    expect(s.totalCost).toBe(91_775);
    expect(s.todayCost).toBe(2292);
    expect(s.yesterdayCost).toBe(694);
    expect(s.peakDayCost).toBe(2979);
    expect(s.peakDayTokens).toBe(3_190_000_000);
    expect(s.yesterdayTokens).toBe(755_370_000);
  });
});

describe("the usage mix", () => {
  it("keeps the top four and folds the rest into Other", () => {
    const many = { ...FIXTURE_COST, models: Array.from({ length: 6 }, (_, i) => ({ pricedCostMicros: 0, tokens: 1000 * (6 - i), requests: 1, harness: `h${i}`, model: `m${i}`, unpricedEvents: 0 })) };
    const { slices, total } = usageMix(many, "models");
    expect(slices.map((s) => s.label)).toEqual(["m0", "m1", "m2", "m3", "Other"]);
    expect(total).toBe(1000 * 21);
  });

  it("says why a dimension is empty rather than drawing nothing", () => {
    expect(usageMix(FIXTURE_COST, "projects").empty).toMatch(/Project attribution/);
    expect(usageMix(null, "harnesses").empty).toBe("No local usage in this range.");
  });
});

describe("upcoming resets", () => {
  it("lists the core providers' buckets inside seven days, soonest first, labelled like native", () => {
    const events = upcomingResets(FIXTURE_VIEW, FIXTURE_SETTINGS, FIXTURE_NOW);
    expect(events[0].label).toBe("Claude · 5 Hours");
    expect(events.map((e) => e.resetAt)).toEqual([...events.map((e) => e.resetAt)].sort((a, b) => a - b));
    // A group that merely repeats the SubProvider is not said twice.
    const anti = events.find((e) => e.label.startsWith("AntiGravity"));
    expect(anti?.label).toMatch(/^AntiGravity · (Gemini models|Claude and GPT models) · 5 Hours$/);
  });
});

describe("service status states", () => {
  it("reads a missing feed as checking, not down", () => {
    expect(statusState(undefined, false)).toBe("checking");
    expect(statusState({ tool: "codex", indicator: "none", description: "", incidents: [] }, false)).toBe("up");
    expect(statusState({ tool: "codex", indicator: "critical", description: "", incidents: [] }, false)).toBe("down");
    expect(statusState({ tool: "codex", indicator: "minor", description: "", incidents: [] }, false)).toBe("degraded");
  });
});

describe("a company's SubProvider sections", () => {
  it("files each bucket under the SubProvider the naming contract gives it", () => {
    // Cursor's account carries the Grok Bot weekly bucket; it is Grok Bot's.
    const view = {
      ...FIXTURE_VIEW,
      accounts: [
        {
          accountId: "cursor-demo", tool: "cursor", queriedAt: FIXTURE_NOW, origin: "sharedCache",
          buckets: [
            { id: "models", title: "Monthly", shortLabel: "Monthly", usedPercent: 24 },
            { id: "grok_bot_weekly", title: "Weekly", shortLabel: "Weekly", usedPercent: 0 },
          ],
        },
        ...FIXTURE_VIEW.accounts.filter((a) => a.tool === "grok"),
      ],
    } as typeof FIXTURE_VIEW;
    const sections = companySections(view, FIXTURE_SETTINGS, "SpaceXAI");
    const names = sections.map((s) => s.subProvider);
    expect(names).toContain("Grok Bot");
    expect(names).toContain("Cursor");
    const grokBot = sections.find((s) => s.subProvider === "Grok Bot")!;
    expect(grokBot.blocks.flatMap((b) => b.buckets.map((x) => x.id))).toEqual(["grok_bot_weekly"]);
    const cursor = sections.find((s) => s.subProvider === "Cursor")!;
    expect(cursor.blocks.flatMap((b) => b.buckets.map((x) => x.id))).toEqual(["models"]);
  });
});
