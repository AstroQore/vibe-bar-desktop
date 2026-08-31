import type { PresentationSettings, QuotaForecast, QuotaView } from "../api";
import { companyFor, subProviderFor } from "../naming";
import {
  forecastDetail,
  forecastHeadline,
  forecastSeverity,
  quotaBarColor,
} from "../api";
import { orderedVisibleAccounts } from "./Overview";

const CLOCK_SKEW_SECONDS = 300;
const RESET_GRACE_SECONDS = 180;

interface ResetEvent {
  id: string;
  vendor: string;
  product: string;
  bucket: string;
  plan?: string;
  resetAt: number;
  used: number;
  remaining: number;
  state: "upcoming" | "due" | "expired";
  forecast?: QuotaForecast;
}

export function collectResetEvents(
  view: QuotaView,
  settings: PresentationSettings | null,
  now = Date.now() / 1000,
) {
  const upcoming: ResetEvent[] = [];
  const expired: ResetEvent[] = [];
  let missing = 0;
  let invalid = 0;
  let futureDated = 0;

  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    if (!Number.isFinite(account.queriedAt) || account.queriedAt > now + CLOCK_SKEW_SECONDS) {
      futureDated += account.buckets.length;
      continue;
    }
    const vendor = companyFor(account.tool);
    const product = subProviderFor(account.tool);
    for (const bucket of account.buckets) {
      if (bucket.resetAt === undefined) {
        missing += 1;
        continue;
      }
      const resetDate = new Date(bucket.resetAt * 1000);
      if (!Number.isFinite(bucket.resetAt) || bucket.resetAt <= 0 || !Number.isFinite(resetDate.valueOf())) {
        invalid += 1;
        continue;
      }
      const used = Number.isFinite(bucket.usedPercent)
        ? Math.min(100, Math.max(0, bucket.usedPercent))
        : 0;
      const delta = bucket.resetAt - now;
      const event: ResetEvent = {
        id: `${account.accountId}/${bucket.id}/${bucket.resetAt}`,
        vendor,
        product,
        bucket: bucket.groupTitle?.trim()
          ? `${bucket.groupTitle.trim()} · ${bucket.title}`
          : bucket.title,
        plan: settings?.providerPlanLabels[account.tool] ?? account.plan,
        resetAt: bucket.resetAt,
        forecast: bucket.forecast,
        used,
        remaining: 100 - used,
        state: delta > 0 ? "upcoming" : delta >= -RESET_GRACE_SECONDS ? "due" : "expired",
      };
      (event.state === "upcoming" ? upcoming : expired).push(event);
    }
  }

  const tieBreak = (left: ResetEvent, right: ResetEvent) =>
    left.vendor.localeCompare(right.vendor) ||
    left.product.localeCompare(right.product) ||
    left.bucket.localeCompare(right.bucket);
  upcoming.sort((left, right) => left.resetAt - right.resetAt || tieBreak(left, right));
  expired.sort((left, right) => right.resetAt - left.resetAt || tieBreak(left, right));
  return { upcoming, expired, missing, invalid, futureDated };
}

