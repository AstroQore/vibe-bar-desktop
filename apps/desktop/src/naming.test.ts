import { describe, expect, it } from "vitest";

import {
  GROUP_LABELS,
  HIERARCHY,
  bucketLabelFor,
  companyFor,
  groupKeyFor,
  groupLabelFor,
  subProviderFor,
} from "./naming";

/**
 * The quota axis, checked by behaviour rather than by file contents.
 *
 * A Rust test already checks that `naming.ts` is what the generator produces
 * from the contract. That says the table was copied faithfully; it says
 * nothing about whether the rules built on it answer correctly, and every one
 * of the cases below was wrong at some point while this was being written.
 */
describe("the L3 group a bucket belongs to", () => {
  it("puts a plain window in its tool's default group", () => {
    // Not `codex.weekly`: a bucket with no group of its own belongs to "All",
    // which is what the native app files it under.
    expect(groupKeyFor("codex", "weekly")).toBe("codex.all-models");
    expect(groupKeyFor("claude", "five_hour")).toBe("claude.all-models");
  });

  it("gives a named group its own key", () => {
    expect(groupKeyFor("codex", "gpt_5_3_codex_spark_weekly")).toBe("codex.spark");
    expect(groupKeyFor("claude", "weekly_fable")).toBe("claude.fable");
  });

  /// The rule a static table cannot express, and the one an invented
  /// predicate got wrong: a bucket discovered at runtime is its own group
  /// exactly when it arrived carrying a group title.
  it("treats a discovered bucket as its own group only when it had a title", () => {
    expect(groupKeyFor("codex", "gpt_reserve_weekly", "GPT-reserve")).toBe(
      "codex.gpt_reserve",
    );
    expect(groupKeyFor("codex", "gpt_reserve_weekly")).toBe("codex.all-models");
  });

  it("folds both windows of one runtime group together", () => {
    expect(groupKeyFor("codex", "gpt_reserve_five_hour", "GPT-reserve")).toBe(
      groupKeyFor("codex", "gpt_reserve_weekly", "GPT-reserve"),
    );
  });

  /// Order is load-bearing and cannot be recovered from the tables: a bucket
  /// naming Flash Lite also contains "flash".
  it("tests the longer pattern first", () => {
    expect(groupKeyFor("gemini", "flash-lite_weekly", "Flash Lite")).toBe(
      "gemini.flash-lite",
    );
    expect(groupKeyFor("gemini", "flash_weekly", "Flash")).toBe("gemini.flash");
  });

  it("treats every AntiGravity bucket as its own group", () => {
    // Its four lanes are split across two groups, so none of them belongs to
    // a default the way a plain weekly does.
    expect(groupKeyFor("antigravity", "gemini_five_hour")).toBe(
      "antigravity.gemini-models",
    );
    expect(groupKeyFor("antigravity", "claude_gpt_weekly")).toBe(
      "antigravity.claude-gpt-models",
    );
  });

  it("leaves Grok Bot ungrouped", () => {
    expect(groupKeyFor("cursor", "grok_bot_weekly")).toBeNull();
  });
});

describe("what a group is called", () => {
  it("shortens a known group", () => {
    expect(groupLabelFor("codex", "gpt_5_3_codex_spark_weekly", "GPT-5.3 Codex Spark"))
      .toBe("Spark");
    expect(groupLabelFor("antigravity", "claude_gpt_weekly", "Claude + GPT Models"))
      .toBe("Claude + GPT");
  });

  it("falls back to the provider's own title for a group it does not know", () => {
    expect(groupLabelFor("codex", "gpt_reserve_weekly", "GPT-reserve")).toBe(
      "GPT-reserve",
    );
  });

  it("says nothing for a bucket that has no group", () => {
    expect(groupLabelFor("cursor", "grok_bot_weekly", "Grok Bot")).toBeNull();
  });
});

describe("the SubProvider a bucket belongs to", () => {
  it("is normally the account's", () => {
    expect(subProviderFor("claude", "weekly")).toBe("Claude");
    expect(companyFor("claude")).toBe("Anthropic");
  });

  /// The one bucket that belongs to a SubProvider its account does not.
  /// Resolving by tool alone filed it under "Cursor".
  it("is Grok Bot for the bucket Cursor reports on its behalf", () => {
    expect(subProviderFor("cursor", "grok_bot_weekly")).toBe("Grok Bot");
    expect(subProviderFor("cursor", "models")).toBe("Cursor");
  });
});

describe("how a bucket reads in a flat list", () => {
  it("prefers the adapter's short label", () => {
    expect(
      bucketLabelFor("codex", "gpt_5_3_codex_spark_weekly", "Weekly", "Spark Weekly",
        "GPT-5.3 Codex Spark"),
    ).toBe("Spark Weekly");
  });

  /// The heading above it already says the name; a third repetition is noise.
  it("names only the window when the group is its SubProvider", () => {
    expect(bucketLabelFor("cursor", "grok_bot_weekly", "Weekly", "Grok Bot", "Grok Bot"))
      .toBe("Weekly");
  });

  it("keeps a group that is not just the SubProvider", () => {
    expect(bucketLabelFor("cursor", "models", "Monthly", "Cursor", "Cursor Models"))
      .toBe("Cursor");
  });

  it("composes from the contract when the adapter offered nothing", () => {
    expect(bucketLabelFor("claude", "weekly_fable", "Weekly", undefined, "Fable", " · "))
      .toBe("Fable · Weekly");
  });
});

describe("the tables themselves", () => {
  it("names a company and a SubProvider for every tool it lists", () => {
    for (const [tool, entry] of Object.entries(HIERARCHY)) {
      expect(entry.company, tool).toBeTruthy();
      expect(entry.subProvider, tool).toBeTruthy();
    }
  });

  it("falls back to the tool's own name rather than showing nothing", () => {
    expect(companyFor("a-provider-this-build-has-never-heard-of")).toBe(
      "a-provider-this-build-has-never-heard-of",
    );
  });

  it("has a label for every group the static rules can produce", () => {
    // Rule outputs without a label take the provider's own group title, which
    // is fine — but a *statically listed* group with no label would draw a
    // blank heading.
    for (const key of ["codex.spark", "claude.fable", "cursor.models"]) {
      expect(GROUP_LABELS[key], key).toBeTruthy();
    }
  });
});
