/**
 * The Resets page's pure logic, after the native `ResetsPage`: which resets
 * are worth listing, the per-SubProvider cycle cards, the run-out risk
 * ranking, the calendar's entries, and the one-line forecast reading.
 */
import type { PresentationSettings, QuotaBucket, QuotaCycle, QuotaForecast, QuotaView } from "../../api";
import { companyFor, subProviderFor, bucketLabelFor } from "../../naming";
import { orderedVisibleAccounts } from "../../components/Overview";
import { CORE_PAGES } from "../../popover/data";
import { countdown } from "../../popover/format";

const CLOCK_SKEW_SECONDS = 300;
const RESET_GRACE_SECONDS = 180;

interface ResetEvent {
  id: string;
  vendor: string;
  product: string;
  bucket: string;
  plan?: string;
  resetAt: number;
  used: number;
  remaining: number;
  state: "upcoming" | "due" | "expired";
  forecast?: QuotaForecast;
}

export function collectResetEvents(
  view: QuotaView,
  settings: PresentationSettings | null,
  now = Date.now() / 1000,
) {
  const upcoming: ResetEvent[] = [];
  const expired: ResetEvent[] = [];
  let missing = 0;
  let invalid = 0;
  let futureDated = 0;

  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (!Number.isFinite(account.queriedAt) || account.queriedAt > now + CLOCK_SKEW_SECONDS) {
      futureDated += account.buckets.length;
      continue;
    }
    const vendor = companyFor(account.tool);
    for (const bucket of account.buckets) {
      if (bucket.resetAt === undefined) {
        missing += 1;
        continue;
      }
      const resetDate = new Date(bucket.resetAt * 1000);
      if (!Number.isFinite(bucket.resetAt) || bucket.resetAt <= 0 || !Number.isFinite(resetDate.valueOf())) {
        invalid += 1;
        continue;
      }
      const used = Number.isFinite(bucket.usedPercent)
        ? Math.min(100, Math.max(0, bucket.usedPercent))
        : 0;
      const delta = bucket.resetAt - now;
      const event: ResetEvent = {
        id: `${account.accountId}/${bucket.id}/${bucket.resetAt}`,
        vendor,
        // Per bucket, not per account: Cursor reports Grok Bot, which belongs
        // to a SubProvider its account does not.
        product: subProviderFor(account.tool, bucket.id),
        bucket: bucketLabelFor(
          account.tool,
          bucket.id,
          bucket.title,
          bucket.shortLabel,
          bucket.groupTitle,
          " · ",
        ),
        plan: settings?.providerPlanLabels?.[account.tool] ?? account.plan,
        resetAt: bucket.resetAt,
        forecast: bucket.forecast,
        used,
        remaining: 100 - used,
        state: delta > 0 ? "upcoming" : delta >= -RESET_GRACE_SECONDS ? "due" : "expired",
      };
      (event.state === "upcoming" ? upcoming : expired).push(event);
    }
  }

  const tieBreak = (left: ResetEvent, right: ResetEvent) =>
    left.vendor.localeCompare(right.vendor) ||
    left.product.localeCompare(right.product) ||
    left.bucket.localeCompare(right.bucket);
  upcoming.sort((left, right) => left.resetAt - right.resetAt || tieBreak(left, right));
  expired.sort((left, right) => right.resetAt - left.resetAt || tieBreak(left, right));
  return { upcoming, expired, missing, invalid, futureDated };
}

export interface SubProviderCycle {
  id: string;
  tool: string;
  accountId: string;
  accountLabel: string | null;
  subProviderName: string;
  groupTitle: string | null;
  plan: string | null;
  buckets: QuotaBucket[];
  /** The longest window in the group: the cycle the card is about. */
  headline: QuotaBucket;
  forecast: QuotaForecast | null;
  name: string;
}

/** The native `subProviderCycles`: every visible core account's buckets,
 *  grouped by SubProvider and group title in first-seen order, headlined
 *  by the longest window. */
