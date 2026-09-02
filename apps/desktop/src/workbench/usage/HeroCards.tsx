import type { UsageSummary } from "../../api";
import { KIND_COLORS, compactTokens, formatMicroUSD, formatPercent, groupedNumber } from "./model";

function Section({ label, value, detail, className }: { label: string; value: string; detail?: React.ReactNode; className?: string }) {
  return (
    <div className={`us-hero-section ${className ?? ""}`}>
      <div className="us-hero-label">{label}</div>
      <div className="us-hero-value">{value}</div>
      {detail ? <div className="us-hero-detail">{detail}</div> : null}
    </div>
  );
}

/** The native `UsageHeroCards`: real tokens with their composition, total
 *  cost, requests, and the cache-hit ring, in one card. */
export function HeroCards({ summary }: { summary: UsageSummary }) {
  const total = summary.totalTokens;
  const segments = [
    { key: "input", value: summary.freshInput, tint: KIND_COLORS.input, label: "In" },
    { key: "output", value: summary.output, tint: KIND_COLORS.output, label: "Out" },
    { key: "cacheWrite", value: summary.cacheCreation, tint: KIND_COLORS.cacheWrite, label: "Cache write" },
    { key: "cacheRead", value: summary.cacheRead, tint: KIND_COLORS.cacheRead, label: "Cache read" },
  ];
  const cost = summary.costMicros == null ? "—" : formatMicroUSD(summary.costMicros);
  const radius = 24.5;
  const circumference = 2 * Math.PI * radius;
  return (
    <section className="wb-card us-hero" aria-label="Usage summary">
      <div className="us-hero-tokens">
        <div className="us-hero-label">Real tokens · selected range</div>
        <div className="us-hero-big">{compactTokens(total)}</div>
        <div className="us-hero-legend">
          {segments.map((segment) => (
            <span className="us-hero-legend-item" key={segment.key}>
              <i style={{ background: segment.tint }} />
              {segment.label}
              <b>{compactTokens(segment.value)}</b>
            </span>
          ))}
        </div>
        <div className="us-hero-bar" role="img" aria-label="Token composition">
          {total > 0
            ? segments.map((segment) => (
                <span key={segment.key} style={{ width: `${(segment.value / total) * 100}%`, background: segment.tint }} />
              ))
            : null}
        </div>
      </div>
      <div className="us-hero-divider" />
      <Section
        label="Total cost"
        value={cost}
        className="us-hero-cost"
        detail={
          summary.unpricedRequests > 0 ? (
            <span className="us-unpriced" title={`${summary.unpricedRequests} request(s) had no usable price and contribute $0.`}>
              {summary.unpricedRequests} unpriced
            </span>
          ) : summary.requests > 0 ? (
            "all requests priced"
          ) : null
        }
      />
      <div className="us-hero-divider" />
      <Section
        label="Requests"
        value={groupedNumber(summary.requests)}
        className="us-hero-requests"
        detail={summary.requests === 0 ? "no traffic in range" : "in selected range"}
      />
      <div className="us-hero-divider" />
      <div className="us-hero-section us-hero-cache">
        <svg width="54" height="54" viewBox="0 0 54 54" className="us-hero-ring" aria-hidden="true">
          <circle cx="27" cy="27" r={radius} fill="none" stroke="var(--wb-track)" strokeWidth="5" />
          <circle
            cx="27"
            cy="27"
            r={radius}
            fill="none"
            stroke={KIND_COLORS.output}
            strokeWidth="5"
            strokeLinecap="round"
            strokeDasharray={`${circumference}`}
            strokeDashoffset={`${circumference * (1 - Math.max(0, Math.min(1, summary.cacheHitRate)))}`}
            transform="rotate(-90 27 27)"
          />
        </svg>
        <div>
          <div className="us-hero-label">Cache hit</div>
          <div className="us-hero-value">{formatPercent(summary.cacheHitRate)}</div>
          <div className="us-hero-detail">{compactTokens(summary.cacheRead)} read</div>
        </div>
      </div>
    </section>
  );
}
