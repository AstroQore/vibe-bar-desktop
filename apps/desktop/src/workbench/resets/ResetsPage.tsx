import { useEffect, useMemo, useState } from "react";
import type { PresentationSettings, QuotaCycle, QuotaView } from "../../api";
import { api, quotaBarColor } from "../../api";
import { ResetLane } from "../../popover/cards";
import { countdown, verdictColor } from "../../popover/format";
import { ChevronLeft, ChevronRight } from "../icons";
import {
  type CalendarEntry,
  type SubProviderCycle,
  calendarEntries,
  coarseNow,
  dayStartOf,
  entriesByDay,
  laneEvents,
  miniForecastLine,
  monthGrid,
  monthStartOf,
  monthTitle,
  remainingOf,
  riskRows,
  subDailyEvents,
  subProviderCycles,
} from "./model";
import "./resets.css";

const MAX_HISTORY_FETCHES = 24;

function Box({ title, detail, children, style }: { title?: string; detail?: string; children: React.ReactNode; style?: React.CSSProperties }) {
  return (
    <section className="wb-card rs-box" style={style} aria-label={title}>
      {title ? (
        <div className="rs-box-head">
          <span className="rs-box-title">{title}</span>
          {detail ? <span className="rs-box-detail">{detail}</span> : null}
        </div>
      ) : null}
      {children}
    </section>
  );
}

function CycleCard({ cycle, now }: { cycle: SubProviderCycle; now: number }) {
  const remaining = remainingOf(cycle.headline);
  const color = quotaBarColor(remaining, false);
  const others = cycle.buckets.filter((b) => b.id !== cycle.headline.id);
  const forecastColor = cycle.forecast ? (verdictColor(cycle.forecast.verdict) ?? "var(--wb-secondary)") : "var(--wb-tertiary)";
  return (
    <article className="wb-card rs-cycle">
      <div className="rs-cycle-head">
        <i style={{ background: color }} />
        <span className="rs-cycle-name">{cycle.name}</span>
        {cycle.accountLabel ? <span className="rs-cycle-account">{cycle.accountLabel}</span> : null}
        {cycle.plan ? <span className="rs-plan">{cycle.plan}</span> : null}
      </div>
      <div className="rs-cycle-big" style={{ color }}>
        {Math.round(remaining)}%
      </div>
      <div className="rs-cycle-line">
        {cycle.headline.title} · resets {countdown(cycle.headline.resetAt, now) ?? "—"}
      </div>
      <div className="rs-bar">
        <span style={{ width: `${Math.max(2, remaining)}%`, background: color }} />
      </div>
      {cycle.forecast ? (
        <div className="rs-cycle-forecast" style={{ color: forecastColor }}>
          {miniForecastLine(cycle.forecast, now)}
        </div>
      ) : null}
      {others.length > 0 ? (
        <div className="rs-cycle-buckets">
          {others.map((bucket) => {
            const left = remainingOf(bucket);
            return (
              <div className="rs-cycle-bucket" key={bucket.id}>
                <span className="rs-cycle-bucket-title">{bucket.title}</span>
                <span className="rs-cycle-bucket-left" style={{ color: quotaBarColor(left, false) }}>
                  {Math.round(left)}%
                </span>
                <span className="rs-cycle-bucket-when">{countdown(bucket.resetAt, now) ?? "—"}</span>
              </div>
            );
          })}
        </div>
      ) : null}
    </article>
  );
}

/** The Workbench Resets page — the native `ResetsPage`: the refill lane,
 *  one card per SubProvider cycle, the reset calendar, and run-out risk. */