export function subProviderCycles(view: QuotaView, settings: PresentationSettings | null, now: number): SubProviderCycle[] {
  const core = new Set(CORE_PAGES.map((p) => p.company));
  const accounts = orderedVisibleAccounts(view.accounts, settings).filter((a) => core.has(companyFor(a.tool)));
  const byTool = new Map<string, number>();
  for (const account of accounts) byTool.set(account.tool, (byTool.get(account.tool) ?? 0) + 1);
  const out: SubProviderCycle[] = [];
  for (const account of accounts) {
    if (!Number.isFinite(account.queriedAt) || account.queriedAt > now + CLOCK_SKEW_SECONDS) continue;
    const accountLabel = (byTool.get(account.tool) ?? 0) > 1 ? account.accountId : null;
    const order: string[] = [];
    const groups = new Map<string, { sub: string; group: string | null; buckets: QuotaBucket[] }>();
    for (const bucket of account.buckets) {
      const sub = subProviderFor(account.tool, bucket.id);
      const trimmed = bucket.groupTitle?.trim();
      const group = trimmed && trimmed.length > 0 ? trimmed : null;
      const key = `${sub}/${group ?? ""}`;
      if (!groups.has(key)) {
        order.push(key);
        groups.set(key, { sub, group, buckets: [] });
      }
      groups.get(key)!.buckets.push(bucket);
    }
    for (const key of order) {
      const entry = groups.get(key)!;
      const headline = [...entry.buckets].sort((a, b) => (b.rawWindowSeconds ?? 0) - (a.rawWindowSeconds ?? 0))[0];
      if (!headline) continue;
      const name = entry.group ? `${entry.sub} · ${entry.group}` : entry.sub;
      out.push({
        id: `${account.accountId}/${entry.sub}/${entry.group ?? ""}`,
        tool: account.tool,
        accountId: account.accountId,
        accountLabel,
        subProviderName: entry.sub,
        groupTitle: entry.group,
        plan: settings?.providerPlanLabels?.[account.tool] ?? account.plan ?? null,
        buckets: entry.buckets,
        headline,
        forecast: headline.forecast ?? null,
        name,
      });
    }
  }
  return out;
}

export function remainingOf(bucket: QuotaBucket): number {
  return Math.max(0, 100 - (Number.isFinite(bucket.usedPercent) ? bucket.usedPercent : 0));
}

/** The native `miniForecastLine`: the forecast in a few words. */
export function miniForecastLine(forecast: QuotaForecast, now: number): string {
  if (forecast.runOutAt != null) {
    const when = countdown(forecast.runOutAt, now);
    if (when) return forecast.verdict === "watch" ? `may run out ${when}` : `out ${when}`;
  }
  const left = Math.round(100 - forecast.projectedUsedPercent);
  switch (forecast.verdict) {
    case "enough":
      return `${left}% left`;
    case "surplus":
      return `surplus · ${left}% left`;
    case "watch":
      return "watch";
    case "atRisk":
      return "risk";
    case "learning":
      return `learning · ${left}% left`;
  }
}

export interface RiskRow {
  cycle: SubProviderCycle;
  bucket: QuotaBucket;
  forecast: QuotaForecast | null;
  remaining: number;
  badge: "OUT" | "RISK" | "WATCH" | "LOW";
}

/** The native `riskList`: buckets whose forecast is uneasy or that sit at
 *  15% or less, soonest run-out first, then the least remaining. */
export function riskRows(cycles: SubProviderCycle[]): RiskRow[] {
  const rows: RiskRow[] = [];
  for (const cycle of cycles) {
    for (const bucket of cycle.buckets) {
      const forecast = bucket.id === cycle.headline.id ? cycle.forecast : (bucket.forecast ?? null);
      const remaining = remainingOf(bucket);
      const uneasy = forecast ? forecast.verdict === "watch" || forecast.verdict === "atRisk" : false;
      if (uneasy || remaining <= 15) rows.push({ cycle, bucket, forecast, remaining, badge: riskBadge(remaining, forecast) });
    }
  }
  return rows.sort((a, b) => {
    const aOut = a.forecast?.runOutAt ?? Number.POSITIVE_INFINITY;
    const bOut = b.forecast?.runOutAt ?? Number.POSITIVE_INFINITY;
    if (aOut !== bOut) return aOut - bOut;
    return a.remaining - b.remaining;
  });
}

export function riskBadge(remaining: number, forecast: QuotaForecast | null): RiskRow["badge"] {
  if (remaining <= 1) return "OUT";
  switch (forecast?.verdict) {
    case "atRisk":
      return "RISK";
    case "watch":
      return "WATCH";
    default:
      return "LOW";
  }
}

