import type { AccountQuota, PresentationSettings, QuotaView } from "../api";
import {
  describeError,
  formatCountdown,
  formatRelative,
  hierarchyFor,
  severityFor,
} from "../api";

/** Quota grouped the way Vibe Bar names things: L1 company → L2 SubProvider
 *  → L3 buckets. Never mix this with the harness (usage) axis. */
export function Overview({
  view,
  settings,
}: {
  view: QuotaView;
  settings: PresentationSettings | null;
}) {
  const accounts = orderedVisibleAccounts(view.accounts, settings);
  if (accounts.length === 0) {
    return (
      <p className="empty">
        {view.accounts.length > 0
          ? "All quota providers are hidden by the shared presentation settings."
          : "No quota yet. Sign in with the Codex or Claude CLI, configure a supported provider API key, or launch the macOS app once to populate shared Vibe Bar data."}
      </p>
    );
  }

  const vendors = new Map<string, AccountQuota[]>();
  for (const account of accounts) {
    const vendor = hierarchyFor(account.tool).vendor;
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
            <QuotaCard key={account.accountId} account={account} settings={settings} />
          ))}
        </section>
      ))}
    </>
  );
}

function QuotaCard({
  account,
  settings,
}: {
  account: AccountQuota;
  settings: PresentationSettings | null;
}) {
  const { product } = hierarchyFor(account.tool);
  const plan = settings?.providerPlanLabels[account.tool] ?? account.plan;
  const showsUsed = settings?.displayMode === "used";

  return (
    <article className="card">
      <div className="card-head">
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
            title="This card combines quota windows from Desktop cache and shared Vibe Bar data."
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
        account.buckets.map((bucket) => {
          const remaining = Math.max(0, 100 - bucket.usedPercent);
          const shown = showsUsed ? bucket.usedPercent : remaining;
          const severity = severityFor(remaining);
          const countdown = formatCountdown(bucket.resetAt);
          return (
            <div className="bucket" key={bucket.id}>
              <div className="bucket-head">
                <span className="bucket-label">
                  {bucket.groupTitle
                    ? `${bucket.groupTitle} · ${bucket.title}`
                    : bucket.title}
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
                aria-label={`${product} ${bucket.title} ${showsUsed ? "used" : "remaining"}`}
              >
                <div
                  className={`fill ${severity}`}
                  style={{ width: `${shown}%` }}
                />
              </div>
            </div>
          );
        })
      )}
    </article>
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
