import type {
  AccountQuota,
  PresentationSettings,
  QuotaBucket,
  QuotaForecast,
  QuotaView,
} from "../api";
import { bucketLabelFor, companyFor, subProviderFor } from "../naming";
import { ProviderIcon } from "./ProviderIcon";
import { ResetHistory } from "./ResetHistory";
import {
  describeError,
  forecastConfidence,
  forecastDetail,
  forecastHeadline,
  forecastSeverity,
  formatCountdown,
  forecastMarks,
  formatRelative,
  quotaBarColor,
} from "../api";
import { FORECAST_VERDICT } from "../tokens";

/** Quota grouped the way Vibe Bar names things: L1 company → L2 SubProvider
 *  → L3 buckets. Never mix this with the harness (usage) axis. */
export function Overview({
  view,
  settings,
  company,
  detailed = false,
}: {
  view: QuotaView;
  settings: PresentationSettings | null;
  /** Show only this company's accounts. Everything when absent. */
  company?: string;
  /** Draw each bucket's cycle history. Reserved for a provider's own page:
   *  the overview is a glance across every provider, and a strip under every
   *  bucket turns it into a page you have to scroll. */
  detailed?: boolean;
}) {
  const visible = orderedVisibleAccounts(view.accounts, settings);
  const accounts = company
    ? visible.filter((account) => companyFor(account.tool) === company)
    : visible;
  const showsHistory = detailed;
  if (accounts.length === 0) {
    return (
      <p className="empty">
        {company
          ? `No quota from ${company} right now.`
          : view.accounts.length > 0
            ? "All quota providers are hidden by the shared presentation settings."
            : "No quota yet. Sign in with the Codex or Claude CLI, configure a supported provider API key, or launch the macOS app once to populate shared Vibe Bar data."}
      </p>
    );
  }

  const vendors = new Map<string, AccountQuota[]>();
  for (const account of accounts) {
    const vendor = companyFor(account.tool);
    const list = vendors.get(vendor) ?? [];
    list.push(account);
    vendors.set(vendor, list);
  }

  return (
    <>
      {[...vendors].map(([vendor, accounts]) => (
        <section className="vendor-group" key={vendor}>
          <h2 className="vendor-name">{vendor}</h2>
          {accounts.map((account) => (
            <QuotaCard
              key={account.accountId}
              account={account}
              settings={settings}
              showsHistory={showsHistory}
            />
          ))}
        </section>
      ))}
    </>
  );
}

