import { describe, expect, it } from "vitest";
import type { PresentationSettings } from "../../api";
import { filterSections, formatPerMillion, routeStatus, sections } from "./model";

const settings = {
  visibleCoreProviders: ["codex", "claude"],
  miscProviderInstances: [
    { id: "copilot-1", tool: "copilot", name: "Copilot", isVisible: true },
    { id: "kilo-1", tool: "kilo", name: "", isVisible: false },
  ],
} as unknown as PresentationSettings;

describe("sidebar", () => {
  it("lists the native pages, core providers with visibility, and misc instances", () => {
    const entries = sections(settings);
    expect(entries.filter((e) => e.group === "settings").map((e) => e.title)).toEqual([
      "System", "Cost Data", "Model Pricing", "MCP Server", "Remote Probes", "Privacy", "Mini Window", "Layout",
    ]);
    const core = entries.filter((e) => e.group === "core");
    expect(core.map((e) => `${e.title}:${e.enabled}`)).toEqual(["OpenAI:true", "Anthropic:true", "Google AI:false", "SpaceXAI:false"]);
    const misc = entries.filter((e) => e.group === "misc");
    expect(misc.map((e) => e.title)).toEqual(["Browser Cookies", "Copilot", "kilo"]);
    expect(misc[2].enabled).toBe(false);
  });

  it("filters by title", () => {
    expect(filterSections(sections(null), "min").map((e) => e.title)).toEqual(["Mini Window"]);
    expect(filterSections(sections(null), "")).toHaveLength(sections(null).length);
  });

  it("offers no menu-bar page: that surface is the native app's", () => {
    expect(filterSections(sections(null), "menu")).toEqual([]);
  });
});

describe("routes and prices", () => {
  it("reports CLI/OAuth routes from the quota view and says cookies are unused", () => {
    const view = { accounts: [{ accountId: "a", tool: "codex", buckets: [], queriedAt: 1, origin: "own" }], hasSharedData: true, isDemo: false } as never;
    expect(routeStatus("cli", "openAI", view)).toBe("found");
    expect(routeStatus("oauth", "anthropic", view)).toBe("missing");
    expect(routeStatus("cookies", "openAI", view)).toBe("unused");
    expect(routeStatus("cli", "openAI", null)).toBe("missing");
  });

  it("formats per-million rates by magnitude", () => {
    expect(formatPerMillion(null)).toBe("—");
    expect(formatPerMillion(1.25)).toBe("$1.25");
    expect(formatPerMillion(15)).toBe("$15.0");
    expect(formatPerMillion(150)).toBe("$150");
  });
});
