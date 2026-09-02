/**
 * The Overview's cards, each a port of the native view it is named after.
 *
 * One card recipe (`CardShell`): fill, hairline, no shadow. Density comes
 * from the caller as CSS variables set by `PopoverRoot`, never resolved in a
 * leaf, which is native's rule too.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { AccountQuota, CostView, PresentationSettings, QuotaBucket, QuotaView, ServiceStatusView } from "../api";
import { quotaBarColor } from "../api";
import { ToolBrandBadge, ToolBrandIcon } from "./brand";
import {
  ArrowClockwise, CheckCircle, ClockBadge, InfoCircle, RefreshCircle, WarningTriangle, Wrench, XOctagon,
} from "./icons";
import {
  bucketGroups, companySections, costSummary, statusDetail, statusState, statusTitle, upcomingResets, usageMix,
  MIX_DIMENSIONS, STATUS_LABEL, type MixDimension, type StatusState,
} from "./data";
import {
  countdown, forecastPrimaryText, forecastUseUpText, formatCost, formatTokens, paceColor, paceEtaText,
  paceStageSummary, planBadgeLabel, resetStatus, staleAfterSeconds, staleLabel, updatedAgo, usagePace, verdictColor, type DisplayMode,
} from "./format";
import { providerAccent } from "../tokens";
import { fannedOffsets } from "../components/MiniQuota";

// ── Shared chrome ──────────────────────────────────────────────────────────

export function CardShell({ children, className, style, pinnedHeight }: { children: ReactNode; className?: string; style?: React.CSSProperties; pinnedHeight?: number }) {
  return (
    <section
      className={`pv-card${className ? ` ${className}` : ""}`}
      style={pinnedHeight ? { ...style, height: pinnedHeight } : style}
    >
      {children}
    </section>
  );
}

/** Native `PlanBadgeView`: bold, accent at 0.18, capsule. */
export function PlanBadge({ text }: { text?: string | null }) {
  const label = text?.trim();
  if (!label) return null;
  return <span className="pv-plan">{label}</span>;
}