export interface CalendarEntry {
  id: string;
  at: number;
  label: string;
  shortLabel: string;
  gainPercent: number;
  kind: "past" | "next";
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTHS_LONG = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

export function entryTime(at: number): string {
  const d = new Date(at * 1000);
  const pad = (v: number) => (v < 10 ? `0${v}` : `${v}`);
  return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function monthTitle(monthStart: number): string {
  const d = new Date(monthStart * 1000);
  return `${MONTHS_LONG[d.getMonth()]} ${d.getFullYear()}`;
}

export function monthStartOf(at: number, offsetMonths = 0): number {
  const d = new Date(at * 1000);
  return new Date(d.getFullYear(), d.getMonth() + offsetMonths, 1).getTime() / 1000;
}

export function dayStartOf(at: number): number {
  const d = new Date(at * 1000);
  d.setHours(0, 0, 0, 0);
  return d.getTime() / 1000;
}

/** The native `calendarEntries`: completed cycles landing in the month
 *  (from the history the forecast keeps) and every scheduled reset in it. */
export function calendarEntries(
  view: QuotaView,
  settings: PresentationSettings | null,
  history: Record<string, QuotaCycle[]>,
  monthStart: number,
  monthEnd: number,
  now: number,
): CalendarEntry[] {
  const out: CalendarEntry[] = [];
  const core = new Set(CORE_PAGES.map((p) => p.company));
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (!core.has(companyFor(account.tool))) continue;
    for (const bucket of account.buckets) {
      const sub = subProviderFor(account.tool, bucket.id);
      const name = `${sub} · ${bucket.title}`;
      for (const cycle of history[historyKey(account.accountId, bucket)] ?? []) {
        const at = cycle.windowEnd;
        if (!(at >= monthStart && at < monthEnd && at <= now)) continue;
        out.push({
          id: `past.${account.accountId}.${bucket.id}.${at}`,
          at,
          label: `${name} — reset ${entryTime(at)} at ${Math.round(cycle.lastUsedPercent)}% used`,
          shortLabel: sub,
          gainPercent: cycle.lastUsedPercent,
          kind: "past",
        });
      }
      const resetAt = bucket.resetAt;
      if (resetAt && resetAt > now && resetAt >= monthStart && resetAt < monthEnd) {
        const used = Number.isFinite(bucket.usedPercent) ? Math.min(100, Math.max(0, bucket.usedPercent)) : 0;
        out.push({
          id: `next.${account.accountId}.${bucket.id}`,
          at: resetAt,
          label: `${name} — resets ${entryTime(resetAt)}, +${Math.round(used)}% comes back`,
          shortLabel: sub,
          gainPercent: used,
          kind: "next",
        });
      }
    }
  }
  return out.sort((a, b) => a.at - b.at);
}

/** History is recorded under the account whose read produced the bucket,
 *  which differs from the card's account on a merged card. */
export function historyKey(accountId: string, bucket: Pick<QuotaBucket, "id" | "sourceAccountId">): string {
  return `${bucket.sourceAccountId ?? accountId}:${bucket.id}`;
}

export interface MonthGrid {
  weekdays: string[];
  leadingBlanks: number;
  dayCount: number;
}

/** A Sunday-first month grid, as the en-US calendar lays it out. */
export function monthGrid(monthStart: number): MonthGrid {
  const d = new Date(monthStart * 1000);
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const dayCount = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
  return { weekdays: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"], leadingBlanks: first.getDay(), dayCount };
}

export function entriesByDay(entries: CalendarEntry[], monthStart: number): Map<number, CalendarEntry[]> {
  const byDay = new Map<number, CalendarEntry[]>();
  const month = new Date(monthStart * 1000).getMonth();
  for (const entry of entries) {
    const d = new Date(entry.at * 1000);
    if (d.getMonth() !== month) continue;
    const list = byDay.get(d.getDate()) ?? [];
    list.push(entry);
    byDay.set(d.getDate(), list);
  }
  return byDay;
}

/** The lane events for the Refill Horizon and the sub-daily lane. */
export interface LaneEvent {
  id: string;
  accountId: string;
  bucketId: string;
  label: string;
  remainingPercent: number;
  gainPercent: number;
  resetAt: number;
  rawWindowSeconds: number | null;
}

export function laneEvents(view: QuotaView, settings: PresentationSettings | null, now: number, horizonDays: number): LaneEvent[] {
  const horizon = now + horizonDays * 86_400;
  const core = new Set(CORE_PAGES.map((p) => p.company));
  const out: LaneEvent[] = [];
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (!core.has(companyFor(account.tool))) continue;
    for (const bucket of account.buckets) {
      if (!bucket.resetAt || bucket.resetAt <= now || bucket.resetAt > horizon) continue;
      out.push({
        id: `${account.accountId}:${bucket.id}`,
        accountId: account.accountId,
        bucketId: bucket.id,
        label: bucketLabelFor(account.tool, bucket.id, bucket.title, bucket.shortLabel, bucket.groupTitle, " · "),
        remainingPercent: remainingOf(bucket),
        gainPercent: Number.isFinite(bucket.usedPercent) ? Math.min(100, Math.max(0, bucket.usedPercent)) : 0,
        resetAt: bucket.resetAt,
        rawWindowSeconds: bucket.rawWindowSeconds ?? null,
      });
    }
  }
  return out.sort((a, b) => a.resetAt - b.resetAt);
}

/** Sub-daily quotas resetting within a day — the native `subDailyLane` filter. */
export function subDailyEvents(events: LaneEvent[]): LaneEvent[] {
  return events.filter((event) => (event.rawWindowSeconds ?? 86_400) < 86_400);
}

/** Native rounds `now` down to five minutes so the cards do not re-lay
 *  out every second. */
export function coarseNow(now: number): number {
  return Math.floor(now / 300) * 300;
}
