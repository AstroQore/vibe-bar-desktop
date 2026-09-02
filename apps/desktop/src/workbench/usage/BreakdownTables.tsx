import { useEffect, useState } from "react";
import type { GroupStat, RequestRow, TrendBucket, TrendPoint } from "../../api";
import {
  BREAKDOWNS,
  type Breakdown,
  compactTokens,
  countSummary,
  formatMicroUSD,
  groupedNumber,
  periodTitle,
  populatedPeriods,
  timestamp,
} from "./model";

const PERIOD_PAGE = 120;

function cost(micros: number | null): string {
  return micros == null ? "—" : formatMicroUSD(micros);
}

/** The native `UsageBreakdownTables`: one card, four tabs, the same columns. */
export function BreakdownTables({
  bucket,
  points,
  requests,
  totalRequests,
  providers,
  models,
  onLoadMore,
  loadingMore,
}: {
  bucket: TrendBucket;
  points: TrendPoint[];
  requests: RequestRow[];
  totalRequests: number;
  providers: GroupStat[];
  models: GroupStat[];
  onLoadMore: () => void;
  loadingMore: boolean;
}) {
  const [active, setActive] = useState<Breakdown>("periods");
  const [periodLimit, setPeriodLimit] = useState(PERIOD_PAGE);
  const periods = populatedPeriods(points);
  useEffect(() => setPeriodLimit(PERIOD_PAGE), [bucket, points.length]);
  const summary = countSummary(active, {
    periods: periods.length,
    bucket,
    loadedRequests: requests.length,
    totalRequests,
    providers: providers.length,
    models: models.length,
  });
  const empty = <div className="us-table-empty">No usage recorded in this range</div>;
  return (
    <section className="wb-card us-breakdown" aria-label="Breakdown">
      <div className="us-breakdown-head">
        <div className="us-tabs" role="tablist">
          {BREAKDOWNS.map((tab) => (
            <button type="button" role="tab" key={tab.id} aria-selected={active === tab.id} className={`us-tab${active === tab.id ? " on" : ""}`} onClick={() => setActive(tab.id)}>
              {tab.title}
            </button>
          ))}
        </div>
        <span className="us-count">{summary}</span>
      </div>
      <div className="us-table-scroll">
        {active === "periods" ? (
          periods.length === 0 ? (
            empty
          ) : (
            <table className="wb-table us-table">
              <thead>
                <tr>
                  <th>{bucket === "hour" ? "Hour" : bucket === "day" ? "Day" : "Week of"}</th>
                  <th className="num">Input</th>
                  <th className="num">Output</th>
                  <th className="num">Cache</th>
                  <th className="num">Tokens</th>
                  <th className="num">Cost</th>
                </tr>
              </thead>
              <tbody>
                {periods.slice(0, periodLimit).map((point) => (
                  <tr key={point.bucketStart}>
                    <td>{periodTitle(bucket, point.bucketStart)}</td>
                    <td className="num">{compactTokens(point.freshInput)}</td>
                    <td className="num">{compactTokens(point.output)}</td>
                    <td className="num">{compactTokens(point.cacheRead + point.cacheCreation)}</td>
                    <td className="num">{compactTokens(point.totalTokens)}</td>
                    <td className="num">{formatMicroUSD(point.costMicros)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        ) : null}
        {active === "periods" && periods.length > periodLimit ? (
          <button type="button" className="us-more" onClick={() => setPeriodLimit((limit) => limit + PERIOD_PAGE)}>
            Show {Math.min(PERIOD_PAGE, periods.length - periodLimit)} more
          </button>
        ) : null}
        {active === "requests" ? (
          requests.length === 0 ? (
            empty
          ) : (
            <>
              <table className="wb-table us-table us-requests">
                <thead>
                  <tr>
                    <th>Time</th>
                    <th>Harness</th>
                    <th>Model</th>
                    <th>Tier</th>
                    <th className="num">Input</th>
                    <th className="num">Output</th>
                    <th className="num">Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {requests.map((row, index) => (
                    <tr key={`${row.time}-${index}`}>
                      <td className="us-mono">{timestamp(row.time)}</td>
                      <td>{row.harness}</td>
                      <td className="us-model" title={row.model}>
                        {row.model}
                      </td>
                      <td className="us-tier">{row.tier ?? "—"}</td>
                      <td className="num" title={`${groupedNumber(row.freshInput)} fresh · ${groupedNumber(row.cacheRead)} cache read · ${groupedNumber(row.cacheCreation)} cache write`}>
                        {compactTokens(row.freshInput + row.cacheRead + row.cacheCreation)}
                      </td>
                      <td className="num">{compactTokens(row.output)}</td>
                      <td className="num">{cost(row.costMicros)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {requests.length < totalRequests ? (
                <button type="button" className="us-more" onClick={onLoadMore} disabled={loadingMore}>
                  {loadingMore ? "Loading…" : `Load more (${groupedNumber(totalRequests - requests.length)} remaining)`}
                </button>
              ) : null}
            </>
          )
        ) : null}
        {active === "providers" ? (
          providers.length === 0 ? (
            empty
          ) : (
            <table className="wb-table us-table">
              <thead>
                <tr>
                  <th>Provider</th>
                  <th className="num">Requests</th>
                  <th className="num">Tokens</th>
                  <th className="num">Cost</th>
                </tr>
              </thead>
              <tbody>
                {providers.map((row) => (
                  <tr key={row.name}>
                    <td>{row.name}</td>
                    <td className="num">{groupedNumber(row.requests)}</td>
                    <td className="num">{compactTokens(row.totalTokens)}</td>
                    <td className="num">
                      {formatMicroUSD(row.costMicros)}
                      {row.unpricedRequests > 0 ? <span className="us-unpriced inline">{row.unpricedRequests} unpriced</span> : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        ) : null}
        {active === "models" ? (
          models.length === 0 ? (
            empty
          ) : (
            <table className="wb-table us-table">
              <thead>
                <tr>
                  <th>Model</th>
                  <th className="num">Requests</th>
                  <th className="num">Tokens</th>
                  <th className="num">Cost</th>
                  <th className="num">Avg/req</th>
                </tr>
              </thead>
              <tbody>
                {models.map((row) => (
                  <tr key={row.name}>
                    <td className="us-model" title={row.name}>
                      {row.name}
                    </td>
                    <td className="num">{groupedNumber(row.requests)}</td>
                    <td className="num">{compactTokens(row.totalTokens)}</td>
                    <td className="num">
                      {formatMicroUSD(row.costMicros)}
                      {row.unpricedRequests > 0 ? <span className="us-unpriced inline">{row.unpricedRequests} unpriced</span> : null}
                    </td>
                    <td className="num">{row.requests > 0 ? compactTokens(Math.round(row.totalTokens / row.requests)) : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        ) : null}
      </div>
    </section>
  );
}