/** Native `BorderlessIconButton`: 22pt hit box, secondary colour. */
export function IconButton({ title, onClick, disabled, spinning, children }: { title: string; onClick?: () => void; disabled?: boolean; spinning?: boolean; children: ReactNode }) {
  return (
    <button type="button" className={`pv-iconbtn${spinning ? " spinning" : ""}`} title={title} aria-label={title} onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}

function CardTitle({ title, meta, onRefresh, refreshing, refreshHelp }: { title: string; meta?: string | null; onRefresh?: () => void; refreshing?: boolean; refreshHelp?: string }) {
  return (
    <div className="pv-card-title">
      <span className="pv-card-title-text">{title}</span>
      <span className="pv-spacer" />
      {meta ? <span className="pv-meta">{meta}</span> : null}
      {refreshing ? <span className="pv-progress" aria-label="Refreshing" /> : null}
      {onRefresh ? (
        <IconButton title={refreshHelp ?? "Refresh"} onClick={onRefresh} disabled={refreshing}>
          <ArrowClockwise size={11} />
        </IconButton>
      ) : null}
    </div>
  );
}

// ── Cost summary ───────────────────────────────────────────────────────────

export function CostSummaryCard({ cost, now, pinnedHeight, onRefresh, refreshing }: { cost: CostView | null; now: number; pinnedHeight: number; onRefresh?: () => void; refreshing?: boolean }) {
  const s = costSummary(cost, now);
  const metric = (label: string, value: string, highlight = false) => (
    <div className="pv-metric">
      <span className="pv-metric-label">{label}</span>
      <span className={`pv-metric-value${highlight ? " highlight" : ""}`}>{value}</span>
    </div>
  );
  const divider = <span className="pv-metric-divider" />;
  return (
    <CardShell className="pv-cost" pinnedHeight={pinnedHeight}>
      <CardTitle title="Cost" meta={updatedAgo(cost && cost.scannedAt > 0 ? cost.scannedAt : undefined, now)} onRefresh={onRefresh} refreshing={refreshing} refreshHelp="Refresh cost data" />
      <div className="pv-metrics">
        <div className="pv-metric-row">
          {metric("TOTAL COST", formatCost(s?.totalCost), true)}{divider}
          {metric("TOTAL TOK", formatTokens(s?.totalTokens), true)}{divider}
          {metric("PEAK DAY", formatCost(s?.peakDayCost))}{divider}
          {metric("PEAK TOK DAY", formatTokens(s?.peakDayTokens))}
        </div>
        <div className="pv-metric-row">
          {metric("TODAY", formatCost(s?.todayCost))}{divider}
          {metric("YESTERDAY", formatCost(s?.yesterdayCost))}{divider}
          {metric("7-DAY", formatCost(s?.last7Cost))}{divider}
          {metric("30-DAY", formatCost(s?.last30Cost))}
        </div>
        <div className="pv-metric-row">
          {metric("TODAY TOK", formatTokens(s?.todayTokens))}{divider}
          {metric("YESTERDAY TOK", formatTokens(s?.yesterdayTokens))}{divider}
          {metric("7-DAY TOK", formatTokens(s?.last7Tokens))}{divider}
          {metric("30-DAY TOK", formatTokens(s?.last30Tokens))}
        </div>
      </div>
    </CardShell>
  );
}

// ── Status summary ─────────────────────────────────────────────────────────

const STATE_ICON: Record<StatusState, (p: { size?: number }) => JSX.Element> = {
  up: CheckCircle, degraded: WarningTriangle, down: XOctagon, checking: RefreshCircle, maintenance: Wrench,
};

export function StatusSummaryCard({ status, tools, now, pinnedHeight, refreshing, onRefresh }: { status: ServiceStatusView | null; tools: string[]; now: number; pinnedHeight: number; refreshing: boolean; onRefresh?: () => void }) {
  const rows = Math.max(1, Math.ceil(tools.length / 2));
  return (
    <CardShell className="pv-status" pinnedHeight={pinnedHeight}>
      <CardTitle title="Status" meta={status?.updatedAt ? updatedAgo(status.updatedAt, now) : null} onRefresh={onRefresh} refreshing={refreshing} refreshHelp="Refresh service status" />
      {tools.length === 0 ? (
        <p className="pv-empty-centre">Enable a core provider in Settings to show service status.</p>
      ) : (
        <div className="pv-status-grid" style={{ gridTemplateColumns: `repeat(${Math.min(2, tools.length)}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${rows}, 1fr)` }}>
          {tools.map((tool) => {
            const provider = status?.providers.find((p) => p.tool === tool);
            const state = statusState(provider, refreshing);
            const Icon = STATE_ICON[state];
            return (
              <div key={tool} className={`pv-status-tile ${state}`}>
                <div className="pv-status-head">
                  <span className="pv-status-icon"><Icon size={14} /></span>
                  <ToolBrandIcon tool={tool} size={19} opacity={0.9} />
                  <span className="pv-status-name">{statusTitle(tool)}</span>
                  <span className="pv-spacer" />
                  <span className="pv-status-state">{STATUS_LABEL[state]}</span>
                </div>
                <div className="pv-status-foot">
                  <span className="pv-status-detail">{statusDetail(provider, state)}</span>
                  <span className="pv-spacer" />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </CardShell>
  );
}

// ── Provider quota card ────────────────────────────────────────────────────

export interface QuotaFreshness { lastSuccessAt?: number; lastAttemptAt?: number; error?: string }

export function ProviderQuotaCard({ view, settings, company, tool, now, mode, refreshIntervalSeconds, onRefresh, refreshing }: {
  view: QuotaView; settings: PresentationSettings | null; company: string; tool: string; now: number; mode: DisplayMode;
  refreshIntervalSeconds: number; onRefresh?: (tool: string) => void; refreshing?: boolean;
}) {
  const sections = companySections(view, settings, company);
  return (
    <CardShell className="pv-quota">
      <div className="pv-section-title">
        <ToolBrandBadge tool={tool} iconSize={16} containerSize={24} />
        <span className="pv-vendor">{company}</span>
        <span className="pv-spacer" />
        {refreshing ? <span className="pv-progress" /> : null}
        <IconButton title="Refresh" onClick={() => onRefresh?.(tool)} disabled={refreshing}><ArrowClockwise size={11} /></IconButton>
      </div>
      {sections.length === 0 ? (
        <MessageRow text={emptyMessage(tool)} tone="secondary" />
      ) : (
        sections.map((section, index) => (
          <div key={section.subProvider} className="pv-subprovider">
            {index > 0 ? <hr className="pv-rule" /> : null}
            {section.accounts.map((account) => (
              <SubProviderBlock
                key={account.accountId}
                account={account}
                subProvider={section.subProvider}
                now={now}
                mode={mode}
                staleAfter={staleAfterSeconds(refreshIntervalSeconds)}
                planOverrides={settings?.providerPlanLabels}
              />
            ))}
          </div>
        ))
      )}
    </CardShell>
  );
}

function SubProviderBlock({ account, subProvider, now, mode, staleAfter, planOverrides }: { account: AccountQuota; subProvider: string; now: number; mode: DisplayMode; staleAfter: number; planOverrides?: Record<string, string> | null }) {
  const stale = staleLabel(account.queriedAt, account.queriedAt, account.error?.detail ?? (account.error ? account.error.kind : undefined), staleAfter, now);
  return (
    <>
      <div className="pv-subprovider-row">
        <ToolBrandIcon tool={account.tool} size={13} opacity={0.85} />
        <span className="pv-subprovider-name">{subProvider}</span>
        <span className="pv-spacer" />
        <PlanBadge text={planBadgeLabel(account.tool, account.plan, planOverrides)} />
      </div>
      {stale ? (
        <div className="pv-stale" title="Cached quota is older than two refresh intervals.">
          <ClockBadge size={12} />
          <span>{stale}</span>
        </div>
      ) : null}
      {account.buckets.length === 0 ? (
        <MessageRow text={account.error ? account.error.detail ?? account.error.kind : "No quota windows reported."} tone={account.error ? "warning" : "secondary"} />
      ) : (
        <div className="pv-buckets">
          {bucketGroups(account.buckets).map((group, index) => (
            <div key={group.title ?? "__primary"} className="pv-bucket-group">
              {index > 0 ? <hr className="pv-rule faint" /> : null}
              {group.title ? <div className="pv-group-caption">{group.title}</div> : null}
              {group.buckets.map((bucket) => (
                <BucketRow key={bucket.id} tool={account.tool} bucket={bucket} now={now} mode={mode} />
              ))}
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function MessageRow({ text, tone }: { text: string; tone: "secondary" | "warning" }) {
  return (
    <div className={`pv-message ${tone}`}>
      <InfoCircle size={12} />
      <span>{text}</span>
    </div>
  );
}

/** Native `ProviderQuotaCard.emptyMessage`. */
function emptyMessage(tool: string): string {
  switch (tool) {
    case "codex": return "Run codex login, then refresh.";
    case "claude": return "Run claude login, then refresh.";
    case "grok": return "Run grok login or import grok.com cookies, then refresh.";
    case "cursor": return "Sign in to Cursor.app or import cursor.com cookies, then refresh.";
    default: return "Configure this provider in Settings → Misc Providers.";
  }
}

// ── Bucket row ─────────────────────────────────────────────────────────────

export function BucketRow({ tool, bucket, now, mode }: { tool: string; bucket: QuotaBucket; now: number; mode: DisplayMode }) {
  const percent = mode === "used" ? bucket.usedPercent : 100 - bucket.usedPercent;
  const status = resetStatus(bucket.resetAt, now);
  const expired = status?.isExpired ?? false;
  const forecast = bucket.forecast;
  const colour = quotaBarColor(percent, mode === "used");
  void tool;
  return (
    <div className="pv-bucket">
      <div className="pv-bucket-head">
        <span className="pv-bucket-title">{bucket.title}</span>
        {status ? <span className="pv-bucket-reset">{status.label}</span> : null}
        <span className="pv-spacer" />
        <span className="pv-bucket-percent" style={{ color: expired ? undefined : colour }}>{Math.round(percent)}%</span>
      </div>
      <div style={{ opacity: expired ? 0.45 : 1 }}>
        {forecast ? (
          <ForecastBar percent={percent} mode={mode} forecast={forecast} bucket={bucket} now={now} />
        ) : (
          <PaceBar percent={percent} colour={colour} bucket={bucket} now={now} mode={mode} />
        )}
      </div>
      {forecast ? (
        // Native's popover row is `QuotaForecastRow(showGuidance: false)`:
        // the verdict and the use-up line. The confidence label and the
        // guidance sentence belong to the detail pages.
        <div className="pv-forecast">
          <div className="pv-forecast-line">
            <span className="pv-forecast-dot" style={{ background: verdictColor(forecast.verdict) ?? "var(--pv-text-secondary)" }} />
            <span className="pv-forecast-primary" style={{ color: verdictColor(forecast.verdict) ?? "var(--pv-text-secondary)" }}>{forecastPrimaryText(forecast, mode)}</span>
          </div>
          <div className="pv-forecast-useup">{forecastUseUpText(forecast, now)}</div>
        </div>
      ) : (
        <PaceRow bucket={bucket} now={now} />
      )}
    </div>
  );
}

/** Native `UsagePaceRow`, the row a bucket gets before it has a forecast. */
function PaceRow({ bucket, now }: { bucket: QuotaBucket; now: number }) {
  const pace = usagePace(bucket, now);
  if (!pace) return null;
  const eta = paceEtaText(pace, now);
  return (
    <div className="pv-pace">
      <span style={{ color: paceColor(pace) }}>{paceStageSummary(pace)}</span>
      {eta ? <><span className="pv-pace-dot">·</span><span>{eta}</span></> : null}
    </div>
  );
}

/** Native `PaceMarkerCapsule`: the fill plus a neutral tick where an evenly spent window would be. */
function PaceBar({ percent, colour, bucket, now, mode }: { percent: number; colour: string; bucket: QuotaBucket; now: number; mode: DisplayMode }) {
  const pace = wallClockPace(bucket, now, mode);
  return (
    <div className="pv-bar">
      <div className="pv-bar-fill" style={{ width: `${Math.max(0, Math.min(100, percent))}%`, background: colour }} />
      {pace !== null && pace > 2 && pace < 98 ? <div className="pv-bar-pace" style={{ left: `calc(${pace}% - 2.5px)` }} title="Time-only pace" /> : null}
    </div>
  );
}

/**
 * Native `ForecastQuotaBar`: the actual fill, a translucent confidence band
 * from the lower to the upper projection, a neutral tick at the wall-clock
 * pace, and the median as a full-height line on a halo. All in the
 * verdict's colour, lightened a little in the dark appearance as native does.
 */
export function ForecastBar({ percent, mode, forecast, bucket, now }: { percent: number; mode: DisplayMode; forecast: NonNullable<QuotaBucket["forecast"]>; bucket: QuotaBucket; now: number }) {
  const toDisplay = (used: number) => (mode === "used" ? used : 100 - used);
  const clamp = (v: number) => Math.max(0, Math.min(100, v));
  const fill = clamp(percent);
  const median = clamp(toDisplay(forecast.projectedUsedPercent));
  const lower = clamp(toDisplay(forecast.projectedUsedUpperPercent));
  const upper = clamp(toDisplay(forecast.projectedUsedLowerPercent));
  const bandStart = Math.min(lower, upper);
  const bandEnd = Math.max(lower, upper);
  const colour = verdictColor(forecast.verdict) ?? "var(--pv-text-secondary)";
  const timePace = wallClockPace(bucket, now, mode);
  const actualColour = quotaBarColor(percent, mode === "used");
  const hasUncertainty = bandEnd - bandStart > 0.5;
  return (
    <div className="pv-bar forecast" style={{ ["--pv-verdict" as string]: colour }}>
      <div className="pv-bar-fill" style={{ width: `${fill}%`, background: actualColour }} />
      {hasUncertainty ? (
        <div className="pv-bar-band" style={{ left: `${bandStart}%`, width: `${Math.max(1.5, bandEnd - bandStart)}%` }} />
      ) : null}
      {timePace !== null && timePace > 2 && timePace < 98 ? (
        <div className="pv-bar-pace" style={{ left: `calc(${timePace}% - 2.5px)` }} title="Time-only pace" />
      ) : null}
      <div className="pv-bar-median" style={{ left: `calc(${median}% - 3.5px)` }} title="Forecast at reset" />
    </div>
  );
}

/** Where an evenly spent window would be right now, in display terms. */
function wallClockPace(bucket: QuotaBucket, now: number, mode: DisplayMode): number | null {
  if (!bucket.resetAt || !bucket.rawWindowSeconds || bucket.rawWindowSeconds <= 0) return null;
  const remaining = bucket.resetAt - now;
  if (remaining <= 0) return null;
  const elapsed = Math.max(0, Math.min(bucket.rawWindowSeconds, bucket.rawWindowSeconds - remaining));
  const expectedUsed = (elapsed / bucket.rawWindowSeconds) * 100;
  return mode === "used" ? expectedUsed : 100 - expectedUsed;
}

// ── Upcoming resets ────────────────────────────────────────────────────────

export function UpcomingResetsCard({ view, settings, now }: { view: QuotaView; settings: PresentationSettings | null; now: number }) {
  const events = upcomingResets(view, settings, now);
  return (
    <CardShell className="pv-resets">
      <div className="pv-card-title">
        <span className="pv-card-title-text">Upcoming Resets</span>
        <span className="pv-card-subtitle">next 7 days</span>
      </div>
      {events.length === 0 ? (
        <p className="pv-empty">Nothing refills in the next seven days.</p>
      ) : (
        <>
        <ResetLane events={events} now={now} />
        <div className="pv-reset-rows">
          {events.slice(0, 3).map((event) => (
            <div key={event.id} className="pv-reset-row">
              <span className="pv-reset-dot" style={{ background: quotaBarColor(event.remainingPercent, false) }} />
              <span className="pv-reset-label">{event.label}</span>
              <span className="pv-spacer" />
              <span className="pv-reset-gain" style={{ color: quotaBarColor(event.remainingPercent, false) }}>+{Math.round(event.gainPercent)}%</span>
              <span className="pv-reset-when">{countdown(event.resetAt, now) ?? "—"}</span>
            </div>
          ))}
        </div>
        </>
      )}
    </CardShell>
  );
}

const LANE_HEIGHT = 64;
const LANE_HORIZON_DAYS = 7;

/**
 * Native `ResetLaneView`: seven days of lane, a baseline, the "now" bar at
 * the left, a tick and label per day, and one marker per reset whose height
 * is the refill it brings and whose colour is what is left now. Markers that
 * would land on each other are fanned apart by the rail's rule.
 */
function ResetLane({ events, now }: { events: { id: string; remainingPercent: number; gainPercent: number; resetAt: number }[]; now: number }) {
  const [width, setWidth] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const el = ref.current;
    const observer = new ResizeObserver(() => setWidth(el.getBoundingClientRect().width));
    observer.observe(el);
    setWidth(el.getBoundingClientRect().width);
    return () => observer.disconnect();
  }, []);
  const horizon = LANE_HORIZON_DAYS * 86_400;
  const rail = events.map((e) => ({ id: e.id, fraction: Math.min(1, (e.resetAt - now) / horizon) }));
  const offsets = width > 0 ? fannedOffsets(rail as never, width) : [];
  const ticks = Array.from({ length: LANE_HORIZON_DAYS }, (_, i) => ({ fraction: (i + 1) / LANE_HORIZON_DAYS, label: `+${i + 1}d` }));
  return (
    <div ref={ref} className="pv-lane" style={{ height: LANE_HEIGHT }}>
      <div className="pv-lane-baseline" style={{ top: LANE_HEIGHT - 14 }} />
      <div className="pv-lane-now" style={{ top: 4, height: LANE_HEIGHT - 20 }} />
      {ticks.map((t) => (
        <span key={t.label}>
          <span className="pv-lane-tick" style={{ left: `${t.fraction * 100}%`, top: LANE_HEIGHT - 18 }} />
          <span className="pv-lane-tick-label" style={{ left: `calc(${t.fraction * 100}% - 15px)`, top: LANE_HEIGHT - 11 }}>{t.label}</span>
        </span>
      ))}
      {events.map((event, index) => {
        const fraction = Math.min(1, (event.resetAt - now) / horizon);
        const x = width > 0 ? Math.max(4, Math.min(width - 4, (offsets[index] ?? width * fraction))) : 0;
        const height = 6 + (LANE_HEIGHT - 30) * (event.gainPercent / 100);
        return (
          <span
            key={event.id}
            className="pv-lane-marker"
            title={`${Math.round(event.remainingPercent)}% left now · +${Math.round(event.gainPercent)}% comes back`}
            style={{ left: x - 3.5, top: LANE_HEIGHT - 14 - height, height, background: quotaBarColor(event.remainingPercent, false) }}
          />
        );
      })}
    </div>
  );
}

// ── Usage mix ──────────────────────────────────────────────────────────────

export function UsageMixCard({ cost, dark, onRefresh, refreshing }: { cost: CostView | null; dark: boolean; onRefresh?: () => void; refreshing?: boolean }) {
  const [dimension, setDimension] = useState<MixDimension>("harnesses");
  const { slices, total, empty } = usageMix(cost, dimension);
  const palette = ["#5B8DEF", "#8C8CD6", "#9A7BD4", "#6FB3E8", "#B0A7C9"];
  const colourFor = (slice: { id: string; label: string }, index: number) =>
    dimension === "harnesses" ? (providerAccent(harnessTool(slice.label), dark) ?? palette[index % palette.length]) : palette[index % palette.length];
  return (
    <CardShell className="pv-mix">
      <div className="pv-card-title">
        <span className="pv-card-title-text">Usage Mix</span>
        <span className="pv-spacer" />
        <span className="pv-meta">Last 30 days</span>
        {onRefresh ? <IconButton title="Refresh cost data" onClick={onRefresh} disabled={refreshing}><ArrowClockwise size={11} /></IconButton> : null}
      </div>
      <div className="pv-segmented" role="tablist">
        {MIX_DIMENSIONS.map((d) => (
          <button key={d.id} type="button" role="tab" aria-selected={dimension === d.id} className={`pv-segment${dimension === d.id ? " selected" : ""}`} onClick={() => setDimension(d.id)}>{d.title}</button>
        ))}
      </div>
      {empty ? (
        <p className="pv-empty-centre">{empty}</p>
      ) : (
        <div className="pv-mix-body">
          <Donut slices={slices.map((s, i) => ({ value: s.tokens, colour: colourFor(s, i) }))} total={total} />
          <div className="pv-mix-rows">
            {slices.map((slice, index) => (
              <div key={slice.id} className="pv-mix-row">
                <span className="pv-mix-dot" style={{ background: colourFor(slice, index) }} />
                <span className="pv-mix-text">
                  <span className="pv-mix-label">{slice.label}</span>
                  {slice.detail ? <span className="pv-mix-detail">{slice.detail}</span> : null}
                </span>
                <span className="pv-spacer" />
                <span className="pv-mix-nums">
                  <span className="pv-mix-tokens">{formatTokens(slice.tokens)}</span>
                  <span className="pv-mix-share">{Math.round((slice.tokens / total) * 100)}%</span>
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </CardShell>
  );
}

function harnessTool(harness: string): string {
  const h = harness.toLowerCase();
  if (h.includes("codex") || h.includes("chatgpt")) return "codex";
  if (h.includes("claude")) return "claude";
  if (h.includes("gemini")) return "gemini";
  if (h.includes("antigravity")) return "antigravity";
  if (h.includes("grok")) return "grok";
  if (h.includes("cursor")) return "cursor";
  return h;
}

/** Native's `SectorMark` donut: an SVG ring with a 2px gap between sectors and the total in the middle. */
function Donut({ slices, total }: { slices: { value: number; colour: string }[]; total: number }) {
  const size = 118, stroke = 22, r = (size - stroke) / 2, c = 2 * Math.PI * r;
  let offset = 0;
  const gap = 2;
  return (
    <div className="pv-donut" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <g transform={`rotate(-90 ${size / 2} ${size / 2})`}>
          {slices.map((s, i) => {
            const len = Math.max(0, (s.value / total) * c - gap);
            const el = (
              <circle key={i} cx={size / 2} cy={size / 2} r={r} fill="none" stroke={s.colour} strokeWidth={stroke}
                strokeDasharray={`${len} ${c - len}`} strokeDashoffset={-offset} />
            );
            offset += (s.value / total) * c;
            return el;
          })}
        </g>
      </svg>
      <div className="pv-donut-centre">
        <span className="pv-donut-total">{formatTokens(total)}</span>
        <span className="pv-donut-unit">tokens</span>
      </div>
    </div>
  );
}
