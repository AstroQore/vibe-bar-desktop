import { describe, expect, it } from "vitest";
import type { SessionRow } from "../../api";
import {
  applyFolderFilters,
  buildListingQuery,
  collapsed,
  collapses,
  findMatches,
  groupRows,
  harnessSummary,
  indexStatusText,
  outline,
  pageLabel,
  pageRange,
  parseFolderList,
  projectTitle,
  relativeTime,
  sinceFor,
  sortRows,
  splitHighlights,
} from "./model";
import { findMatches, foldedLines, messageParts, rowSummary, rowTitle } from "./model";

function row(id: string, projectDir: string | undefined, lastActiveAt: number): SessionRow {
  return { provider: "codex", harness: "Codex", sessionId: id, sessionRef: id, projectDir, lastActiveAt };
}

describe("listing query", () => {
  const now = 1_756_800_000;

  it("maps companies to provider ids and leaves everything open by default", () => {
    expect(buildListingQuery({ search: "", companies: null, harnesses: null, range: "all", offset: 0, limit: 250, now })).toEqual({
      query: undefined,
      providers: undefined,
      harnesses: undefined,
      since: undefined,
      offset: 0,
      limit: 250,
    });
    const anthropic = buildListingQuery({ search: " deploy ", companies: ["Anthropic"], harnesses: [], range: "week", offset: 250, limit: 250, now });
    expect(anthropic.query).toBe("deploy");
    expect(anthropic.providers).toEqual(["claude", "claudeCowork"]);
    expect(anthropic.harnesses).toEqual([]);
    expect(anthropic.since).toBe(now - 7 * 86_400);
  });

  it("starts Today at local midnight", () => {
    const since = sinceFor("today", now)!;
    expect(new Date(since * 1000).getHours()).toBe(0);
    expect(now - since).toBeLessThanOrEqual(86_400);
  });
});

describe("folders and grouping", () => {
  it("parses comma lists and filters by substring", () => {
    expect(parseFolderList(" /a, /b ,, ")).toEqual(["/a", "/b"]);
    const rows = [row("1", "/work/app", 3), row("2", "/work/vendor/lib", 2), row("3", undefined, 1)];
    expect(applyFolderFilters(rows, { include: ["/work"], exclude: ["vendor"] }).map((r) => r.sessionId)).toEqual(["1"]);
    expect(applyFolderFilters(rows, { include: [], exclude: [] })).toHaveLength(3);
  });

  it("titles projects by their last path component", () => {
    expect(projectTitle("/Users/example/Coding/vibe-bar/")).toBe("vibe-bar");
    expect(projectTitle(undefined)).toBe("No project");
  });

  it("groups by project with the newest group first and sorts either way", () => {
    const rows = [row("1", "/x/app", 10), row("2", "/x/lib", 30), row("3", "/x/app", 20)];
    const groups = groupRows(rows);
    expect(groups.map((g) => g.title)).toEqual(["lib", "app"]);
    expect(groups[1].rows).toHaveLength(2);
    expect(sortRows(rows, "recentFirst").map((r) => r.sessionId)).toEqual(["2", "3", "1"]);
    expect(sortRows(rows, "oldestFirst").map((r) => r.sessionId)).toEqual(["1", "3", "2"]);
  });

  it("formats relative times like the list", () => {
    const now = 1_000_000;
    expect(relativeTime(now - 30, now)).toBe("just now");
    expect(relativeTime(now - 3_700, now)).toBe("1h ago");
    expect(relativeTime(now - 3 * 86_400, now)).toBe("3d ago");
    expect(relativeTime(undefined, now)).toBe("");
  });

  it("summarises menus and index status", () => {
    expect(harnessSummary(null, 4)).toBe("All");
    expect(harnessSummary(["a", "b"], 4)).toBe("2/4");
    expect(indexStatusText(12, 12, true)).toBe("12 sessions");
    expect(indexStatusText(250, 1_200, true)).toBe("250 of 1200 sessions");
    expect(indexStatusText(1, undefined, true)).toBe("1 session");
    expect(indexStatusText(0, undefined, false)).toBe("index unavailable");
  });
});