function QuotaCard({
  account,
  settings,
  showsHistory,
}: {
  account: AccountQuota;
  settings: PresentationSettings | null;
  showsHistory: boolean;
}) {
  // A bucket can belong to a SubProvider its account does not — Cursor
  // reports Grok Bot — so the buckets are grouped by theirs rather than the
  // card picking one name and filing the rest under it. With one group, which
  // is every provider but Cursor, this draws exactly as before.
  const groups: { subProvider: string; buckets: QuotaBucket[] }[] = [];
  for (const bucket of account.buckets) {
    const subProvider = subProviderFor(account.tool, bucket.id);
    const existing = groups.find((group) => group.subProvider === subProvider);
    if (existing) existing.buckets.push(bucket);
    else groups.push({ subProvider, buckets: [bucket] });
  }
  const product = groups[0]?.subProvider ?? subProviderFor(account.tool);
  const plan = settings?.providerPlanLabels?.[account.tool] ?? account.plan;
  const showsUsed = settings?.displayMode === "used";

  return (
    <article className="card">
      <div className="card-head">
        <ProviderIcon tool={account.tool} />
        <span className="card-title">{product}</span>
        {plan ? <span className="pill">{plan}</span> : null}
        {account.origin === "sharedCache" ? (
          <span
            className="pill"
            title="Read from the Vibe Bar data this Mac already had — not fetched by Desktop."
          >
            shared data
          </span>
        ) : null}
        {account.origin === "desktopCache" ? (
          <span
            className="pill"
            title="Last successful quota fetched by Desktop — not a live refresh or native shared data."
          >
            desktop cache
          </span>
        ) : null}
        {account.origin === "mixed" ? (
          <span
            className="pill"
            title="This card combines quota windows observed through different sources."
          >
            mixed sources
          </span>
        ) : null}
        <span className="card-meta">{formatRelative(account.queriedAt)}</span>
      </div>

      {account.error ? (
        <p className="error-row">{describeError(account.error, account.tool)}</p>
      ) : null}
      {account.buckets.length === 0 && !account.error ? (
        <p className="error-row">No quota windows reported.</p>
      ) : (
        groups.flatMap((group) => [
          // Named only when the card holds more than one, so a heading always
          // means "these are not the same SubProvider".
          groups.length > 1 ? (
            <p className="sub-provider" key={`head-${group.subProvider}`}>
              {group.subProvider}
            </p>
          ) : null,
          ...group.buckets.map((bucket, index) => {
          // The model group this bucket belongs to, named once above the
          // first bucket in it. The provider's own wording rather than the
          // short label the mini window uses: there is room for it here, and
          // "Claude and GPT Models" says more than "Claude + GPT".
          const groupTitle = bucket.groupTitle?.trim();
          const previousTitle = group.buckets[index - 1]?.groupTitle?.trim();
          const inNamedGroup = namesItsOwnGroup(groupTitle, group.subProvider);
          const opensGroup = inNamedGroup && groupTitle !== previousTitle;
          const remaining = Math.max(0, 100 - bucket.usedPercent);
          const shown = showsUsed ? bucket.usedPercent : remaining;
          const countdown = formatCountdown(bucket.resetAt);
          return (
            <div className="bucket" key={bucket.id}>
              {opensGroup && <p className="bucket-group">{groupTitle}</p>}
              <div className="bucket-head">
                <span className="bucket-label">
                  {/* Under a heading that already names the group, the row
                      names only its window: "Weekly", not "Spark Weekly". */}
                  {inNamedGroup
                    ? bucket.title
                    : bucketLabelFor(
                        account.tool,
                        bucket.id,
                        bucket.title,
                        bucket.shortLabel,
                        bucket.groupTitle,
                        " · ",
                      )}
                </span>
                {countdown ? (
                  <span className="bucket-reset">{countdown}</span>
                ) : null}
                <span className="bucket-percent">
                  {Math.round(shown)}% {showsUsed ? "used" : "left"}
                </span>
              </div>
              <div
                className="track"
                role="progressbar"
                aria-valuenow={Math.round(shown)}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-label={`${group.subProvider} ${bucket.title} ${showsUsed ? "used" : "remaining"}`}
              >
                <div
                  className="fill"
                  style={{
                    width: `${shown}%`,
                    background: quotaBarColor(shown, showsUsed),
                  }}
                />
                {bucket.forecast ? (
                  <ForecastOverlay
                    forecast={bucket.forecast}
                    showsUsed={showsUsed}
                  />
                ) : null}
              </div>
              {bucket.forecast ? (
                <div className="verdict">
                  <span
                    className={`verdict-line ${forecastSeverity(
                      bucket.forecast.verdict,
                    )}`}
                  >
                    {forecastHeadline(bucket.forecast)}
                  </span>
                  {(() => {
                    const confidence = forecastConfidence(bucket.forecast);
                    return confidence ? (
                      <span className="verdict-confidence">{confidence}</span>
                    ) : null;
                  })()}
                  {(() => {
                    const detail = forecastDetail(bucket.forecast, Date.now() / 1000);
                    return detail ? (
                      <span className="verdict-detail">{detail}</span>
                    ) : null;
                  })()}
                </div>
              ) : null}
              {showsHistory && (
              <ResetHistory
                accountId={account.accountId}
                bucketId={bucket.id}
                tool={account.tool}
                mode={showsUsed ? "used" : "remaining"}
                refreshedAt={account.queriedAt}
                targetRemainingPercent={bucket.forecast?.targetRemainingPercent}
              />
              )}
            </div>
          );
          }),
        ])
      )}
    </article>
  );
}

