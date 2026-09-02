/**
 * What the Overview shows, derived from the views the shell already serves.
 *
 * Nothing here draws. It turns `QuotaView` / `CostView` / `ServiceStatusView`
 * into the cards the native `PageModuleCatalog.overviewDescriptors` produces,
 * in the same order, gated by the same facts — so the waterfall's inputs are
 * decided by one function that a test can hold against native's order.
 */
import type { AccountQuota, CostView, PresentationSettings, ProviderStatus, QuotaBucket, QuotaView } from "../api";
import { companyFor, subProviderFor } from "../naming";
import { orderedVisibleAccounts } from "../components/Overview";
import type { Phase } from "./masonry";

/** Native `OverviewPage` tabs: the company each popover page is about. */
export const CORE_PAGES: { id: string; label: string; tool: string; company: string }[] = [
  { id: "openAI", label: "OpenAI", tool: "codex", company: "OpenAI" },
  { id: "claude", label: "Anthropic", tool: "claude", company: "Anthropic" },
  { id: "googleAI", label: "Google AI", tool: "gemini", company: "Google AI" },
  { id: "grok", label: "SpaceXAI", tool: "grok", company: "SpaceXAI" },
];

export type DescriptorKind =
  | { kind: "costSummary" }
  | { kind: "statusSummary" }
  | { kind: "quota"; company: string; tool: string }
  | { kind: "upcomingResets" }
  | { kind: "usageMix" };

export interface Descriptor {
  id: string;
  phase: Phase;
  kind: DescriptorKind;
}

/**
 * Native's declaration order: cost summary, status summary, upcoming resets,
 * usage mix (when there is cost data), then one quota card per visible core
 * company. Cards the core cannot feed yet (quota history, cost cards, model
 * ranking, heatmaps) are left out rather than drawn empty.
 */
export function overviewDescriptors(view: QuotaView, settings: PresentationSettings | null, cost: CostView | null): Descriptor[] {
  const out: Descriptor[] = [
    { id: "overview-summary-cost", phase: "summary", kind: { kind: "costSummary" } },
    { id: "overview-summary-status", phase: "summary", kind: { kind: "statusSummary" } },
    { id: "overview-upcoming-resets", phase: "auxiliary", kind: { kind: "upcomingResets" } },
  ];
  if (hasCostData(cost)) out.push({ id: "overview-usage-mix", phase: "auxiliary", kind: { kind: "usageMix" } });
  for (const page of visibleCorePages(view, settings)) {
    out.push({ id: `overview-quota:${page.tool}`, phase: "quota", kind: { kind: "quota", company: page.company, tool: page.tool } });
  }
  return out;
}

export function hasCostData(cost: CostView | null): boolean {
  return !!cost && (cost.allTime.tokens > 0 || cost.allTime.requests > 0);
}

/** The core pages that have a visible account, in native's tab order. */
export function visibleCorePages(view: QuotaView, settings: PresentationSettings | null) {
  const companies = new Set(orderedVisibleAccounts(view.accounts, settings).map((a) => companyFor(a.tool)));
  return CORE_PAGES.filter((page) => companies.has(page.company));
}

export interface SubProviderSection {
  subProvider: string;
  tool: string;
  /** One block per account that contributes buckets to this SubProvider. */
  blocks: { account: AccountQuota; buckets: QuotaBucket[] }[];
}

/**
 * A company's visible accounts, partitioned by SubProvider **per bucket**,
 * in first-seen order. Per bucket, not per account: the naming contract
 * files Cursor's `grok_bot_weekly` under Grok Bot, so one account can feed
 * two sections. An account with no buckets still gets a section, so its
 * error or empty state has somewhere to be said.
 */