export function ResetsPage({
  view,
  settings,
  now: fixedNow,
  history: fixedHistory,
}: {
  view: QuotaView;
  settings: PresentationSettings | null;
  now?: number;
  history?: Record<string, QuotaCycle[]>;
}) {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (fixedNow) return;
    const timer = setInterval(() => setTick((t) => t + 1), 60_000);
    return () => clearInterval(timer);
  }, [fixedNow]);
  const wall = fixedNow ?? Math.floor(Date.now() / 1000) + tick * 0;
  const now = coarseNow(wall);
  const [monthOffset, setMonthOffset] = useState(0);
  const [history, setHistory] = useState<Record<string, QuotaCycle[]>>(fixedHistory ?? {});

  const cycles = useMemo(() => subProviderCycles(view, settings, now), [view, settings, now]);
  const weekEvents = useMemo(() => laneEvents(view, settings, now, 7), [view, settings, now]);
  const dayEvents = useMemo(() => subDailyEvents(laneEvents(view, settings, now, 1)), [view, settings, now]);
  const risks = useMemo(() => riskRows(cycles), [cycles]);

  const keys = useMemo(
    () => cycles.flatMap((cycle) => cycle.buckets.map((bucket) => ({ accountId: cycle.accountId, bucketId: bucket.id }))).slice(0, MAX_HISTORY_FETCHES),
    [cycles],
  );
  useEffect(() => {
    if (fixedHistory) return;
    let cancelled = false;
    (async () => {
      const next: Record<string, QuotaCycle[]> = {};
      for (const key of keys) {
        try {
          const response = await api.quotaCycles(key.accountId, key.bucketId);
          next[`${key.accountId}:${key.bucketId}`] = response.completed;
        } catch {
          // No history is not an error surface: the calendar just shows what is scheduled.
        }
      }
      if (!cancelled) setHistory(next);
    })();
    return () => {
      cancelled = true;
    };
  }, [keys, fixedHistory]);

  const monthStart = monthStartOf(wall, monthOffset);
  const monthEnd = monthStartOf(wall, monthOffset + 1);
  const entries = useMemo(() => calendarEntries(view, settings, history, monthStart, monthEnd, wall), [view, settings, history, monthStart, monthEnd, wall]);
  const byDay = entriesByDay(entries, monthStart);
  const grid = monthGrid(monthStart);
  const today = dayStartOf(wall);
  const todayInMonth = today >= monthStart && today < monthEnd ? new Date(today * 1000).getDate() : null;

  return (
    <div className="rs-page">
      <Box title="Refill Horizon" detail="next 7 days · column height = how much comes back">
        {weekEvents.length > 0 ? (
          <ResetLane events={weekEvents} now={now} horizonDays={7} height={96} />
        ) : (
          <div className="rs-empty">No quota window resets within the next seven days.</div>
        )}
      </Box>
      {cycles.length > 0 ? (
        <div className="rs-grid">
          {cycles.map((cycle) => (
            <CycleCard key={cycle.id} cycle={cycle} now={now} />
          ))}
        </div>
      ) : (
        <Box>
          <div className="rs-empty">No quota windows with reset times yet. Refresh after signing in with a provider CLI.</div>
        </Box>
      )}
      <div className="rs-columns">
        <Box style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div className="rs-cal-head">
            <span className="rs-box-title">Reset Calendar</span>
            <span className="rs-spacer" />
            <button type="button" className="wb-iconbtn rs-cal-nav" title="Previous month" onClick={() => setMonthOffset((m) => m - 1)}>
              <ChevronLeft size={12} />
            </button>
            <span className="rs-cal-month">{monthTitle(monthStart)}</span>
            <button type="button" className="wb-iconbtn rs-cal-nav" title="Next month" onClick={() => setMonthOffset((m) => m + 1)}>
              <ChevronRight size={12} />
            </button>
            {monthOffset !== 0 ? (
              <button type="button" className="rs-today" onClick={() => setMonthOffset(0)}>
                Today
              </button>
            ) : null}
          </div>
          {dayEvents.length > 0 ? (
            <div className="rs-subdaily">
              <div className="rs-subdaily-label">Next 24 hours · sub-daily quotas</div>
              <ResetLane events={dayEvents} now={now} horizonDays={1} height={52} />
            </div>
          ) : null}
          <div className="rs-month" role="grid">
            {grid.weekdays.map((symbol) => (
              <div className="rs-weekday" key={symbol}>
                {symbol}
              </div>
            ))}
            {Array.from({ length: grid.leadingBlanks }, (_, i) => (
              <div className="rs-day blank" key={`blank-${i}`} />
            ))}
            {Array.from({ length: grid.dayCount }, (_, i) => i + 1).map((day) => {
              const dayEntries: CalendarEntry[] = byDay.get(day) ?? [];
              return (
                <div className={`rs-day${todayInMonth === day ? " today" : ""}`} key={day} role="gridcell">
                  <div className="rs-day-number">{day}</div>
                  {dayEntries.slice(0, 3).map((entry) => (
                    <div className={`rs-entry ${entry.kind}`} key={entry.id} title={entry.label}>
                      <i style={{ background: quotaBarColor(100 - entry.gainPercent, false) }} />
                      <span>
                        {entry.shortLabel} +{Math.round(entry.gainPercent)}%
                      </span>
                    </div>
                  ))}
                  {dayEntries.length > 3 ? <div className="rs-entry more">+{dayEntries.length - 3} more</div> : null}
                </div>
              );
            })}
          </div>
        </Box>
        <Box title="Run-out Risk" detail="ranked by the personal forecast" style={{ flex: "0 0 320px", width: 320 }}>
          {risks.length === 0 ? (
            <div className="rs-empty">Every bucket is projected to last its cycle.</div>
          ) : (
            <div className="rs-risks">
              {risks.map((row) => (
                <div className="rs-risk" key={`${row.cycle.id}/${row.bucket.id}`}>
                  <span className="rs-risk-badge" style={{ background: quotaBarColor(row.remaining, false) }}>
                    {row.badge}
                  </span>
                  <span className="rs-risk-body">
                    <span className="rs-risk-name">
                      {row.cycle.name}
                      {row.cycle.accountLabel ? ` · ${row.cycle.accountLabel}` : ""}
                      {row.bucket.id === row.cycle.headline.id ? "" : ` · ${row.bucket.title}`}
                    </span>
                    <span className="rs-risk-detail">
                      {Math.round(row.remaining)}% · refills {countdown(row.bucket.resetAt, now) ?? "—"}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          )}
        </Box>
      </div>
    </div>
  );
}
