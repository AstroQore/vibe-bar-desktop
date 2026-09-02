import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { TrendBucket, TrendPoint, TrendSeries } from "../../api";
import { ChartBar, ChevronLeft, ChevronRight, Dollar, Refresh, Sigma } from "../icons";
import { Menu, MenuItem } from "./Menu";
import {
  GRANULARITY_OPTIONS,
  KIND_COLORS,
  KIND_LABELS,
  WINDOW_SPANS,
  type ChartWindow,
  axisLabel,
  bucketDescription,
  bucketSeconds,
  clampWindow,
  compactTokens,
  compactUSD,
  formatMicroUSD,
  granularityTitle,
  paletteColor,
  shortDate,
  shortDateTime,
} from "./model";

type Metric = "tokens" | "cost";
type Kind = keyof typeof KIND_COLORS;
const KINDS: Kind[] = ["input", "output", "cacheWrite", "cacheRead"];
const CHART_HEIGHT = 190;
const BRUSH_HEIGHT = 44;
const AXIS_WIDTH = 46;
const AXIS_HEIGHT = 18;
const DEFAULT_VISIBLE_BUCKETS = 60;

function kindValue(point: TrendPoint, kind: Kind): number {
  switch (kind) {
    case "input":
      return point.freshInput;
    case "output":
      return point.output;
    case "cacheWrite":
      return point.cacheCreation;
    case "cacheRead":
      return point.cacheRead;
  }
}