export function companySections(view: QuotaView, settings: PresentationSettings | null, company: string): SubProviderSection[] {
  const sections: SubProviderSection[] = [];
  const section = (name: string, tool: string) => {
    let found = sections.find((s) => s.subProvider === name);
    if (!found) {
      found = { subProvider: name, tool, blocks: [] };
      sections.push(found);
    }
    return found;
  };
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (companyFor(account.tool) !== company) continue;
    if (account.buckets.length === 0) {
      section(subProviderFor(account.tool), account.tool).blocks.push({ account, buckets: [] });
      continue;
    }
    const byName = new Map<string, QuotaBucket[]>();
    for (const bucket of account.buckets) {
      const name = subProviderFor(account.tool, bucket.id);
      byName.set(name, [...(byName.get(name) ?? []), bucket]);
    }
    for (const [name, buckets] of byName) section(name, account.tool).blocks.push({ account, buckets });
  }
  return sections;
}

/** Native `bucketContent`: ungrouped buckets first, then each group under its caption. */
export function bucketGroups(buckets: QuotaBucket[]): { title: string | null; buckets: QuotaBucket[] }[] {
  const primary = buckets.filter((b) => !b.groupTitle);
  const groups: { title: string | null; buckets: QuotaBucket[] }[] = primary.length ? [{ title: null, buckets: primary }] : [];
  for (const bucket of buckets) {
    if (!bucket.groupTitle) continue;
    let group = groups.find((g) => g.title === bucket.groupTitle);
    if (!group) {
      group = { title: bucket.groupTitle, buckets: [] };
      groups.push(group);
    }
    group.buckets.push(bucket);
  }
  return groups;
}

// ── Cost summary ───────────────────────────────────────────────────────────

export interface CostSummary {
  totalCost: number; totalTokens: number; peakDayCost: number; peakDayTokens: number;
  todayCost: number; yesterdayCost: number; last7Cost: number; last30Cost: number;
  todayTokens: number; yesterdayTokens: number; last7Tokens: number; last30Tokens: number;
}

const micros = (m: number) => m / 1_000_000;

/**
 * The twelve numbers on the Cost card. Native reads them off its ledger;
 * here they are folded from the per-day rows, which is the same arithmetic
 * one step later. Days are the scanner's local calendar days.
 */
export function costSummary(cost: CostView | null, now: number): CostSummary | null {
  if (!cost) return null;
  const byDay = new Map<string, { cost: number; tokens: number }>();
  for (const row of cost.daily) {
    const cur = byDay.get(row.day) ?? { cost: 0, tokens: 0 };
    cur.cost += micros(row.pricedCostMicros);
    cur.tokens += row.tokens;
    byDay.set(row.day, cur);
  }
  const dayKey = (t: number) => {
    const d = new Date(t * 1000);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };
  const yesterday = byDay.get(dayKey(now - 86_400)) ?? { cost: 0, tokens: 0 };
  let peakCost = 0, peakTokens = 0;
  for (const v of byDay.values()) { peakCost = Math.max(peakCost, v.cost); peakTokens = Math.max(peakTokens, v.tokens); }
  return {
    totalCost: micros(cost.allTime.pricedCostMicros), totalTokens: cost.allTime.tokens,
    peakDayCost: peakCost, peakDayTokens: peakTokens,
    todayCost: micros(cost.today.pricedCostMicros), yesterdayCost: yesterday.cost,
    last7Cost: micros(cost.last7Days.pricedCostMicros), last30Cost: micros(cost.last30Days.pricedCostMicros),
    todayTokens: cost.today.tokens, yesterdayTokens: yesterday.tokens,
    last7Tokens: cost.last7Days.tokens, last30Tokens: cost.last30Days.tokens,
  };
}

// ── Usage mix ──────────────────────────────────────────────────────────────

export type MixDimension = "projects" | "harnesses" | "models" | "flow";
export const MIX_DIMENSIONS: { id: MixDimension; title: string }[] = [
  { id: "projects", title: "Projects" },
  { id: "harnesses", title: "Harnesses" },
  { id: "models", title: "Models" },
  { id: "flow", title: "Token Flow" },
];

export interface MixSlice { id: string; label: string; detail: string | null; tokens: number }