describe("transcript", () => {
  it("pages 80 messages and clamps the last page", () => {
    expect(pageRange(0, 200)).toEqual({ lower: 0, upper: 80 });
    expect(pageRange(160, 200)).toEqual({ lower: 120, upper: 200 });
    expect(pageRange(0, 20)).toEqual({ lower: 0, upper: 20 });
    expect(pageLabel({ lower: 0, upper: 20 }, 20)).toBe("Messages 1–20 of 20");
  });

  it("collapses very long messages to a prefix", () => {
    const long = "x".repeat(3_001);
    expect(collapses(long)).toBe(true);
    expect(collapsed(long)).toHaveLength(1_501);
    expect(collapses("short")).toBe(false);
  });

  it("finds and highlights matches case-insensitively", () => {
    const messages = [
      { role: "user" as const, text: "Fix the Chip row" },
      { role: "assistant" as const, text: "Looking" },
      { role: "tool" as const, text: "chip.tsx" },
    ];
    expect(findMatches(messages, "chip")).toEqual([0, 2]);
    expect(findMatches(messages, "  ")).toEqual([]);
    expect(splitHighlights("Fix the Chip row", "chip")).toEqual([
      { text: "Fix the ", hit: false },
      { text: "Chip", hit: true },
      { text: " row", hit: false },
    ]);
  });

  it("outlines user prompts by their first non-empty line", () => {
    const entries = outline([
      { role: "system", text: "s" },
      { role: "user", text: "\n\nMake it wrap\nplease" },
      { role: "assistant", text: "ok" },
      { role: "user", text: "Ship it" },
    ]);
    expect(entries).toEqual([
      { index: 1, seq: 1, title: "Make it wrap" },
      { index: 3, seq: 2, title: "Ship it" },
    ]);
  });
});

describe("a message's parts", () => {
  it("turns the kit's tool-call line into a name, a purpose and fields", () => {
    const parts = messageParts('Looking first.\n[tool call] Bash {"command":"ls -lh","description":"Check the size"}');
    expect(parts[0]).toEqual({ kind: "text", text: "Looking first." });
    expect(parts[1]).toMatchObject({ kind: "tool", name: "Bash", purpose: "Check the size" });
    expect((parts[1] as { fields: Array<{ key: string; block: boolean }> }).fields).toEqual([{ key: "command", value: "ls -lh", block: true }]);
  });
  it("reads the native app's two-line form as well", () => {
    const parts = messageParts('[Tool: Read]\n{"file_path":"Sources/A.swift"}');
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ kind: "tool", name: "Read", fields: [{ key: "file_path", value: "Sources/A.swift", block: false }] });
  });
  it("keeps a call without arguments and prose around it", () => {
    const parts = messageParts("[tool call] Glob\nDone.");
    expect(parts).toEqual([{ kind: "tool", name: "Glob", purpose: undefined, fields: [] }, { kind: "text", text: "Done." }]);
  });
  it("keeps unparseable arguments as one field", () => {
    const parts = messageParts("[tool call] Bash not json");
    expect(parts[0]).toMatchObject({ kind: "tool", name: "Bash", fields: [{ key: "input", value: "not json", block: false }] });
  });
});

describe("row title and summary", () => {
  const base = { sessionId: "abc", provider: "claude", harness: "Claude Code" };
  it("prefers the title, then the summary, then the excerpt, then the id", () => {
    expect(rowTitle({ ...base, title: "  Fix it " })).toBe("Fix it");
    expect(rowTitle({ ...base, summary: "A summary" })).toBe("A summary");
    expect(rowTitle({ ...base, excerpt: "an <mark>excerpt</mark>" })).toBe("an excerpt");
    expect(rowTitle({ ...base })).toBe("abc");
  });
  it("does not repeat the title as its summary", () => {
    expect(rowSummary({ ...base, summary: "Same" })).toBeUndefined();
    expect(rowSummary({ ...base, title: "Title", summary: "Different" })).toBe("Different");
    expect(rowSummary({ ...base, title: "Title", summary: "Different", excerpt: "hit <mark>x</mark>" })).toBe("hit x");
  });
});

describe("folding a tool result", () => {
  it("shows the first lines and counts the rest", () => {
    const text = Array.from({ length: 12 }, (_, i) => `line ${i + 1}`).join("\n");
    const fold = foldedLines(text);
    expect(fold.hidden).toBe(4);
    expect(fold.shown.split("\n")).toHaveLength(8);
    expect(foldedLines("one\ntwo")).toEqual({ shown: "one\ntwo", hidden: 0 });
  });
});

describe("find over structured tool calls", () => {
  const messages = [
    { role: "assistant" as const, text: '[tool call] Bash {"command":"ls -lh","description":"Check the size"}' },
    { role: "user" as const, text: "plain words" },
  ];
  it("matches what the reader sees — name, purpose, fields — not the log's syntax", () => {
    expect(findMatches(messages, "Check the size")).toEqual([0]);
    expect(findMatches(messages, "ls -lh")).toEqual([0]);
    expect(findMatches(messages, "Bash")).toEqual([0]);
    expect(findMatches(messages, "tool call")).toEqual([]);
    expect(findMatches(messages, "description")).toEqual([]);
    expect(findMatches(messages, "plain")).toEqual([1]);
  });
});
