import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";

import type { PresentationSettings, QuotaBucket, QuotaForecast, QuotaView } from "../api";
import { api, formatRemaining, quotaBarColor } from "../api";
import { companyFor, groupLabelFor, subProviderFor } from "../naming";
import { useDarkMode } from "../theme";
import { FORECAST_VERDICT, providerAccent } from "../tokens";
import { RingGauge } from "./RingGauge";

const DEFAULT_FIELDS = ["codex.weekly", "claude.weekly", "claude.five_hour"];
/** Beyond this the window is wider than the screen it sits on the edge of. */
const MAX_CELLS = 12;

/** One dial: a bucket, and what is said about it. */
interface Cell {
  id: string;
  bucket: QuotaBucket;
  label: string;
  value: number;
  showsUsed: boolean;
}

/** The quota axis as this window draws it — company, SubProvider, model group,
 *  then the windows themselves. */
interface Group {
  label: string | null;
  cells: Cell[];
}
interface SubProvider {
  name: string;
  groups: Group[];
}
interface Company {
  name: string;
  tool: string;
  subProviders: SubProvider[];
}

/** The frameless mini window moves by its body, like the native panel. The
 *  drag-region attribute only covers the element it sits on; a press on any
 *  child that is not a control starts the drag explicitly. */
function beginDrag(event: React.MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, a, input, select, textarea, [data-tauri-drag-region='false']")) return;
  void getCurrentWindow().startDragging().catch(() => undefined);
}