/** Top four by tokens plus "Other", as native folds them. */
export function usageMix(cost: CostView | null, dimension: MixDimension): { slices: MixSlice[]; total: number; empty: string | null } {
  if (!cost) return { slices: [], total: 0, empty: "No local usage in this range." };
  let rows: MixSlice[] = [];
  if (dimension === "harnesses") {
    const by = new Map<string, number>();
    for (const m of cost.models) by.set(m.harness, (by.get(m.harness) ?? 0) + m.tokens);
    rows = [...by].map(([label, tokens]) => ({ id: label, label, detail: null, tokens }));
  } else if (dimension === "models") {
    rows = cost.models.map((m) => ({ id: `${m.harness}:${m.model}`, label: m.model, detail: m.harness, tokens: m.tokens }));
  } else if (dimension === "projects") {
    return { slices: [], total: 0, empty: "Project attribution appears after the next Codex or Claude cost refresh." };
  } else {
    return { slices: [], total: 0, empty: "Token flow needs the per-request ledger, which this client does not keep yet." };
  }
  rows = rows.filter((r) => r.tokens > 0).sort((a, b) => b.tokens - a.tokens);
  const total = rows.reduce((s, r) => s + r.tokens, 0);
  if (total === 0) return { slices: [], total: 0, empty: "No local usage in this range." };
  const head = rows.slice(0, 4);
  const rest = rows.slice(4).reduce((s, r) => s + r.tokens, 0);
  if (rest > 0) head.push({ id: "other", label: "Other", detail: null, tokens: rest });
  return { slices: head, total, empty: null };
}

// ── Upcoming resets ────────────────────────────────────────────────────────

export interface ResetEvent { id: string; label: string; remainingPercent: number; gainPercent: number; resetAt: number }

/**
 * Native `UpcomingResetsCard.events`: the visible core providers' buckets
 * resetting within seven days, soonest first. The label is SubProvider,
 * then the group unless it merely repeats the SubProvider, then the bucket.
 */
export function upcomingResets(view: QuotaView, settings: PresentationSettings | null, now: number): ResetEvent[] {
  const horizon = now + 7 * 86_400;
  const core = new Set(CORE_PAGES.map((p) => p.company));
  const out: ResetEvent[] = [];
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (!core.has(companyFor(account.tool))) continue;
    for (const bucket of account.buckets) {
      if (!bucket.resetAt || bucket.resetAt <= now || bucket.resetAt > horizon) continue;
      const sub = subProviderFor(account.tool, bucket.id);
      const group = bucket.groupTitle?.trim();
      const parts = [sub];
      if (group && group.toLowerCase() !== sub.toLowerCase()) parts.push(group);
      parts.push(bucket.title);
      out.push({
        id: `${account.accountId}:${bucket.id}`,
        label: parts.join(" · "),
        remainingPercent: Math.max(0, 100 - bucket.usedPercent),
        gainPercent: bucket.usedPercent,
        resetAt: bucket.resetAt,
      });
    }
  }
  return out.sort((a, b) => a.resetAt - b.resetAt);
}

// ── Service status ─────────────────────────────────────────────────────────

export type StatusState = "up" | "degraded" | "down" | "checking" | "maintenance";

export const STATUS_LABEL: Record<StatusState, string> = { up: "Up", degraded: "Degraded", down: "Down", checking: "Checking", maintenance: "Maintenance" };
export const STATUS_DETAIL: Record<StatusState, string> = { up: "Operational", degraded: "Partial outage", down: "Needs attention", checking: "Checking", maintenance: "Maintenance" };

/** Native `statusState`: Statuspage indicators folded to five states. */
export function statusState(provider: ProviderStatus | undefined, refreshing: boolean): StatusState {
  // No feed for this company yet reads as checking, as native's nil snapshot
  // does — "down" is what a feed *says*, not what its absence implies.
  if (!provider) return "checking";
  switch (provider.indicator) {
    case "none": return "up";
    case "maintenance": return "maintenance";
    case "minor": case "major": return "degraded";
    case "critical": return "down";
    default: return refreshing ? "checking" : "up";
  }
}

/** Native `statusTitle`: the company the page is named after. */
export function statusTitle(tool: string): string {
  return companyFor(tool);
}

export function statusDetail(provider: ProviderStatus | undefined, state: StatusState): string {
  const text = provider?.description?.trim();
  return text && text.length > 0 ? text : STATUS_DETAIL[state];
}
