import type { AccountQuota, QuotaView } from "../api";
import {
  describeError,
  formatCountdown,
  formatRelative,
  hierarchyFor,
  severityFor,
} from "../api";

/** Quota grouped the way Vibe Bar names things: L1 company → L2 SubProvider
 *  → L3 buckets. Never mix this with the harness (usage) axis. */
export function Overview({ view }: { view: QuotaView }) {
  if (view.accounts.length === 0) {
    return (
      <p className="empty">
        No quota yet. Sign in with the Codex or Claude CLI, or launch the macOS
        app once to populate shared Vibe Bar data.
      </p>
    );
  }

  const vendors = new Map<string, AccountQuota[]>();
  for (const account of view.accounts) {
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
            <QuotaCard key={account.accountId} account={account} />
          ))}
        </section>
      ))}
    </>
  );
}

function QuotaCard({ account }: { account: AccountQuota }) {
  const { product } = hierarchyFor(account.tool);

  return (
    <article className="card">
      <div className="card-head">
        <span className="card-title">{product}</span>
        {account.plan ? <span className="pill">{account.plan}</span> : null}
        {account.origin === "sharedCache" ? (
          <span
            className="pill"
            title="Read from the Vibe Bar data this Mac already had — not fetched by Desktop."
          >
            shared data
          </span>
        ) : null}
        <span className="card-meta">{formatRelative(account.queriedAt)}</span>
      </div>

      {account.error ? (
        <p className="error-row">{describeError(account.error)}</p>
      ) : account.buckets.length === 0 ? (
        <p className="error-row">No quota windows reported.</p>
      ) : (
        account.buckets.map((bucket) => {
          const remaining = Math.max(0, 100 - bucket.usedPercent);
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
                  {Math.round(remaining)}% left
                </span>
              </div>
              <div
                className="track"
                role="progressbar"
                aria-valuenow={Math.round(remaining)}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-label={`${product} ${bucket.title} remaining`}
              >
                <div
                  className={`fill ${severity}`}
                  style={{ width: `${remaining}%` }}
                />
              </div>
            </div>
          );
        })
      )}
    </article>
  );
}