export function MiniQuota() {
  const [view, setView] = useState<QuotaView | null>(null);
  const [settings, setSettings] = useState<PresentationSettings | null>(null);

  useEffect(() => {
    const refreshSettings = () =>
      api.presentationSettings().then(setSettings).catch(() => undefined);
    api.quotaView().then(setView).catch(() => undefined);
    refreshSettings();
    const unlistenQuota = api.onQuotaUpdated((next) => {
      setView(next);
      refreshSettings();
    });
    const unlistenShown = api.onMiniShown(refreshSettings);
    return () => {
      unlistenQuota.then((off) => off()).catch(() => undefined);
      unlistenShown.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const fields = settings?.selectedFieldIds.length
    ? settings.selectedFieldIds
    : [
        ...new Set([
          ...DEFAULT_FIELDS,
          ...(view?.accounts.flatMap((account) =>
            account.buckets[0] ? [`${account.tool}.${account.buckets[0].id}`] : [],
          ) ?? []),
        ]),
      ];
  const companies = view ? arrange(view, settings, fields.slice(0, MAX_CELLS)) : [];

  return (
    <main className="mini-quota" data-tauri-drag-region onMouseDown={beginDrag}>
      <div className="mini-title" data-tauri-drag-region="deep">
        <span>Vibe Bar</span>
        <button
          className="mini-close"
          aria-label="Hide Mini"
          data-tauri-drag-region="false"
          onClick={() => void api.hideMini().catch(() => undefined)}
        >
          ×
        </button>
      </div>
      <MiniQuotaBody
        companies={companies}
        loading={view === null}
        layout={settings?.miniDisplayMode}
        density={settings?.miniStripDensity}
        order={fields.slice(0, MAX_CELLS)}
        onContentSize={api.resizeMini}
      />
    </main>
  );
}

/**
 * The arranged tree, without the plumbing that fetches it.
 *
 * Split out so the layout can be mounted on its own — the quota axis is the
 * part worth looking at, and it should not need a running Tauri host to see.
 */
export function MiniQuotaBody({
  companies,
  loading = false,
  layout = "regular",
  density = "roomy",
  order,
  onContentSize,
}: {
  companies: Company[];
  loading?: boolean;
  /** The field ids in the order they were chosen, for the layouts that page or
   *  tile through buckets rather than drawing the tree. */
  order?: string[];
  /** Called with the size the content needs, so the shell can fit the window
   *  to it. Absent in the preview page, which is a page and not a window. */
  onContentSize?: (width: number, height: number) => void;
  /** Which of native's layouts to draw: regular, compact, ledger, tile,
   *  focus, rail or strip — all seven now. */
  layout?: string;
  /** The strip's density. Ignored by every other layout, because native
   *  stores it per window rather than per layout. */
  density?: string;
}) {
  const dark = useDarkMode();
  const measured = useContentSize(onContentSize);
  // The layouts that are not the tree, looked up rather than chained — six of
  // them is well past where a ternary stops being readable, and the condition
  // they share belongs in one place. (The comment said this before the code
  // did; the chain it described is gone.)
  const alternatives: Record<string, () => JSX.Element> = {
    ledger: () => <MiniLedger companies={companies} dark={dark} />,
    tile: () => <MiniTiles entries={flatten(companies, order)} dark={dark} />,
    focus: () => <MiniFocus entries={flatten(companies, order)} dark={dark} />,
    rail: () => <MiniRail entries={flatten(companies, order)} dark={dark} />,
    strip: () => <MiniStrip entries={flatten(companies, order)} density={density} />,
  };
  const drawn =
    loading || companies.length === 0 ? null : (alternatives[layout]?.() ?? null);
  if (drawn) {
    // One wrapper for every layout, laid out at its content's size so the
    // measurement is the layout's and not the window's.
    return (
      <div ref={measured} style={{ width: "max-content" }}>
        {drawn}
      </div>
    );
  }
  return (
    <div ref={measured} style={{ width: "max-content" }}>
      {loading ? (
        <p className="mini-empty">Loading quota…</p>
      ) : companies.length === 0 ? (
        <p className="mini-empty">No configured quota is available.</p>
      ) : (
        <div className="mini-companies">
          {companies.map((company, index) => (
            <section className="mini-company" key={company.name}>
              {index > 0 && <span className="mini-company-rule" aria-hidden />}
              <h2 className="mini-company-name">
                <span
                  className="mini-company-dot"
                  style={{ background: providerAccent(company.tool, dark) }}
                  aria-hidden
                />
                {company.name}
              </h2>
              <div className="mini-subproviders">
                {company.subProviders.map((subProvider) => (
                  <div className="mini-subprovider" key={subProvider.name}>
                    <h3 className="mini-subprovider-name">{subProvider.name}</h3>
                    <div className="mini-groups">
                      {subProvider.groups.map((group, groupIndex) => (
                        <div className="mini-group" key={group.label ?? groupIndex}>
                          {/* Named only where the SubProvider has more than one, so a
                              heading always means "these are different models" — "ALL"
                              over a lone column says nothing. The line is kept either
                              way, so dials in neighbouring columns stay level. */}
                          <div className="mini-group-name">
                            {subProvider.groups.length > 1 ? group.label ?? " " : " "}
                          </div>
                          <div className="mini-cells">
                            {group.cells.map((cell) =>
                              layout === "compact" ? (
                                <MiniCompactCell key={cell.id} cell={cell} />
                              ) : (
                                <MiniCell key={cell.id} cell={cell} />
                              ),
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * The same three tiers as vertical bars, sized for a corner.
 *
 * Same arrangement as the rings — company → SubProvider → group is decided in
 * `arrange`, not here — and the same facts per cell. What changes is the
 * gauge: a bar fills from the bottom, which fits a narrow column where a ring
 * needs a square. The forecast marker is a line across the bar rather than a
 * notch on an arc, for the same reason.
 */
/**
 * Report the size the whole window needs, whenever it changes.
 *
 * Measured from the content's intrinsic size, and from nothing the current
 * window size can influence. Both obvious shortcuts are viewport-bound and
 * fail in opposite directions:
 *
 * - the wrapper's border box is its *container's* width, so it reads 244 in a
 *   272 window and stays 244 while a 284 ledger overflows — the window can
 *   never grow;
 * - `documentElement.scrollWidth` is floored by the viewport, so once tiles
 *   have widened the window to 525, focus still reads 525 — it can never
 *   shrink.
 *
 * So the wrapper is laid out at `max-content`, which makes its box the
 * layout's own size, and the surface's padding and title are added
 * explicitly.
 *
 * Reported after every render as well as from the observer: a
 * `ResizeObserver` fires on its target's border box, and content that grows
 * only in overflow does not move it — a tile grid going from one bucket to
 * four stays one row. The observer stays for what a render does not cause,
 * such as the window's own resize or a font change.
 */
function useContentSize(report?: (width: number, height: number) => void) {
  const node = useRef<HTMLDivElement | null>(null);
  const observer = useRef<ResizeObserver | null>(null);
  const lastSent = useRef<[number, number] | null>(null);

  const send = useCallback(() => {
    const content = node.current;
    const surface = content?.parentElement;
    if (!report || !content || !surface) return;

    const style = getComputedStyle(surface);
    const chromeWidth = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    let chromeHeight = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    // Everything in the surface that is not the layout — the title, and its
    // margins. Summed rather than read off the surface's own box, which is the
    // viewport's. Heights only: the title stretches to whatever width the
    // layout asks for, so its width says nothing about what is needed.
    for (const sibling of surface.children) {
      if (sibling === content) continue;
      const siblingStyle = getComputedStyle(sibling);
      chromeHeight +=
        sibling.getBoundingClientRect().height +
        parseFloat(siblingStyle.marginTop) +
        parseFloat(siblingStyle.marginBottom);
    }

    const width = content.scrollWidth + chromeWidth;
    const height = content.scrollHeight + chromeHeight;
    if (width <= 0 || height <= 0) return;
    const [lastWidth, lastHeight] = lastSent.current ?? [0, 0];
    if (Math.abs(width - lastWidth) < 1 && Math.abs(height - lastHeight) < 1) return;
    lastSent.current = [width, height];
    report(width, height);
  }, [report]);

  useEffect(send);

  return useCallback(
    (next: HTMLDivElement | null) => {
      observer.current?.disconnect();
      observer.current = null;
      node.current = next;
      if (!next || !report) return;
      const watcher = new ResizeObserver(send);
      watcher.observe(next);
      observer.current = watcher;
    },
    [report, send],
  );
}

/** One bucket with the tiers above it carried along, for the layouts that
 *  draw a flat list rather than a tree. */
export interface Entry {
  cell: Cell;
  company: string;
  tool: string;
  subProvider: string;
  groupLabel: string | null;
}

/**
 * The arranged tree as a flat list, parents kept.
 *
 * Tiles and focus pages are one per bucket with no headers, so each has to say
 * whose quota it is — a bare "All · Weekly" says nothing about that.
 *
 * `order` restores the sequence the fields were chosen in. The tree does not
 * preserve it: `arrange` gathers each company's buckets together, so fields
 * picked as codex, claude, codex come back as both Codex buckets and then
 * Claude. Focus pages through these in the user's order, which is native's
 * stated rule, so walking the tree is not enough.
 */
export function flatten(companies: Company[], order?: string[]): Entry[] {
  const entries: Entry[] = [];
  for (const company of companies) {
    for (const subProvider of company.subProviders) {
      for (const group of subProvider.groups) {
        for (const cell of group.cells) {
          entries.push({
            cell,
            company: company.name,
            tool: company.tool,
            subProvider: subProvider.name,
            // Named only where the SubProvider has more than one, the same
            // rule the tree layouts use for their group headings.
            groupLabel: subProvider.groups.length > 1 ? group.label : null,
          });
        }
      }
    }
  }
  if (!order) return entries;
  const rank = new Map(order.map((field, index) => [field, index]));
  // A bucket the order does not mention keeps its tree position, after the
  // ones it does — the order is the user's selection, which a runtime-found
  // field may not be in yet.
  return entries
    .map((entry, index) => ({ entry, index }))
    .sort(
      (left, right) =>
        (rank.get(left.entry.cell.id) ?? order.length + left.index) -
        (rank.get(right.entry.cell.id) ?? order.length + right.index),
    )
    .map((ranked) => ranked.entry);
}

/** Seven days, in seconds. The rail's horizon. */
const RAIL_HORIZON_SECONDS = 7 * 86_400;
/** How far apart two markers have to be before they stop being one blob. */
const RAIL_FAN_STEP = 9;
/** How close a marker's centre may come to the end of the lane. */
const RAIL_MARKER_INSET = 4;

interface RailEvent {
  entry: Entry;
  /** Where it sits along the horizon, 0 to 1. */
  fraction: number;
  /** What comes back at the reset. */
  gain: number;
  /** What is left now, which is what colours it. */
  remaining: number;
  resetAt: number;
}

/**
 * The buckets that actually refill inside the horizon, soonest first.
 *
 * Filtered the way native filters: a reset in the future, inside seven days,
 * and something to come back — a bucket at 0% used has no refill to draw, and
 * a marker of zero height on the lane is a claim that nothing happens.
 */
export function railEvents(entries: Entry[], now: number): RailEvent[] {
  return entries
    .flatMap((entry) => {
      const resetAt = entry.cell.bucket.resetAt;
      const used = entry.cell.bucket.usedPercent;
      if (!resetAt || resetAt <= now) return [];
      const ahead = resetAt - now;
      if (ahead > RAIL_HORIZON_SECONDS || used < 1) return [];
      return [
        {
          entry,
          fraction: Math.min(1, ahead / RAIL_HORIZON_SECONDS),
          gain: used,
          remaining: Math.max(0, 100 - used),
          resetAt,
        },
      ];
    })
    .sort((left, right) => left.resetAt - right.resetAt);
}

/**
 * Nudge markers that would land on top of each other.
 *
 * Two passes, because centring a cluster and keeping clusters apart are
 * different problems and doing only the first creates the second.
 *
 * First, markers that fall within a step of each other are spread evenly
 * around their own centre — native's behaviour, and what keeps a cluster
 * reading as "these all reset at about the same time" rather than as a line
 * marching rightwards.
 *
 * Then a sweep out and back enforces the spacing and the lane's edges over
 * *every* marker, not within a group. Centring alone is not enough: a group
 * pushed inward at an edge can land on the marker beside it — centres at 0, 8
 * and 17 become 4, 13 and 17, and the last two 7-wide bars overlap. The sweep
 * settles that by construction, so there is no grouping rule left to get
 * wrong.
 */
export function fannedOffsets(events: RailEvent[], width: number): number[] {
  const placed = events
    .map((event, index) => ({ index, x: width * event.fraction }))
    .sort((left, right) => left.x - right.x);
  if (placed.length === 0) return [];

  // Pass one: centre each cluster on itself.
  const desired = placed.map((marker) => marker.x);
  let start = 0;
  const centreCluster = (end: number) => {
    const size = end - start;
    if (size > 1) {
      const span = (size - 1) * RAIL_FAN_STEP;
      const centre = (placed[start].x + placed[end - 1].x) / 2;
      for (let i = start; i < end; i += 1) {
        desired[i] = centre - span / 2 + (i - start) * RAIL_FAN_STEP;
      }
    }
    start = end;
  };
  for (let i = 1; i < placed.length; i += 1) {
    if (placed[i].x - placed[i - 1].x >= RAIL_FAN_STEP) centreCluster(i);
  }
  centreCluster(placed.length);

  // Pass two: no two markers closer than a step, none off the lane.
  const settled = desired.slice();
  settled[0] = Math.max(settled[0], RAIL_MARKER_INSET);
  for (let i = 1; i < settled.length; i += 1) {
    settled[i] = Math.max(settled[i], settled[i - 1] + RAIL_FAN_STEP);
  }
  const last = settled.length - 1;
  settled[last] = Math.min(settled[last], width - RAIL_MARKER_INSET);
  for (let i = last - 1; i >= 0; i -= 1) {
    settled[i] = Math.min(settled[i], settled[i + 1] - RAIL_FAN_STEP);
  }

  const offsets = new Array(events.length).fill(0);
  placed.forEach((marker, i) => {
    offsets[marker.index] = settled[i] - marker.x;
  });
  return offsets;
}

/**
 * The next seven days as a refill lane, with the coming resets listed.
 *
 * A marker's height is how much comes back, its colour is how much is left
 * now, and its position is when. The legend under it names the first few,
 * because a lane of bars says when and how much but never which.
 */
function MiniRail({ entries, dark }: { entries: Entry[]; dark: boolean }) {
  const now = Date.now() / 1000;
  const events = railEvents(entries, now);
  const width = 536;
  const laneHeight = 64;
  const offsets = fannedOffsets(events, width);

  return (
    <div className="mini-rail">
      <div className="mini-rail-title">QUOTA RESETS · NEXT 7 DAYS</div>
      {events.length === 0 ? (
        <p className="mini-rail-empty">Nothing selected refills in the next seven days.</p>
      ) : (
        <>
          <div className="mini-rail-lane" style={{ width, height: laneHeight }}>
            <span className="mini-rail-baseline" style={{ top: laneHeight - 14 }} />
            <span className="mini-rail-now" />
            {[1, 2, 3, 4, 5, 6, 7].map((day) => (
              <span key={day} className="mini-rail-tick" style={{ left: (width * day) / 7 }}>
                <span className="mini-rail-tick-mark" style={{ top: laneHeight - 18 }} />
                <span className="mini-rail-tick-label" style={{ top: laneHeight - 11 }}>
                  +{day}d
                </span>
              </span>
            ))}
            {events.map((event, index) => {
              const height = 6 + ((laneHeight - 30) * event.gain) / 100;
              // The group shift has already kept fanned markers on the lane;
              // this catches a lone marker at either end.
              const x = Math.max(
                RAIL_MARKER_INSET,
                Math.min(width - RAIL_MARKER_INSET, width * event.fraction + offsets[index]),
              );
              return (
                <span
                  key={event.entry.cell.id}
                  className="mini-rail-marker"
                  style={{
                    left: x - 3.5,
                    top: laneHeight - 14 - height,
                    height,
                    background: quotaBarColor(event.remaining, false),
                  }}
                  title={`${entryLabel(event.entry)} — ${Math.round(event.remaining)}% now, +${Math.round(event.gain)}% ${formatRemaining(event.resetAt) ?? ""}`}
                />
              );
            })}
          </div>
          <div className="mini-rail-legend" style={{ width }}>
            {events.slice(0, 4).map((event) => (
              <div className="mini-rail-legend-row" key={event.entry.cell.id}>
                <span
                  className="mini-company-dot"
                  style={{ background: providerAccent(event.entry.tool, dark) }}
                  aria-hidden
                />
                {/* The SubProvider as well as the bucket: Codex and Claude
                    both have a "Weekly", and a coloured dot is not a name.
                    The other flat layouts carry the same context. */}
                <span className="mini-rail-legend-sub">{event.entry.subProvider}</span>
                <span className="mini-rail-legend-label">{entryLabel(event.entry)}</span>
                <span
                  className="mini-rail-legend-gain"
                  style={{ color: quotaBarColor(event.remaining, false) }}
                >
                  +{Math.round(event.gain)}%
                </span>
                <span className="mini-rail-legend-when">
                  {formatRemaining(event.resetAt) || "—"}
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/**
 * One selected bucket at a time, large.
 *
 * A page per bucket in the order the fields were chosen — not a per-company
 * "most critical" of the layout's own choosing, which is native's stated rule
 * and the reason the pager is a plain index rather than anything clever.
 *
 * (Native's own doc comment above `MiniFocusLayout` says it pages by company;
 * the code below it says bucket, and the code is what this follows.)
 */
/** The bucket's name with its model group, where it has one. `arrange` moves
 *  the group out of the label, so without it Claude's ordinary weekly and its
 *  Opus weekly both read "Weekly". */
function entryLabel(entry: Entry): string {
  return entry.groupLabel ? `${entry.groupLabel} · ${entry.cell.label}` : entry.cell.label;
}

function MiniFocus({ entries, dark }: { entries: Entry[]; dark: boolean }) {
  const [page, setPage] = useState(0);
  // A refresh can drop the bucket that was open, so an index outliving its
  // page would show nothing at all.
  const index = Math.min(page, entries.length - 1);
  const headline = entries[index];
  // The rest of this SubProvider's buckets, which is the context a single
  // large dial loses.
  const others = entries
    .filter(
      (entry) =>
        entry !== headline &&
        entry.tool === headline.tool &&
        entry.subProvider === headline.subProvider,
    )
    .slice(0, 3);

  const { cell } = headline;
  const colour = quotaBarColor(cell.value, cell.showsUsed);
  const forecast = cell.bucket.forecast;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;
  const line = forecast ? forecastLine(forecast) : formatRemaining(cell.bucket.resetAt) || "—";
  const label = entryLabel(headline);

  return (
    <div className="mini-focus">
      <div className="mini-focus-company">
        <span
          className="mini-company-dot"
          style={{ background: providerAccent(headline.tool, dark) }}
          aria-hidden
        />
        {headline.company}
      </div>
      <div className="mini-focus-sub">
        {headline.subProvider} · {label}
      </div>
      <RingGauge
        percent={cell.value}
        expected={forecast ? plannedFor(forecast, cell.showsUsed) : undefined}
        color={colour}
        markerColor={verdictColour}
        size={84}
        lineWidth={7}
      >
        <span className="mini-focus-value" style={{ color: colour }}>
          {Math.round(cell.value)}%
        </span>
      </RingGauge>
      <div className="mini-focus-others">
        {others.map((entry) => (
          <span className="mini-focus-other" key={entry.cell.id}>
            <span className="mini-focus-other-label">{entryLabel(entry)}</span>
            <span
              style={{ color: quotaBarColor(entry.cell.value, entry.cell.showsUsed) }}
            >
              {Math.round(entry.cell.value)}%
            </span>
          </span>
        ))}
      </div>
      <div className="mini-focus-line" style={verdictColour ? { color: verdictColour } : undefined}>
        {line}
      </div>
      <div className="mini-focus-pager">
        {/* Dots up to eight, a counter beyond: nine dots in 252 points stop
            being separable, which is native's cut-off too. */}
        {entries.length <= 8 ? (
          entries.map((entry, dot) => (
            <button
              type="button"
              key={entry.cell.id}
              className="mini-focus-dot"
              // The group-qualified label, like the heading: two of a
              // SubProvider's groups otherwise announce as the same "Weekly".
              aria-label={`Show ${entry.company} ${entryLabel(entry)}`}
              aria-current={dot === index}
              onClick={() => setPage(dot)}
            >
              {/* The button is the target; this is the 5px dot. A transparent
                  box-shadow paints a larger circle but is not hit-tested, so
                  it made the dots look bigger than they were to click. */}
              <span
                className="mini-focus-dot-mark"
                style={{
                  background:
                    dot === index
                      ? providerAccent(entry.tool, dark)
                      : "color-mix(in srgb, currentColor 18%, transparent)",
                }}
              />
            </button>
          ))
        ) : (
          <span className="mini-focus-count">
            {index + 1}/{entries.length}
          </span>
        )}
        {entries.length > 1 ? (
          <button
            type="button"
            className="mini-focus-next"
            onClick={() => setPage((current) => (Math.min(current, entries.length - 1) + 1) % entries.length)}
            aria-label="Next quota"
          >
            ›
          </button>
        ) : null}
      </div>
    </div>
  );
}

/**
 * A grid of tiles with a big number and a severity stripe.
 *
 * Four columns at most, like native, so a window with many buckets grows
 * downward rather than off the side of the screen.
 */
/**
 * The strip's geometry, which is arithmetic rather than a stylesheet.
 *
 * Native computes the window's size from the entry count before drawing
 * anything, and the cells never shrink to fit: past `MAX_ROW_WIDTH` the row
 * wraps into further bands instead. Both facts have to hold here too, because
 * the shell sizes the window from what this reports — a layout that let CSS
 * decide the wrap would be measured at one width and drawn at another.
 *
 * `twoLine` is a column of two stacked cells rather than a wider cell, so its
 * unit of wrapping is the column and an odd last entry leaves a half-empty one.
 */
const STRIP: Record<string, { cell: number; band: number; gap: number; font: number }> = {
  roomy: { cell: 132, band: 40, gap: 8, font: 11.5 },
  twoLine: { cell: 128, band: 54, gap: 12, font: 10.5 },
  narrow: { cell: 96, band: 32, gap: 4, font: 9.5 },
};
const STRIP_LEADING = 14;
// Native reserves this for the window's close button, not for whitespace.
const STRIP_TRAILING = 26;
const STRIP_ROW_GAP = 2;
const STRIP_PAIR_GAP = 2;
const MAX_ROW_WIDTH = 1180;

export function stripDensity(name: string | undefined): "roomy" | "twoLine" | "narrow" {
  return name === "twoLine" || name === "narrow" ? name : "roomy";
}

/**
 * How the entries divide into bands, and how wide the result is.
 *
 * The width is a *full* band even when the last one is partial — native pads
 * to `perBand` cells rather than to the widest row, so a window holding nine
 * roomy cells is as wide as one holding sixteen.
 */
export function stripBands(count: number, density: string) {
  const metrics = STRIP[stripDensity(density)];
  const cells = stripDensity(density) === "twoLine" ? Math.ceil(count / 2) : count;
  const available = MAX_ROW_WIDTH - STRIP_LEADING - STRIP_TRAILING;
  const perBand = Math.max(
    1,
    Math.min(cells, Math.floor((available + metrics.gap) / (metrics.cell + metrics.gap))),
  );
  const bands = cells === 0 ? 0 : Math.ceil(cells / perBand);
  const width =
    STRIP_LEADING + STRIP_TRAILING + perBand * metrics.cell + Math.max(0, perBand - 1) * metrics.gap;
  const height = bands * metrics.band + Math.max(0, bands - 1) * STRIP_ROW_GAP;
  return { perBand, bands, width, height };
}

// No `dark`: the strip draws no provider accent, so it has nothing to tint.
function MiniStrip({ entries, density }: { entries: Entry[]; density: string }) {
  const chosen = stripDensity(density);
  const metrics = STRIP[chosen];
  // Two entries to a column, so the thing that wraps is the column.
  const units: Entry[][] =
    chosen === "twoLine"
      ? Array.from({ length: Math.ceil(entries.length / 2) }, (_, i) =>
          entries.slice(i * 2, i * 2 + 2),
        )
      : entries.map((entry) => [entry]);
  const { perBand } = stripBands(entries.length, chosen);
  const bands = Array.from({ length: Math.ceil(units.length / perBand) }, (_, i) =>
    units.slice(i * perBand, (i + 1) * perBand),
  );

  return (
    <div
      className="mini-strip"
      style={{
        paddingLeft: STRIP_LEADING,
        paddingRight: STRIP_TRAILING,
        rowGap: STRIP_ROW_GAP,
        fontSize: metrics.font,
      }}
    >
      {bands.map((band, index) => (
        <div key={index} className="mini-strip-band" style={{ columnGap: metrics.gap }}>
          {band.map((unit) => (
            <div
              key={unit[0].cell.id}
              className="mini-strip-unit"
              style={{ width: metrics.cell, height: metrics.band, rowGap: STRIP_PAIR_GAP }}
            >
              {unit.map((entry) => (
                <StripCell key={entry.cell.id} entry={entry} />
              ))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/**
 * Label and percentage, and nothing else.
 *
 * No bar, no dot, no countdown, in any density — the strip is the layout that
 * gives up everything but the number, and `narrow` is the same cell at 9.5pt
 * rather than a different one. Which quota it is survives only in the tooltip,
 * as it does natively, and that tooltip carries the SubProvider the cell has
 * no room to name.
 */
function StripCell({ entry }: { entry: Entry }) {
  const { cell } = entry;
  const label = entryLabel(entry);
  return (
    <div className="mini-strip-cell" title={`${entry.subProvider} — ${label}: ${Math.round(cell.value)}%`}>
      <span className="mini-strip-label">{label}</span>
      <span
        className="mini-strip-percent"
        style={{ color: quotaBarColor(cell.value, cell.showsUsed) }}
      >
        {Math.round(cell.value)}%
      </span>
    </div>
  );
}

function MiniTiles({ entries, dark }: { entries: Entry[]; dark: boolean }) {
  const columns = Math.min(4, Math.max(1, entries.length));
  return (
    <div
      className="mini-tiles"
      style={{ gridTemplateColumns: `repeat(${columns}, 120px)` }}
    >
      {entries.map((entry) => (
        <MiniTile key={entry.cell.id} entry={entry} dark={dark} />
      ))}
    </div>
  );
}

function MiniTile({ entry, dark }: { entry: Entry; dark: boolean }) {
  const { cell } = entry;
  const colour = quotaBarColor(cell.value, cell.showsUsed);
  const label = entryLabel(entry);

  return (
    // Native shrinks the caption to fit and still carries a tooltip with the
    // full text; CSS cannot shrink, so the tooltip is doing more work here.
    // A 120-point tile cannot hold "OpenAI · ChatGPT Agentic" at a readable
    // size, and clipping without a way to read it would lose which quota the
    // tile is about.
    <div className="mini-tile" title={`${entry.company} · ${entry.subProvider} · ${label}`}>
      {/* The severity stripe: the same colour the number takes, so the tile
          reads at a glance before any of its text does. */}
      <span className="mini-tile-stripe" style={{ background: colour }} aria-hidden />
      <div className="mini-tile-body">
        <div className="mini-tile-parents">
          <span
            className="mini-company-dot"
            style={{ background: providerAccent(entry.tool, dark) }}
            aria-hidden
          />
          {/* Two tones so the tiers still read apart in one line. */}
          <span className="mini-tile-company">{entry.company}</span>
          <span className="mini-tile-sub">{entry.subProvider}</span>
        </div>
        <div className="mini-tile-label" title={label}>
          {label}
        </div>
        <div className="mini-tile-figure">
          <span className="mini-tile-value" style={{ color: colour }}>
            {Math.round(cell.value)}%
          </span>
          <span className="mini-tile-track">
            <span
              className="mini-tile-fill"
              style={{
                width: `${Math.min(100, Math.max(0, cell.value))}%`,
                background: colour,
              }}
            />
          </span>
        </div>
      </div>
    </div>
  );
}

/**
 * One row per quota bucket — fixed width, grows downward.
 *
 * The same tree as the other layouts, laid out vertically: company, then
 * SubProvider, then the group where one is named, then a row per bucket. The
 * tiers are indents rather than columns, and every bar starts at the same x so
 * the column reads as one scale — which is why the group header and the rows
 * share an indent instead of the header stepping in further than its own
 * children.
 */
function MiniLedger({ companies, dark }: { companies: Company[]; dark: boolean }) {
  return (
    <div className="mini-ledger">
      {companies.map((company) => (
        <section className="mini-ledger-company" key={company.name}>
          <h2 className="mini-ledger-company-name">
            <span
              className="mini-company-dot"
              style={{ background: providerAccent(company.tool, dark) }}
              aria-hidden
            />
            {company.name}
          </h2>
          {company.subProviders.map((subProvider) => (
            <div key={subProvider.name}>
              <h3 className="mini-ledger-sub-name">{subProvider.name}</h3>
              {subProvider.groups.map((group, groupIndex) => (
                <div key={group.label ?? groupIndex}>
                  {/* Only where the SubProvider has more than one: a heading
                      always means "these are different models", the same rule
                      the other layouts follow. */}
                  {subProvider.groups.length > 1 && group.label ? (
                    <h4 className="mini-ledger-group-name">{group.label}</h4>
                  ) : null}
                  {group.cells.map((cell) => (
                    <MiniLedgerRow key={cell.id} cell={cell} />
                  ))}
                </div>
              ))}
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

function MiniLedgerRow({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const planned = forecast ? plannedFor(forecast, showsUsed) : undefined;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;

  return (
    <div className="mini-ledger-row">
      <span className="mini-ledger-label" title={cell.label}>
        {cell.label}
      </span>
      <span className="mini-ledger-track">
        <span
          className="mini-ledger-fill"
          style={{ width: `${Math.min(100, Math.max(0, value))}%`, background: colour }}
        />
        {/* Same guard as the ring and the compact bar: a projection at or past
            the limit is not drawn, because a mark on the rim would read as
            "expected to land exactly there". */}
        {planned !== undefined && planned > 0 && planned < 100 ? (
          <span
            className="mini-ledger-mark"
            style={{ left: `${planned}%`, background: verdictColour ?? colour }}
          />
        ) : null}
      </span>
      <span className="mini-ledger-value" style={{ color: colour }}>
        {Math.round(value)}%
      </span>
      <span className="mini-ledger-reset">{formatRemaining(bucket.resetAt) || "—"}</span>
    </div>
  );
}

function MiniCompactCell({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;
  const planned = forecast ? plannedFor(forecast, showsUsed) : undefined;

  return (
    <div className="mini-compact-cell">
      <div
        className="mini-compact-bar"
        role="img"
        aria-label={`${cell.label} ${Math.round(value)}%`}
      >
        <div
          className="mini-compact-fill"
          style={{ height: `${Math.min(100, Math.max(0, value))}%`, background: colour }}
        />
        {/* Where the forecast expects to be at the reset. Drawn only when it
            is on the bar: a marker pinned to the rim would read as a
            prediction of exactly full, which is not what a projection past
            100% means. */}
        {planned !== undefined && planned > 0 && planned < 100 ? (
          <div
            className="mini-compact-mark"
            style={{ bottom: `${planned}%`, background: verdictColour ?? colour }}
          />
        ) : null}
      </div>
      <div className="mini-compact-value" style={{ color: colour }}>
        {Math.round(value)}%
      </div>
      <div className="mini-compact-label">{cell.label}</div>
    </div>
  );
}

function MiniCell({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;
  // The cell is 62px wide and this line is the widest thing in it. Native
  // shrinks the text rather than clipping it, which is worth copying: "out 3d"
  // and "out 3d 14h" are different answers, and an ellipsis hides which.
  const line = forecast ? forecastLine(forecast) : " ";

  return (
    <div className="mini-cell">
      <RingGauge
        percent={value}
        expected={forecast ? plannedFor(forecast, showsUsed) : undefined}
        color={colour}
        markerColor={verdictColour}
      >
        <span style={{ color: colour }}>{Math.round(value)}%</span>
      </RingGauge>
      <div className="mini-cell-label">{cell.label}</div>
      <div
        className={`mini-cell-forecast${forecastFit(line)}`}
        style={verdictColour ? { color: verdictColour } : undefined}
      >
        {line}
      </div>
      <div className="mini-cell-reset">{formatRemaining(bucket.resetAt) || " "}</div>
    </div>
  );
}

/**
 * Which size class the forecast line needs to fit 62 pixels.
 *
 * Character count is a rough proxy — "may run out 5d 11h" and
 * "surplus · 92% left" are both eighteen characters and differ by five pixels
 * — so the steps are set from the widest wording at each length rather than
 * the average. The alternative is measuring after layout, which is more
 * machinery than a handful of bounded strings deserve.
 */
function forecastFit(line: string): string {
  if (line.length > 17) return " is-longest";
  if (line.length > 13) return " is-long";
  return "";
}

/** Where the same number is expected to be by now, in whichever direction the
 *  dial is drawn. */
function plannedFor(forecast: QuotaForecast, showsUsed: boolean): number {
  return showsUsed ? forecast.plannedUsedPercent : 100 - forecast.plannedUsedPercent;
}

/**
 * The one line a dial has room for.
 *
 * Terser than the popover's wording, which is the native mini's choice too:
 * under a 48-point ring there is space for a verdict or a number, rarely both.
 */
export function forecastLine(forecast: QuotaForecast, now = Date.now() / 1000): string {
  const runOut = formatRemaining(forecast.runOutAt, now);
  if (runOut) {
    return forecast.verdict === "watch" ? `may run out ${runOut}` : `out ${runOut}`;
  }
  const left = Math.round(Math.max(0, 100 - forecast.projectedUsedPercent));
  switch (forecast.verdict) {
    case "enough":
      return `${left}% left`;
    case "surplus":
      return `surplus · ${left}% left`;
    case "watch":
      return "watch";
    case "atRisk":
      return "risk";
    default:
      return `learning · ${left}% left`;
  }
}

/**
 * Resolve the chosen fields into the quota axis.
 *
 * Grouping is the point of this layout: a dial on its own says how full
 * something is but not what it is. All three levels come from the shared
 * naming contract, so a bucket sits under the same headings here as it does in
 * the native app.
 */
export function arrange(
  view: QuotaView,
  settings: PresentationSettings | null,
  fields: string[],
): Company[] {
  const companies: Company[] = [];
  const showsUsed = settings?.displayMode === "used";

  for (const field of fields) {
    const separator = field.indexOf(".");
    if (separator <= 0 || separator === field.length - 1) continue;
    const tool = field.slice(0, separator);
    const bucketId = field.slice(separator + 1);
    const bucket = view.accounts
      .filter((account) => account.tool === tool)
      .flatMap((account) => account.buckets)
      .find((candidate) => candidate.id === bucketId);
    if (!bucket) continue;

    const remaining = Math.max(0, 100 - bucket.usedPercent);
    const cell: Cell = {
      id: field,
      bucket,
      // Just the window. The group heading above already says Spark or Fable,
      // and repeating it under every dial costs the width the forecast line
      // needs. A custom label still wins, because the user chose it.
      label: settings?.customLabels[field] || bucket.title,
      value: showsUsed ? bucket.usedPercent : remaining,
      showsUsed,
    };

    const companyName = companyFor(tool);
    let company = companies.find((candidate) => candidate.name === companyName);
    if (!company) {
      company = { name: companyName, tool, subProviders: [] };
      companies.push(company);
    }
    const subProviderName = subProviderFor(tool, bucketId);
    let subProvider = company.subProviders.find((c) => c.name === subProviderName);
    if (!subProvider) {
      subProvider = { name: subProviderName, groups: [] };
      company.subProviders.push(subProvider);
    }
    const groupLabel = groupLabelFor(tool, bucketId, bucket.groupTitle);
    let group = subProvider.groups.find((candidate) => candidate.label === groupLabel);
    if (!group) {
      group = { label: groupLabel, cells: [] };
      subProvider.groups.push(group);
    }
    group.cells.push(cell);
  }
  return companies;
}