function useWidth<T extends HTMLElement>(): [React.RefObject<T>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    setWidth(node.getBoundingClientRect().width);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setWidth(entry.contentRect.width);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

function bucketTitle(bucket: TrendBucket, start: number): string {
  if (bucket === "hour") return shortDateTime(start);
  if (bucket === "day") return shortDate(start);
  return `Week of ${shortDate(start)}`;
}

/** The native `UsageTrendChartView`: stacked token kinds (or cost) over the
 *  visible window, with a brush navigator over the whole range beneath. */
export function TrendChart({
  trend,
  granularity,
  available,
  onGranularity,
}: {
  trend: TrendSeries;
  granularity: TrendBucket | null;
  available: { hour: boolean; day: boolean; week: boolean };
  onGranularity: (bucket: TrendBucket | null) => void;
}) {
  const [metric, setMetric] = useState<Metric>("tokens");
  const [hiddenProviders, setHiddenProviders] = useState<Set<string>>(new Set());
  const [window, setWindow] = useState<ChartWindow | null>(null);
  const points = trend.points;
  const step = bucketSeconds(trend.bucket);
  const domain = useMemo<ChartWindow>(() => {
    if (points.length === 0) return { start: 0, end: 0 };
    return { start: points[0].bucketStart, end: points[points.length - 1].bucketStart + step };
  }, [points, step]);
  const domainKey = `${domain.start}-${domain.end}-${trend.bucket}`;
  useEffect(() => setWindow(null), [domainKey]);
  const view = useMemo<ChartWindow>(() => {
    if (window) return clampWindow(window, domain, trend.bucket);
    const span = Math.min(domain.end - domain.start, DEFAULT_VISIBLE_BUCKETS * step);
    return clampWindow({ start: domain.end - span, end: domain.end }, domain, trend.bucket);
  }, [window, domain, step, trend.bucket]);
  const visible = useMemo(() => points.filter((p) => p.bucketStart >= view.start && p.bucketStart < view.end), [points, view]);
  const hasData = points.some((p) => p.totalTokens > 0 || p.requests > 0);
  const visibleTokens = visible.reduce((acc, p) => acc + p.totalTokens, 0);
  const visibleCost = visible.reduce((acc, p) => acc + p.costMicros, 0);
  const providers = trend.providers.filter((p) => p.points.some((point) => point.totalTokens > 0));

  const shift = (direction: -1 | 1) => {
    const span = view.end - view.start;
    setWindow(clampWindow({ start: view.start + direction * span, end: view.end + direction * span }, domain, trend.bucket));
  };
  const jumpNow = () => {
    const span = view.end - view.start;
    setWindow(clampWindow({ start: domain.end - span, end: domain.end }, domain, trend.bucket));
  };
  const setSpan = (seconds: number) => setWindow(clampWindow({ start: view.end - seconds, end: view.end }, domain, trend.bucket));

  return (
    <section className="wb-card us-trend" aria-label="Usage over time">
      <div className="us-trend-head">
        <div className="us-trend-lead">
          <div>
            <div className="us-hero-label">Usage over time</div>
            <div className="us-trend-sub">{bucketDescription(trend.bucket)}</div>
          </div>
          <div className="us-segmented" role="radiogroup" aria-label="Chart metric">
            {(["tokens", "cost"] as Metric[]).map((value) => (
              <button
                type="button"
                key={value}
                role="radio"
                aria-checked={metric === value}
                className={`us-segment${metric === value ? " on" : ""}`}
                onClick={() => setMetric(value)}
              >
                {value === "tokens" ? <Sigma size={11} /> : <Dollar size={11} />}
                {value === "tokens" ? "Tokens" : "Cost"}
              </button>
            ))}
          </div>
          <Menu icon={<ChartBar size={12} />} title={granularityTitle(granularity)} detail="" caps={false} ariaLabel="Choose chart granularity" width={150}>
            {(close) => (
              <>
                {GRANULARITY_OPTIONS.map((option) => (
                  <MenuItem
                    key={option.title}
                    checked={granularity === option.id}
                    disabled={option.id !== null && !available[option.id]}
                    onSelect={() => {
                      onGranularity(option.id);
                      close();
                    }}
                  >
                    {option.title}
                  </MenuItem>
                ))}
              </>
            )}
          </Menu>
        </div>
        <div className="us-window-tools">
          <button type="button" className="us-window-btn" title="Previous window" onClick={() => shift(-1)} disabled={view.start <= domain.start}>
            <ChevronLeft size={13} />
          </button>
          <button type="button" className="us-window-btn" title="Next window" onClick={() => shift(1)} disabled={view.end >= domain.end}>
            <ChevronRight size={13} />
          </button>
          <button type="button" className="us-window-btn us-now" title="Return to current window" onClick={jumpNow} disabled={view.end >= domain.end}>
            <Refresh size={11} /> Now
          </button>
        </div>
      </div>
      {hasData ? (
        <>
          <div className="us-legend">
            {metric === "tokens"
              ? KINDS.map((kind) => (
                  <span className="us-legend-item" key={kind}>
                    <i style={{ background: KIND_COLORS[kind] }} />
                    {KIND_LABELS[kind]}
                  </span>
                ))
              : null}
            {providers.length > 1 || metric === "cost" ? (
              <span className="us-legend-providers">
                {providers.map((provider, index) => {
                  const hidden = hiddenProviders.has(provider.harness);
                  return (
                    <button
                      type="button"
                      key={provider.harness}
                      className={`us-legend-item us-legend-toggle${hidden ? " off" : ""}`}
                      title={hidden ? `Show ${provider.harness}` : `Hide ${provider.harness}`}
                      onClick={() =>
                        setHiddenProviders((current) => {
                          const next = new Set(current);
                          if (next.has(provider.harness)) next.delete(provider.harness);
                          else next.add(provider.harness);
                          return next;
                        })
                      }
                    >
                      <i style={{ background: paletteColor(index) }} />
                      {provider.harness}
                    </button>
                  );
                })}
              </span>
            ) : null}
          </div>
          <MainChart points={visible} bucket={trend.bucket} metric={metric} />
          <Navigator points={points} providers={providers} hidden={hiddenProviders} domain={domain} view={view} bucket={trend.bucket} onWindow={setWindow} />
          <div className="us-scope">
            <span className="us-scope-note" title={`${compactTokens(visibleTokens)} tokens, ${formatMicroUSD(visibleCost)} in view`}>
              Drag the navigator handles to focus this chart; filters and tables keep the full window.
            </span>
            <span className="us-spans">
              {WINDOW_SPANS.filter((span) => span.seconds <= domain.end - domain.start && span.seconds >= 2 * step).map((span) => (
                <button
                  type="button"
                  key={span.title}
                  className={`us-span${Math.abs(view.end - view.start - span.seconds) < 1 ? " on" : ""}`}
                  onClick={() => setSpan(span.seconds)}
                >
                  {span.title}
                </button>
              ))}
            </span>
          </div>
        </>
      ) : (
        <div className="us-chart-empty" style={{ height: CHART_HEIGHT }}>
          No usage recorded in this range
        </div>
      )}
    </section>
  );
}

function MainChart({ points, bucket, metric }: { points: TrendPoint[]; bucket: TrendBucket; metric: Metric }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const plotWidth = Math.max(0, width - AXIS_WIDTH);
  const plotHeight = CHART_HEIGHT - AXIS_HEIGHT;
  const value = (point: TrendPoint) => (metric === "tokens" ? point.totalTokens : point.costMicros);
  const max = Math.max(1, ...points.map(value));
  const slot = points.length > 0 ? plotWidth / points.length : 0;
  const gap = Math.min(4, slot * 0.25);
  const barWidth = Math.max(1, slot - gap);
  const ticks = [0, 0.5, 1];
  const tickEvery = Math.max(1, Math.ceil(points.length / 8));
  const format = metric === "tokens" ? compactTokens : compactUSD;
  const hovered = hover != null ? points[hover] : null;
  return (
    <div
      className="us-chart"
      ref={ref}
      style={{ height: CHART_HEIGHT }}
      onMouseMove={(event) => {
        const rect = event.currentTarget.getBoundingClientRect();
        const x = event.clientX - rect.left - AXIS_WIDTH;
        if (x < 0 || slot === 0) return setHover(null);
        setHover(Math.min(points.length - 1, Math.max(0, Math.floor(x / slot))));
      }}
      onMouseLeave={() => setHover(null)}
    >
      <svg width={width} height={CHART_HEIGHT} role="img" aria-label={`${metric} per ${bucket}`}>
        {ticks.map((tick) => {
          const y = plotHeight - tick * (plotHeight - 6);
          return (
            <g key={tick}>
              <line x1={AXIS_WIDTH} x2={width} y1={y} y2={y} stroke="var(--wb-hairline)" strokeWidth="0.5" strokeDasharray={tick === 0 ? undefined : "2 3"} />
              <text x={AXIS_WIDTH - 6} y={y + 3} textAnchor="end" className="us-axis">
                {format(Math.round(tick * max))}
              </text>
            </g>
          );
        })}
        {points.map((point, index) => {
          const x = AXIS_WIDTH + index * slot + gap / 2;
          const total = value(point);
          const height = (total / max) * (plotHeight - 6);
          let y = plotHeight;
          const segments =
            metric === "tokens"
              ? KINDS.map((kind) => {
                  const h = (kindValue(point, kind) / max) * (plotHeight - 6);
                  y -= h;
                  return <rect key={kind} x={x} y={y} width={barWidth} height={h} fill={KIND_COLORS[kind]} opacity={hover == null || hover === index ? 1 : 0.55} />;
                })
              : [<rect key="cost" x={x} y={plotHeight - height} width={barWidth} height={height} fill="var(--wb-accent)" rx={1.5} opacity={hover == null || hover === index ? 1 : 0.55} />];
          return (
            <g key={point.bucketStart}>
              {segments}
              {index % tickEvery === 0 ? (
                <text x={x + barWidth / 2} y={CHART_HEIGHT - 4} textAnchor="middle" className="us-axis">
                  {axisLabel(bucket, point.bucketStart)}
                </text>
              ) : null}
            </g>
          );
        })}
        {hovered ? <line x1={AXIS_WIDTH + hover! * slot + slot / 2} x2={AXIS_WIDTH + hover! * slot + slot / 2} y1={0} y2={plotHeight} stroke="var(--wb-secondary)" strokeWidth="0.75" strokeDasharray="3 3" /> : null}
      </svg>
      {hovered ? (
        <div className="us-tooltip" style={{ left: Math.min(width - 190, AXIS_WIDTH + hover! * slot + slot / 2 + 8) }}>
          <div className="us-tooltip-title">{bucketTitle(bucket, hovered.bucketStart)}</div>
          {KINDS.map((kind) => (
            <div className="us-tooltip-row" key={kind}>
              <i style={{ background: KIND_COLORS[kind] }} />
              <span>{KIND_LABELS[kind]}</span>
              <b>{compactTokens(kindValue(hovered, kind))}</b>
            </div>
          ))}
          <div className="us-tooltip-row total">
            <span>{hovered.requests} req</span>
            <b>{formatMicroUSD(hovered.costMicros)}</b>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Navigator({
  points,
  providers,
  hidden,
  domain,
  view,
  bucket,
  onWindow,
}: {
  points: TrendPoint[];
  providers: TrendSeries["providers"];
  hidden: Set<string>;
  domain: ChartWindow;
  view: ChartWindow;
  bucket: TrendBucket;
  onWindow: (window: ChartWindow) => void;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const span = Math.max(1, domain.end - domain.start);
  const toX = (at: number) => ((at - domain.start) / span) * width;
  const fromX = (x: number) => domain.start + (x / Math.max(1, width)) * span;
  const max = Math.max(1, ...points.map((p) => p.totalTokens));
  const series = providers.length > 0 ? providers.filter((p) => !hidden.has(p.harness)) : [{ harness: "all", company: "", points }];
  const step = bucketSeconds(bucket);
  const drag = useRef<{ mode: "move" | "start" | "end"; originX: number; origin: ChartWindow } | null>(null);

  const onPointerDown = (mode: "move" | "start" | "end") => (event: React.PointerEvent) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { mode, originX: event.clientX, origin: view };
  };
  const onPointerMove = (event: React.PointerEvent) => {
    const state = drag.current;
    if (!state) return;
    const delta = fromX(event.clientX - state.originX) - domain.start;
    const next =
      state.mode === "move"
        ? { start: state.origin.start + delta, end: state.origin.end + delta }
        : state.mode === "start"
          ? { start: state.origin.start + delta, end: state.origin.end }
          : { start: state.origin.start, end: state.origin.end + delta };
    onWindow(clampWindow(next, domain, bucket));
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const left = toX(view.start);
  const right = toX(view.end);
  return (
    <div className="us-nav" ref={ref} style={{ height: BRUSH_HEIGHT }} role="slider" aria-label="Usage chart range navigator" aria-valuetext={`${shortDateTime(view.start)} – ${shortDateTime(view.end)}`}>
      <svg width={width} height={BRUSH_HEIGHT} aria-hidden="true">
        {series.map((provider, index) => {
          const path = provider.points
            .map((point, i) => `${i === 0 ? "M" : "L"}${toX(point.bucketStart + step / 2).toFixed(1)},${(BRUSH_HEIGHT - 2 - (point.totalTokens / max) * (BRUSH_HEIGHT - 6)).toFixed(1)}`)
            .join(" ");
          return <path key={provider.harness} d={path} fill="none" stroke={providers.length > 0 ? paletteColor(index) : "var(--wb-accent)"} strokeWidth="1.2" opacity="0.85" />;
        })}
      </svg>
      <div className="us-brush-shade" style={{ left: 0, width: left }} />
      <div className="us-brush-shade" style={{ left: right, width: Math.max(0, width - right) }} />
      <div className="us-brush" style={{ left, width: Math.max(8, right - left) }} onPointerDown={onPointerDown("move")} onPointerMove={onPointerMove} onPointerUp={onPointerUp}>
        <span className="us-brush-handle start" onPointerDown={onPointerDown("start")} onPointerMove={onPointerMove} onPointerUp={onPointerUp} />
        <span className="us-brush-handle end" onPointerDown={onPointerDown("end")} onPointerMove={onPointerMove} onPointerUp={onPointerUp} />
      </div>
    </div>
  );
}