export function Resets({
  view,
  settings,
}: {
  view: QuotaView;
  settings: PresentationSettings | null;
}) {
  const now = Date.now() / 1000;
  const events = collectResetEvents(view, settings, now);
  const hasRows = events.upcoming.length > 0 || events.expired.length > 0;
  const groups = [
    {
      title: "Next resets",
      detail: "Soonest first",
      rows: events.upcoming,
      empty: "No future reset is present in the current quota data.",
    },
    {
      title: "Needs refresh",
      detail: "These reset times already passed",
      rows: events.expired,
    },
  ];

  return (
    <div className="resets-page">
      <header className="resets-heading">
        <div>
          <h1>Upcoming resets</h1>
          <p>
            Provider-declared next reset times, with what each window is
            projected to do before it gets there.
          </p>
        </div>
      </header>

      {events.futureDated > 0 ? (
        <p className="banner reset-warning">
          {count(events.futureDated, "quota window")} hidden because the source
          observation is dated in the future.
        </p>
      ) : null}
      {events.invalid > 0 ? (
        <p className="banner reset-warning">
          {count(events.invalid, "quota window")} hidden because the reset time is invalid.
        </p>
      ) : null}

      {!hasRows ? (
        <p className="empty">
          {events.missing > 0
            ? `No reset times were reported for ${count(events.missing, "quota window")}.`
            : "No quota windows with reset times yet. Refresh after signing in with a provider CLI."}
        </p>
      ) : (
        <>
          {groups.map((group, index) =>
            index > 0 && group.rows.length === 0 ? null : (
              <section className="reset-section" key={group.title}>
                <h2 className="vendor-name">
                  {group.title} <span>{group.detail}</span>
                </h2>
                {group.rows.length === 0 ? (
                  <p className="card reset-list-empty">{group.empty}</p>
                ) : (
                  group.rows.map((event) => (
                    <article className={`card reset-card ${event.state}`} key={event.id}>
                      <div className="card-head">
                        <span className="card-title">{event.product}</span>
                        <span className="pill">{event.vendor}</span>
                        {event.plan ? <span className="pill">{event.plan}</span> : null}
                      </div>
                      <div className="reset-card-line">
                        <div>
                          <strong>{event.bucket}</strong>
                          <span>
                            {Math.round(event.remaining)}% left → 100% · +
                            {Math.round(event.used)}% refill
                          </span>
                          {event.forecast ? (
                            <span
                              className={`verdict-line ${forecastSeverity(
                                event.forecast.verdict,
                              )}`}
                            >
                              {forecastHeadline(event.forecast)}
                              {(() => {
                                const detail = forecastDetail(event.forecast, now);
                                return detail ? ` · ${detail}` : "";
                              })()}
                            </span>
                          ) : null}
                        </div>
                        <div className="reset-when">
                          <strong>{relativeReset(event, now)}</strong>
                          <span>{absoluteReset(event.resetAt, now)}</span>
                        </div>
                      </div>
                      <div className="track" aria-hidden="true">
                        <div
                          className="fill"
                          style={{
                            width: `${event.remaining}%`,
                            background: quotaBarColor(event.remaining, false),
                          }}
                        />
                      </div>
                    </article>
                  ))
                )}
              </section>
            ),
          )}
        </>
      )}

      {hasRows && events.missing > 0 ? (
        <p className="status-line">
          {count(events.missing, "other quota window")} did not include a reset time.
        </p>
      ) : null}
    </div>
  );
}

function relativeReset(event: ResetEvent, now: number): string {
  if (event.state === "due") return "reset due · waiting for refresh";
  const seconds = Math.abs(event.resetAt - now);
  const totalMinutes = Math.round(seconds / 60);
  const days = Math.floor(totalMinutes / 1_440);
  const hours = Math.floor((totalMinutes % 1_440) / 60);
  const minutes = totalMinutes % 60;
  const duration = seconds < 60
    ? "<1m"
    : days > 0
      ? `${days}d${hours > 0 ? ` ${hours}h` : ""}`
      : hours > 0
        ? `${hours}h${minutes > 0 ? ` ${minutes}m` : ""}`
        : `${minutes}m`;
  return event.state === "expired" ? `passed ${duration} ago` : `in ${duration}`;
}

function absoluteReset(unixSeconds: number, now: number): string {
  const reset = new Date(unixSeconds * 1000);
  const includeYear = reset.getFullYear() !== new Date(now * 1000).getFullYear();
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    ...(includeYear ? { year: "numeric" as const } : {}),
    hour: "2-digit",
    minute: "2-digit",
  }).format(reset);
}

function count(value: number, noun: string): string {
  return `${value} ${noun}${value === 1 ? "" : "s"}`;
}
