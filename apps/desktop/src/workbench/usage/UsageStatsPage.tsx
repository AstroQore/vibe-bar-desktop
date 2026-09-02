import { useCallback, useEffect, useRef, useState } from "react";
import type { UsageStatsView } from "../../api";
import { api } from "../../api";
import { BreakdownTables } from "./BreakdownTables";
import { Distribution } from "./Distribution";
import { FiltersBar, type FilterState } from "./FiltersBar";
import { HeroCards } from "./HeroCards";
import { TrendChart } from "./TrendChart";
import { buildQuery } from "./model";
import "./usage.css";

const REQUEST_PAGE = 200;
/** Auto-refresh re-queries the retained ledger every tick; a rescan of the
 *  JSONL sources is the expensive part, so it happens at most this often. */
const RESCAN_SECONDS = 60;

const DEFAULT_FILTERS: FilterState = {
  preset: "day7",
  harnesses: null,
  models: null,
  granularity: null,
  refreshInterval: 0,
};

/** The Workbench's Usage Stats page — the native `UsageStatsPage` composition.
 *  `fixture` renders a synthetic view for the preview page without Tauri. */
export function UsageStatsPage({ refreshToken = 0, fixture, now: fixedNow }: { refreshToken?: number; fixture?: UsageStatsView; now?: number }) {
  const [filters, setFilters] = useState<FilterState>(DEFAULT_FILTERS);
  const [requestLimit, setRequestLimit] = useState(REQUEST_PAGE);
  const [view, setView] = useState<UsageStatsView | null>(fixture ?? null);
  const [loading, setLoading] = useState(!fixture);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const lastScan = useRef(0);

  const load = useCallback(
    async (rescan: boolean) => {
      if (fixture) return;
      const id = ++generation.current;
      setLoading(true);
      try {
        const now = Date.now() / 1000;
        if (rescan && now - lastScan.current > RESCAN_SECONDS) {
          lastScan.current = now;
          await api.refreshCost();
        }
        const next = await api.usageStats(buildQuery({ ...filters, requestLimit, now }));
        if (id !== generation.current) return;
        setView(next);
        setError(null);
      } catch (cause) {
        if (id !== generation.current) return;
        setError(String(cause));
      } finally {
        if (id === generation.current) setLoading(false);
      }
    },
    [filters, requestLimit, fixture],
  );

  useEffect(() => {
    void load(false);
  }, [load]);
  useEffect(() => {
    // The header's refresh has just rescanned the ledger; re-query only, and
    // count that scan so an auto tick does not scan again straight away.
    if (refreshToken > 0) {
      lastScan.current = Date.now() / 1000;
      void load(false);
    }
  }, [refreshToken, load]);
  useEffect(() => {
    if (filters.refreshInterval === 0 || fixture) return;
    const timer = setInterval(() => void load(true), filters.refreshInterval * 1000);
    return () => clearInterval(timer);
  }, [filters.refreshInterval, load, fixture]);

  const now = fixedNow ?? Date.now() / 1000;
  const noHarness = filters.harnesses !== null && filters.harnesses.length === 0;

  return (
    <div className="us-page">
      <FiltersBar
        state={filters}
        view={view}
        now={now}
        onChange={(next) => {
          setRequestLimit(REQUEST_PAGE);
          setFilters(next);
        }}
      />
      <div className="us-divider" />
      <div className="us-scroll">
        <div className="us-body">
          {view && !view.ledgerAvailable ? (
            <section className="wb-card us-notice">
              <div className="us-donut-title">Usage ledger unavailable</div>
              <p>
                {view.privacySuppressed
                  ? "Cost privacy mode is on, so no local session files were read. Turn it off in Settings › Cost Data to see per-request usage."
                  : "No usage ledger could be read from this Mac."}
              </p>
            </section>
          ) : noHarness ? (
            <section className="wb-card us-notice">
              <div className="us-donut-title">No harness selected</div>
              <p>Pick a harness above, or click All harnesses to include every one.</p>
            </section>
          ) : view ? (
            <>
              <HeroCards summary={view.summary} />
              <TrendChart trend={view.trend} granularity={filters.granularity} available={view.granularity} onGranularity={(granularity) => setFilters({ ...filters, granularity })} />
              <Distribution summary={view.summary} harnesses={view.harnesses} providers={view.providers} models={view.models} />
              <BreakdownTables
                bucket={view.trend.bucket}
                points={view.trend.points}
                requests={view.requests}
                totalRequests={view.totalRequests}
                providers={view.providers}
                models={view.models}
                loadingMore={loading}
                onLoadMore={() => setRequestLimit((limit) => limit + REQUEST_PAGE)}
              />
            </>
          ) : error ? (
            <section className="wb-card us-notice">
              <div className="us-donut-title">Usage ledger unavailable</div>
              <p>{error}</p>
            </section>
          ) : (
            <p className="wb-empty">Reading the local ledger…</p>
          )}
        </div>
      </div>
    </div>
  );
}
