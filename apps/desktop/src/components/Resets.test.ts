import { describe, expect, it } from "vitest";

import type { AccountQuota, QuotaView } from "../api";
import { collectResetEvents } from "./Resets";

const NOW = 1_800_000_000;

function account(overrides: Partial<AccountQuota> = {}): AccountQuota {
  return {
    accountId: "acct",
    tool: "claude",
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
    ...overrides,
  } as AccountQuota;
}

function view(...accounts: AccountQuota[]): QuotaView {
  return { accounts, generatedAt: NOW, hasSharedData: true } as unknown as QuotaView;
}

describe("which resets are worth listing", () => {
  it("separates what is coming from what has passed", () => {
    const passed = account({
      accountId: "b",
      buckets: [{ ...account().buckets[0], resetAt: NOW - 86_400 }],
    });
    const events = collectResetEvents(view(account(), passed), null, NOW);
    expect(events.upcoming).toHaveLength(1);
    expect(events.expired).toHaveLength(1);
  });

  /// A reset a minute ago has almost certainly happened and simply not been
  /// observed yet, which is a different thing from one that passed days ago
  /// and nobody noticed. It joins the passed list, but says which it is.
  it("calls a just-passed reset due rather than expired", () => {
    const events = collectResetEvents(
      view(account({ buckets: [{ ...account().buckets[0], resetAt: NOW - 60 }] })),
      null,
      NOW,
    );
    expect(events.upcoming).toHaveLength(0);
    expect(events.expired[0]?.state).toBe("due");
  });

  it("stops calling it due once the grace period is over", () => {
    const events = collectResetEvents(
      view(account({ buckets: [{ ...account().buckets[0], resetAt: NOW - 3_600 }] })),
      null,
      NOW,
    );
    expect(events.expired[0]?.state).toBe("expired");
  });

  it("counts a bucket with no reset time instead of inventing one", () => {
    const events = collectResetEvents(
      view(account({ buckets: [{ ...account().buckets[0], resetAt: undefined }] })),
      null,
      NOW,
    );
    expect(events.missing).toBe(1);
    expect(events.upcoming).toHaveLength(0);
  });

  it("counts an unusable reset time separately from a missing one", () => {
    const events = collectResetEvents(
      view(account({ buckets: [{ ...account().buckets[0], resetAt: Number.NaN }] })),
      null,
      NOW,
    );
    expect(events.invalid).toBe(1);
    expect(events.missing).toBe(0);
  });

  /// A shared cache accumulates entries from every client that ever ran, and
  /// nothing prunes an account signed out of. A real data root held one
  /// stamped five months in the future.
  it("refuses an account observed in the future rather than trusting it", () => {
    const events = collectResetEvents(
      view(account({ queriedAt: NOW + 90 * 86_400 })),
      null,
      NOW,
    );
    expect(events.futureDated).toBe(1);
    expect(events.upcoming).toHaveLength(0);
  });

  it("tolerates a clock a couple of minutes ahead", () => {
    const events = collectResetEvents(view(account({ queriedAt: NOW + 120 })), null, NOW);
    expect(events.futureDated).toBe(0);
    expect(events.upcoming).toHaveLength(1);
  });
});

describe("the order they are read in", () => {
  it("puts the soonest reset first, and the most recent expiry first", () => {
    const soon = account({
      accountId: "a",
      buckets: [{ ...account().buckets[0], id: "five_hour", resetAt: NOW + 3_600 }],
    });
    const later = account({ accountId: "b" });
    const longGone = account({
      accountId: "c",
      buckets: [{ ...account().buckets[0], id: "monthly", resetAt: NOW - 5 * 86_400 }],
    });
    const justGone = account({
      accountId: "d",
      buckets: [{ ...account().buckets[0], id: "daily", resetAt: NOW - 2 * 86_400 }],
    });

    const events = collectResetEvents(view(later, soon, justGone, longGone), null, NOW);
    expect(events.upcoming.map((e) => e.resetAt)).toEqual([NOW + 3_600, NOW + 86_400]);
    expect(events.expired.map((e) => e.resetAt)).toEqual([
      NOW - 2 * 86_400,
      NOW - 5 * 86_400,
    ]);
  });
});
