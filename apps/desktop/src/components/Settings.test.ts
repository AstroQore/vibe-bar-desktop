import { describe, expect, it } from "vitest";

import { replacedSummary } from "./Settings";
import { humanisedSettingName } from "../settingNames";

/// The sentence the user reads when a choice they made here has been taken
/// over. It has to match the native app's, which says the same thing about
/// the same file.
describe("the notice under a replaced setting", () => {
  it("names one setting in the singular", () => {
    expect(replacedSummary(["refreshIntervalSeconds"])).toBe(
      "Refresh interval seconds now holds the other copy's value.",
    );
  });

  it("names a small handful in full", () => {
    expect(replacedSummary(["displayMode", "menuBarColorBasis"])).toBe(
      "Display mode, Menu bar color basis now hold the other copy's value.",
    );
  });

  it("counts the rest once there are too many to read", () => {
    expect(replacedSummary(["a", "b", "c", "d", "e"])).toMatch(/^A, B, C and 2 more settings/);
  });

  it("says nothing when nothing was replaced", () => {
    expect(replacedSummary([])).toBe("");
  });
});

describe("naming a setting by its key", () => {
  it("splits the words a key runs together", () => {
    expect(humanisedSettingName("refreshOnPopoverOpen")).toBe("Refresh on popover open");
  });

  it("leaves a single word alone but for its capital", () => {
    expect(humanisedSettingName("displayMode")).toBe("Display mode");
  });

  it("gives back an empty key unchanged", () => {
    expect(humanisedSettingName("")).toBe("");
  });
});