/**
 * What a forecast adds to the bar: where the window would be if it were spent
 * evenly, where it is projected to land, and how sure that is.
 *
 * Three marks with three jobs, and one shared transform so they cannot
 * disagree about which way the bar runs. The interval stays inside the bar's
 * own height — a forecast is an annotation on the quota, not a second bar —
 * and the median sits inside its interval.
 */
function ForecastOverlay({
  forecast,
  showsUsed,
}: {
  forecast: QuotaForecast;
  showsUsed: boolean;
}) {
  const marks = forecastMarks(forecast, showsUsed);
  const verdict = FORECAST_VERDICT[forecast.verdict];
  const width = Math.max(0, marks.high - marks.low);
  return (
    <>
      {width > 0 && (
        <span
          className="forecast-band"
          style={{
            left: `${marks.low}%`,
            width: `${width}%`,
            background: verdict ?? "currentColor",
          }}
          title={`Between ${Math.round(marks.low)}% and ${Math.round(marks.high)}% at reset`}
        />
      )}
      {/* The wall clock: where an evenly spent window would be by now. Neutral
          and inset, because it is a reference rather than a prediction. */}
      <span
        className="pace-tick"
        style={{ left: `${marks.pace}%` }}
        title={`Time-only pace ${Math.round(marks.pace)}%`}
      />
      <span
        className="forecast-tick"
        style={{ left: `${marks.median}%`, background: verdict ?? undefined }}
        title={`Forecast ${Math.round(marks.median)}% at reset`}
      />
    </>
  );
}

/**
 * Does this bucket belong to a model group worth naming?
 *
 * A group that only repeats its SubProvider is not a heading — it is the same
 * name a third time, above a row that would then repeat it a fourth. Cursor's
 * Grok Bot bucket is the case: its group title, its SubProvider and its short
 * label are all "Grok Bot".
 */
export function namesItsOwnGroup(
  groupTitle: string | undefined,
  subProvider: string,
): boolean {
  const title = groupTitle?.trim();
  return (
    title !== undefined && title !== "" &&
    title.toLowerCase() !== subProvider.toLowerCase()
  );
}

export function orderedVisibleAccounts(
  accounts: AccountQuota[],
  settings: PresentationSettings | null,
): AccountQuota[] {
  const visibleCore = settings?.visibleCoreProviders;
  const visibleMisc = settings?.visibleMiscProviders;
  const coreOrder = settings?.coreProviderOrder ?? [];
  return accounts
    .filter((account) => {
      const representative = coreRepresentative(account.tool);
      if (representative) {
        return !visibleCore || visibleCore.includes(representative);
      }
      return !visibleMisc || visibleMisc.includes(account.tool);
    })
    .sort((left, right) => {
      const leftRank = coreRank(left.tool, coreOrder);
      const rightRank = coreRank(right.tool, coreOrder);
      return (
        leftRank - rightRank ||
        familyMemberRank(left.tool) - familyMemberRank(right.tool) ||
        left.tool.localeCompare(right.tool)
      );
    });
}

function familyMemberRank(tool: string): number {
  if (tool === "gemini" || tool === "grok") return 0;
  if (tool === "antigravity" || tool === "cursor") return 1;
  return 0;
}

function coreRepresentative(tool: string): string | undefined {
  if (tool === "codex" || tool === "claude" || tool === "gemini" || tool === "grok") return tool;
  if (tool === "antigravity") return "gemini";
  if (tool === "cursor") return "grok";
  return undefined;
}

function coreRank(tool: string, order: string[]): number {
  const representative = coreRepresentative(tool);
  if (!representative) return 10_000;
  const index = order.indexOf(representative);
  return index === -1 ? 1_000 : index;
}
