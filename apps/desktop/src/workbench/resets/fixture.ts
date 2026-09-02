/** Calendar history for `/preview.html?surface=resets`: a few completed
 *  cycles this month, relative to the shared fixture clock. */
import type { QuotaCycle } from "../../api";
import { FIXTURE_NOW, FIXTURE_VIEW } from "../../popover/fixture";

const DAY = 86_400;
function cycle(end: number, used: number, interval: number): QuotaCycle {
  return {
    windowEnd: end,
    windowStart: end - interval,
    peakUsedPercent: used,
    lastUsedPercent: used,
    observationCount: 9,
    firstSeenAt: end - interval,
    lastSeenAt: end - 300,
    completion: "scheduledReset",
    resetKind: "scheduled",
    intervalSeconds: interval,
  };
}

export const FIXTURE_RESET_HISTORY: Record<string, QuotaCycle[]> = Object.fromEntries(
  FIXTURE_VIEW.accounts.flatMap((account, ai) =>
    account.buckets.map((bucket, bi) => {
      const interval = bucket.rawWindowSeconds ?? 7 * DAY;
      const ends = interval >= DAY ? [FIXTURE_NOW - (2 + ai) * DAY, FIXTURE_NOW - (9 + ai + bi) * DAY] : [FIXTURE_NOW - 3 * 3_600 * (bi + 1), FIXTURE_NOW - 14 * 3_600];
      return [`${account.accountId}:${bucket.id}`, ends.map((end, i) => cycle(end, 40 + ((ai * 17 + bi * 23 + i * 11) % 55), interval))];
    }),
  ),
);
