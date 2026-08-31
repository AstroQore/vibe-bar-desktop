import { describe, expect, it } from "vitest";

import { namesItsOwnGroup } from "./Overview";

describe("whether a bucket's model group is worth naming", () => {
  it("names a group the provider gave its own title", () => {
    expect(namesItsOwnGroup("GPT-5.3 Codex Spark", "ChatGPT Agentic")).toBe(true);
    expect(namesItsOwnGroup("Fable", "Claude")).toBe(true);
  });

  it("says nothing for a bucket with no group of its own", () => {
    expect(namesItsOwnGroup(undefined, "Claude")).toBe(false);
    expect(namesItsOwnGroup("   ", "Claude")).toBe(false);
  });

  /// Cursor's Grok Bot bucket has the same word for its group, its
  /// SubProvider and its short label. A heading there would be the third
  /// printing of it, above a row that would make it a fourth.
  it("does not repeat a SubProvider that is already the heading above", () => {
    expect(namesItsOwnGroup("Grok Bot", "Grok Bot")).toBe(false);
    expect(namesItsOwnGroup("grok bot", "Grok Bot")).toBe(false);
  });

  it("keeps a group that merely starts with its SubProvider's name", () => {
    expect(namesItsOwnGroup("Cursor Models", "Cursor")).toBe(true);
  });
});
