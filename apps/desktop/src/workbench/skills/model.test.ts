import { describe, expect, it } from "vitest";
import type { SkillInventoryRow } from "../../api";
import { MANAGED_APPS, activationState, appCountHelp, appCounts, countSummary, filterSkills, healthBadge, isOn, sourceBadge } from "./model";

function row(name: string, targets: string[], health = "healthy", description?: string): SkillInventoryRow {
  return { name, directory: `/Users/example/.agents/skills/${name}`, description, targets, health, source: "local" };
}

const app = (id: string) => MANAGED_APPS.find((a) => a.id === id)!;

describe("activation as the inventory sees it", () => {
  it("reads links, shared roots, and the switches it cannot see", () => {
    const linked = row("a", ["claude", "codex"]);
    expect(activationState(linked, app("claude"))).toBe("unknown");
    expect(activationState(linked, app("codex"))).toBe("unknown");
    expect(activationState(linked, app("antigravity"))).toBe("notProjected");
    expect(activationState(linked, app("cursor"))).toBe("coupled");
    expect(activationState(row("b", ["antigravity"]), app("antigravity"))).toBe("enabled");
    expect(isOn("coupled")).toBe(true);
    expect(isOn("notProjected")).toBe(false);
  });

  it("counts what each harness sees", () => {
    const rows = [row("a", ["claude"]), row("b", []), row("c", ["antigravity", "claude"])];
    const counts = appCounts(rows);
    expect(counts.claude).toBe(2);
    expect(counts.antigravity).toBe(1);
    expect(counts.codex).toBe(3);
    expect(counts.cursor).toBe(3);
    expect(appCountHelp(app("codex"), rows)).toBe("Codex sees 3 skills · 0 linked + 3 via the shared skills root");
    expect(appCountHelp(app("claude"), rows)).toBe("Claude Code sees 2 skills · 2 linked");
  });
});

describe("list", () => {
  it("filters by name or description and sorts by name", () => {
    const rows = [row("zeta", [], "healthy", "Deploy things"), row("alpha", []), row("mid", [], "healthy", "zeta-adjacent")];
    expect(filterSkills(rows, "").map((r) => r.name)).toEqual(["alpha", "mid", "zeta"]);
    expect(filterSkills(rows, "ZETA").map((r) => r.name)).toEqual(["mid", "zeta"]);
    expect(filterSkills(rows, "deploy").map((r) => r.name)).toEqual(["zeta"]);
  });

  it("summarises counts and badges", () => {
    expect(countSummary(3, 3)).toBe("3 skills");
    expect(countSummary(1, 1)).toBe("1 skill");
    expect(countSummary(2, 5)).toBe("2 of 5 skills");
    expect(sourceBadge(row("a", []))).toBe("local");
    expect(sourceBadge({ ...row("a", []), source: "acme/skills" })).toBe("acme/skills");
    expect(healthBadge(row("a", []))).toBeNull();
    expect(healthBadge(row("a", [], "unreadable"))).toBe("UNREADABLE");
    expect(healthBadge(row("a", [], "symlink_ignored"))).toBe("SYMLINK");
  });
});
