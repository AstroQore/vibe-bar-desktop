import { describe, expect, it } from "vitest";

import type { AccountQuota, QuotaView } from "../api";
import { activeCompany, showsProviderDetail, visibleCompanies } from "./ProviderTabs";

const NOW = 1_800_000_000;

function account(tool: string): AccountQuota {
  return {
    accountId: `acct-${tool}`,
    tool,
    queriedAt: NOW - 60,
    origin: "live",
    buckets: [
      {
        id: "weekly",
        title: "Weekly",
        shortLabel: "Weekly",
        usedPercent: 40,
        resetAt: NOW + 86_400,
        rawWindowSeconds: 604_800,
      },
    ],
  } as AccountQuota;
}

function view(...tools: string[]): QuotaView {
  return {
    accounts: tools.map(account),
    generatedAt: NOW,
    hasSharedData: true,
  } as unknown as QuotaView;
}

describe("which provider pages exist", () => {
  it("lists a company once however many of its tools are signed in", () => {
    const companies = visibleCompanies(view("gemini", "antigravity"), null);
    expect(companies.map((c) => c.name)).toEqual(["Google AI"]);
  });

  /// The row follows the same order the cards below it do — which is the one
  /// `orderedVisibleAccounts` decides, not the order the accounts arrived in.
  it("keeps different companies apart, in the order the cards use", () => {
    const listed = visibleCompanies(view("claude", "codex"), null).map((c) => c.name);
    const reversed = visibleCompanies(view("codex", "claude"), null).map((c) => c.name);
    expect(listed).toEqual(reversed);
    expect(new Set(listed)).toEqual(new Set(["OpenAI", "Anthropic"]));
  });

  it("has nothing to show when no account does", () => {
    expect(visibleCompanies(view(), null)).toEqual([]);
  });
});

describe("which page is actually open", () => {
  /// A selection outlives the thing it selected: a refresh can drop the
  /// company that was open, and the row hides itself below two companies, so
  /// a stale string would filter the list to nothing with no way back.
  it("falls back to the overview when the chosen company is gone", () => {
    const companies = visibleCompanies(view("claude"), null);
    expect(activeCompany(companies, "OpenAI")).toBe("");
  });

  it("keeps a choice that is still there", () => {
    const companies = visibleCompanies(view("claude", "codex"), null);
    expect(activeCompany(companies, "Anthropic")).toBe("Anthropic");
  });

  it("treats no choice as the overview", () => {
    expect(activeCompany(visibleCompanies(view("claude"), null), "")).toBe("");
  });
});

describe("when a provider's own detail is shown", () => {
  it("shows it on a chosen provider's page", () => {
    const companies = visibleCompanies(view("claude", "codex"), null);
    expect(showsProviderDetail(companies, "Anthropic")).toBe(true);
    expect(showsProviderDetail(companies, "")).toBe(false);
  });

  /// With one company the overview *is* that provider's page. Hiding the row
  /// there — which is right, since one option is not a choice — also made the
  /// selection unreachable, so the detail had nowhere left to appear.
  it("shows it on the overview when there is only one company", () => {
    const companies = visibleCompanies(view("claude"), null);
    expect(companies).toHaveLength(1);
    expect(showsProviderDetail(companies, "")).toBe(true);
  });

  it("shows nothing to detail when there is no quota at all", () => {
    expect(showsProviderDetail(visibleCompanies(view(), null), "")).toBe(false);
  });

  it("is not fooled by a selection that no longer exists", () => {
    const companies = visibleCompanies(view("claude", "codex"), null);
    expect(showsProviderDetail(companies, "SpaceXAI")).toBe(false);
  });
});
