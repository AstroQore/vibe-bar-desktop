import type { ProviderStatus, ServiceStatusView, StatusIncident } from "../api";
import { formatRelative } from "../api";

const WATCHED_TOOLS = ["claude", "gemini", "cursor"] as const;

export function ServiceStatus({
  status,
  refreshFailed,
}: {
  status: ServiceStatusView | null;
  refreshFailed: boolean;
}) {
  if (!status || status.providers.length === 0) {
    return (
      <section className="service-status unavailable">
        <span>Service status unavailable</span>
      </section>
    );
  }

  const watched = WATCHED_TOOLS.map((tool) => status.providers.find((provider) => provider.tool === tool));
  const incident = watched
    .flatMap((provider) => provider?.incidents ?? [])
    .sort((left, right) => (right.createdAt ?? 0) - (left.createdAt ?? 0))[0];

  return (
    <section className="service-status" aria-label="Service status">
      <div className="service-status-head">
        <span className="service-status-title">Service status</span>
        {status.updatedAt ? <span className="status-line">updated {formatRelative(status.updatedAt)}</span> : null}
      </div>
      <div className="service-status-providers">
        {WATCHED_TOOLS.map((tool, index) => (
          <ProviderPill key={tool} tool={tool} provider={watched[index]} />
        ))}
      </div>
      {refreshFailed ? (
        <span className="status-pill unavailable" title="Showing the last successful status snapshot">
          refresh failed · last known
        </span>
      ) : null}
      {incident ? <Incident incident={incident} /> : null}
    </section>
  );
}

function ProviderPill({ tool, provider }: { tool: string; provider?: ProviderStatus }) {
  const label = tool === "claude" ? "Claude" : tool === "gemini" ? "Google AI" : "Cursor";
  if (!provider) {
    return <span className="status-pill unavailable">{label} unavailable</span>;
  }
  const indicator = provider.indicator || "unknown";
  const description = provider.description || indicator;
  const statusLabel: Record<string, string> = {
    none: "operational",
    minor: "degraded",
    major: "partial outage",
    critical: "major outage",
    maintenance: "maintenance",
    unknown: "unavailable",
  };
  return (
    <span className={`status-pill ${indicator}`} title={description}>
      {label} · {statusLabel[indicator] ?? "unavailable"}
    </span>
  );
}

function Incident({ incident }: { incident: StatusIncident }) {
  const resolved = ["resolved", "postmortem", "completed"].includes(incident.status.trim().toLowerCase());
  return (
    <p className="service-incident" title={incident.impact}>
      {resolved ? "Resolved" : incident.impact || incident.status} · {incident.name}
    </p>
  );
}
