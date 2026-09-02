import type { GroupStat, UsageSummary } from "../../api";
import { compactTokens, compactUSD, formatPercent, paletteColor } from "./model";

interface Slice {
  id: string;
  label: string;
  detail: string | null;
  tokens: number;
  costMicros: number | null;
  color: string;
}

const TOP_SLICES = 6;

function fromGroups(groups: GroupStat[], detail: (group: GroupStat) => string | null): Slice[] {
  const sorted = [...groups].sort((a, b) => b.totalTokens - a.totalTokens);
  const head = sorted.slice(0, TOP_SLICES).map((group, index) => ({
    id: group.name,
    label: group.name,
    detail: detail(group),
    tokens: group.totalTokens,
    costMicros: group.costMicros,
    color: paletteColor(index),
  }));
  const rest = sorted.slice(TOP_SLICES);
  if (rest.length > 0) {
    head.push({
      id: "other",
      label: `Other (${rest.length})`,
      detail: null,
      tokens: rest.reduce((acc, g) => acc + g.totalTokens, 0),
      costMicros: rest.reduce((acc, g) => acc + g.costMicros, 0),
      color: paletteColor(TOP_SLICES),
    });
  }
  return head;
}

function DonutCard({ title, subtitle, slices, emptyMessage }: { title: string; subtitle: string; slices: Slice[]; emptyMessage: string }) {
  const total = slices.reduce((acc, slice) => acc + slice.tokens, 0);
  const radius = 50;
  const stroke = 16;
  const circumference = 2 * Math.PI * radius;
  let offset = 0;
  return (
    <section className="wb-card us-donut-card" aria-label={title}>
      <div className="us-donut-head">
        <div className="us-donut-title">{title}</div>
        <div className="us-donut-sub">{subtitle}</div>
      </div>
      {total === 0 ? (
        <div className="us-donut-empty">{emptyMessage}</div>
      ) : (
        <div className="us-donut-body">
          <svg width="128" height="148" viewBox="0 0 128 148" className="us-donut" role="img" aria-label={`${compactTokens(total)} tokens`}>
            <g transform="translate(64 74)">
              <circle r={radius} fill="none" stroke="var(--wb-track)" strokeWidth={stroke} />
              {slices.map((slice) => {
                const fraction = slice.tokens / total;
                const dash = fraction * circumference;
                const element = (
                  <circle
                    key={slice.id}
                    r={radius}
                    fill="none"
                    stroke={slice.color}
                    strokeWidth={stroke}
                    strokeDasharray={`${Math.max(0, dash - 1.5)} ${circumference - Math.max(0, dash - 1.5)}`}
                    strokeDashoffset={-offset}
                    transform="rotate(-90)"
                  />
                );
                offset += dash;
                return element;
              })}
              <text y="-2" textAnchor="middle" className="us-donut-total">
                {compactTokens(total)}
              </text>
              <text y="12" textAnchor="middle" className="us-donut-unit">
                tokens
              </text>
            </g>
          </svg>
          <ul className="us-donut-legend">
            {slices.map((slice) => (
              <li key={slice.id} title={slice.detail ?? slice.label}>
                <i style={{ background: slice.color }} />
                <span className="us-donut-labels">
                  <span className="us-donut-label">{slice.label}</span>
                  <span className={`us-donut-detail${slice.detail ? "" : " blank"}`}>{slice.detail ?? " "}</span>
                </span>
                <span className="us-donut-values">
                  <span className="us-donut-tokens">{compactTokens(slice.tokens)}</span>
                  <span className="us-donut-share">
                    {formatPercent(slice.tokens / total, 0)}
                    {slice.costMicros != null && slice.costMicros > 0 ? ` ${compactUSD(slice.costMicros)}` : ""}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

/** The native `UsageDistributionDashboard`: five donuts on an adaptive grid. */
export function Distribution({
  summary,
  harnesses,
  providers,
  models,
}: {
  summary: UsageSummary;
  harnesses: GroupStat[];
  providers: GroupStat[];
  models: GroupStat[];
}) {
  const flow: Slice[] = [
    { id: "fresh", label: "Fresh input", detail: null, tokens: summary.freshInput, costMicros: null, color: paletteColor(0) },
    { id: "cache-read", label: "Cache read", detail: null, tokens: summary.cacheRead, costMicros: null, color: paletteColor(1) },
    { id: "cache-write", label: "Cache creation", detail: null, tokens: summary.cacheCreation, costMicros: null, color: paletteColor(4) },
    { id: "output", label: "Output", detail: null, tokens: summary.output, costMicros: null, color: paletteColor(2) },
  ];
  return (
    <div className="us-distribution">
      <DonutCard title="Token Flow" subtitle="input · cache · output" slices={flow} emptyMessage="No tokens in range" />
      <DonutCard title="Harness Mix" subtitle="where requests ran" slices={fromGroups(harnesses, (g) => g.company || null)} emptyMessage="No harness recorded usage in range" />
      <DonutCard title="Provider Mix" subtitle="billing companies" slices={fromGroups(providers, () => null)} emptyMessage="No provider recorded usage in range" />
      <DonutCard title="Project Mix" subtitle="Codex + Claude cwd · up to 30 d detail" slices={[]} emptyMessage="Project attribution isn't available in this client yet" />
      <DonutCard title="Model Mix" subtitle="canonical display names" slices={fromGroups(models, (g) => (g.unpricedRequests > 0 ? `${g.unpricedRequests} unpriced` : null))} emptyMessage="No model recorded usage in range" />
    </div>
  );
}
