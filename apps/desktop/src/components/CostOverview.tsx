import type { CostTotals, CostView, ModelCost } from "../api";
import { formatRelative, hierarchyFor } from "../api";

export function CostOverview({ cost }: { cost: CostView | null }) {
  if (!cost || cost.scannedAt === 0) {
    return (
      <section className="cost-overview unavailable">
        <span>Usage &amp; cost not scanned yet · use Refresh to scan local logs</span>
      </section>
    );
  }

  const hasUsage = cost.last30Days.tokens > 0 || cost.last30Days.requests > 0;
  if (!hasUsage) {
    return (
      <section className="cost-overview">
        <div className="cost-head">
          <span className="cost-title">Usage &amp; cost</span>
          <span className="status-line">scanned {formatRelative(cost.scannedAt)}</span>
        </div>
        <p className="cost-note">No local usage in the last 30 days.</p>
        {cost.truncated ? (
          <p className="cost-note">Scan limited to the newest 10,000 files per provider.</p>
        ) : null}
      </section>
    );
  }

  const models = [...cost.models]
    .sort((left, right) => right.pricedCostMicros - left.pricedCostMicros || right.tokens - left.tokens)
    .slice(0, 3);
  return (
    <section className="cost-overview">
      <div className="cost-head">
        <span className="cost-title">Usage &amp; cost</span>
        <span className="status-line">scanned {formatRelative(cost.scannedAt)}</span>
      </div>
      <div className="cost-totals">
        <CostPeriod label="Today" total={cost.today} />
        <CostPeriod label="7 days" total={cost.last7Days} />
        <CostPeriod label="30 days" total={cost.last30Days} />
      </div>
      {models.length ? (
        <>
          <p className="cost-note">Top models · all time</p>
          <ul className="cost-models">
            {models.map((model) => (
              <ModelRow key={`${model.tool}:${model.model}`} model={model} />
            ))}
          </ul>
        </>
      ) : null}
      {cost.unpricedEvents > 0 ? (
        <p className="cost-note">
          Priced portion only · {cost.unpricedEvents} event{cost.unpricedEvents === 1 ? "" : "s"} unpriced.
        </p>
      ) : null}
      {cost.truncated ? (
        <p className="cost-note">Scan limited to the newest 10,000 files per provider.</p>
      ) : null}
    </section>
  );
}

function CostPeriod({ label, total }: { label: string; total: CostTotals }) {
  return (
    <div className="cost-period">
      <span>{label}</span>
      <strong>{formatCost(total.pricedCostMicros)}</strong>
      <small>{formatCount(total.tokens)} tok · {formatCount(total.requests)} req</small>
    </div>
  );
}

function ModelRow({ model }: { model: ModelCost }) {
  const product = hierarchyFor(model.tool).product;
  return (
    <li>
      <span>{product} · {model.model || "Unknown model"}</span>
      <span>{formatCost(model.pricedCostMicros)}</span>
    </li>
  );
}

function formatCost(micros: number): string {
  return `$${(Math.max(0, micros) / 1_000_000).toFixed(2)}`;
}

function formatCount(value: number): string {
  return new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(value);
}
