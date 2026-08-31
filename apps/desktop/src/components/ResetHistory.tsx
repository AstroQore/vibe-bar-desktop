import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { providerAccent } from "../tokens";
import { useDarkMode } from "../theme";

/// One inferred quota cycle, as `quota_cycles` returns it.
export interface QuotaCycle {
  windowEnd: number;
  windowStart?: number;
  peakUsedPercent: number;
  lastUsedPercent: number;
  observationCount: number;
  firstSeenAt: number;
  lastSeenAt: number;
  completion?: "refillDetected" | "scheduledReset";
}

interface ResetHistoryResponse {
  completed: QuotaCycle[];
  current: QuotaCycle | null;
}

/** The native chart shows twelve bars; more than that and each one is a hair. */
const MAX_CYCLES = 12;
const CHART_HEIGHT = 52;
const BAR_GAP = 3;
/** Below this a bar would vanish; native keeps the same 4% floor. */
const MIN_BAR_FRACTION = 0.04;

/// The one provider colour table, same as every other surface. Two accents
/// resolve per appearance, so this needs to know which one is showing.
///
/// The fallback is unreachable for a real provider — a contract test asserts
/// every tool has an accent — and exists only so a malformed id draws
/// something rather than nothing.
export function accentFor(tool: string, dark: boolean): string {
  return providerAccent(tool, dark) ?? "#738CA6";
}

function remainingAtReset(cycle: QuotaCycle): number {
  return Math.max(0, 100 - cycle.peakUsedPercent);
}

function dayLabel(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

function timestampLabel(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  const day = date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  const time = date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  return `${day} at ${time}`;
}

/// The gap between the last observation and the inferred reset, when it is
/// wide enough to be worth admitting: a cycle last seen at 40% six hours
/// before it reset may well have gone higher unobserved.
function samplingGap(cycle: QuotaCycle): string {
  const gap = cycle.windowEnd - cycle.lastSeenAt;
  if (!Number.isFinite(gap) || gap < 60) return "";
  const hours = Math.floor(gap / 3600);
  const minutes = Math.round((gap % 3600) / 60);
  const spelled = hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
  return ` · last seen ${spelled} before reset`;
}

interface Props {
  accountId: string;
  bucketId: string;
  tool: string;
  /** `used` fills each bar by how much was consumed, `remaining` by what was
   *  left — the same choice the quota bar above the chart is drawn with. */
  mode: "used" | "remaining";
  /** The forecast's safety target as *percent remaining*, drawn as a dashed
   *  line to compare past cycles against. Taken in one orientation and flipped
   *  here to match `mode`, because a caller passing the already-flipped value
   *  would draw "10% left" as "10% used" with nothing to catch it. Omitted
   *  when there is no forecast to take a target from. */
  targetRemainingPercent?: number;
}

/**
 * Reset-cycle history: each bar is one quota cycle, answering how much was
 * still unused when the provider refilled it. The final outlined bar is the
 * cycle in progress.
 *
 * Data comes from the observation store a refresh already wrote, so this
 * renders whatever history exists without triggering any network call.
 */
export function ResetHistory({
  accountId,
  bucketId,
  tool,
  mode,
  targetRemainingPercent,
}: Props) {
  const [history, setHistory] = useState<ResetHistoryResponse | null>(null);
  const [hovered, setHovered] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    invoke<ResetHistoryResponse>("quota_cycles", { accountId, bucketId })
      .then((value) => {
        if (live) setHistory(value);
      })
      // A chart is not worth an error surface: no history simply means no
      // chart, which is what a fresh install sees anyway.
      .catch(() => {
        if (live) setHistory({ completed: [], current: null });
      });
    return () => {
      live = false;
    };
  }, [accountId, bucketId]);

  const cycles = useMemo(() => {
    if (!history) return [];
    const all = [...history.completed];
    if (history.current) all.push(history.current);
    return all.slice(-MAX_CYCLES);
  }, [history]);

  const dark = useDarkMode();
  const accent = accentFor(tool, dark);
  const target =
    targetRemainingPercent === undefined
      ? undefined
      : mode === "used"
        ? 100 - targetRemainingPercent
        : targetRemainingPercent;

  return (
    <div className="reset-history">
      <div className="reset-history-head">
        <span className="reset-history-title">Reset history</span>
        <span className="reset-history-hint">Each bar is one quota cycle</span>
      </div>
      {cycles.length === 0 ? (
        <div className="reset-history-empty" style={{ height: CHART_HEIGHT }}>
          {history === null ? "" : "Waiting for the first quota observation"}
        </div>
      ) : (
        <div
          className="reset-history-strip"
          style={{ height: CHART_HEIGHT, gap: BAR_GAP }}
          onMouseLeave={() => setHovered(null)}
        >
          {cycles.map((cycle, index) => {
            const percent = mode === "used" ? cycle.peakUsedPercent : remainingAtReset(cycle);
            const fraction = Math.max(MIN_BAR_FRACTION, percent / 100);
            const open = cycle.completion === undefined;
            return (
              <div
                key={`${cycle.windowEnd}-${index}`}
                className={`reset-history-bar${open ? " is-open" : ""}`}
                style={open ? { borderColor: accent } : undefined}
                onMouseEnter={() => setHovered(index)}
              >
                <div
                  className="reset-history-fill"
                  style={{
                    height: `${fraction * 100}%`,
                    background: accent,
                    opacity: hovered === index ? 1 : 0.86,
                  }}
                />
              </div>
            );
          })}
          {target !== undefined && target > 3 && target < 97 && (
            <div className="reset-history-target" style={{ bottom: `${target}%` }} aria-hidden />
          )}
        </div>
      )}
      <div className="reset-history-caption">{caption(cycles, hovered)}</div>
      {cycles.length > 0 && (
        <div className="reset-history-axis">
          <span>{dayLabel(axisDate(cycles[0]))}</span>
          {cycles.length > 2 && <span>{dayLabel(axisDate(cycles[cycles.length >> 1]))}</span>}
          <span>
            {cycles[cycles.length - 1].completion === undefined
              ? "Current"
              : dayLabel(axisDate(cycles[cycles.length - 1]))}
          </span>
        </div>
      )}
    </div>
  );
}

function axisDate(cycle: QuotaCycle): number {
  return cycle.completion === undefined ? cycle.lastSeenAt : cycle.windowEnd;
}

/// The hovered cycle, or the most recent one when nothing is hovered — so the
/// caption always says something rather than appearing on hover.
export function caption(cycles: QuotaCycle[], hovered: number | null): string {
  if (cycles.length === 0) return "A cycle is recorded when the quota refills";
  const index = hovered === null ? cycles.length - 1 : Math.min(Math.max(0, hovered), cycles.length - 1);
  const cycle = cycles[index];
  const used = Math.round(cycle.peakUsedPercent);
  const left = Math.round(remainingAtReset(cycle));
  if (cycle.completion === undefined) {
    return `Current cycle · ${used}% used so far · ${left}% left`;
  }
  return `${timestampLabel(cycle.windowEnd)} reset · ${used}% used · ${left}% left${samplingGap(cycle)}`;
}
