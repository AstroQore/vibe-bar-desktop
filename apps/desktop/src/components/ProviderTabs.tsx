import type { PresentationSettings, QuotaView } from "../api";
import { companyFor } from "../naming";
import { orderedVisibleAccounts } from "./Overview";
import { ProviderIcon } from "./ProviderIcon";

/**
 * The provider row for the quota surface: an overview, then one page per
 * company that has quota to show.
 *
 * A second level rather than more top-level tabs, which is how the native
 * popover arranges it — the provider selector belongs to the quota surface,
 * not beside Sessions and Settings. Companies come from the shared naming
 * contract and appear in the order the accounts do, so a page never turns up
 * under a name the other client does not use.
 */
export function ProviderTabs({
  view,
  settings,
  selected,
  onSelect,
}: {
  view: QuotaView;
  settings: PresentationSettings | null;
  selected: string;
  onSelect: (company: string) => void;
}) {
  const companies: { name: string; tool: string }[] = [];
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    const name = companyFor(account.tool);
    if (!companies.some((candidate) => candidate.name === name)) {
      companies.push({ name, tool: account.tool });
    }
  }
  // One company is not a choice; the row would be a label pretending to be a
  // control.
  if (companies.length < 2) return null;

  return (
    <nav className="provider-tabs" role="tablist" aria-label="Provider">
      <button
        className="provider-tab"
        role="tab"
        aria-selected={selected === ""}
        onClick={() => onSelect("")}
      >
        Overview
      </button>
      {companies.map((company) => (
        <button
          key={company.name}
          className="provider-tab"
          role="tab"
          aria-selected={selected === company.name}
          onClick={() => onSelect(company.name)}
        >
          <ProviderIcon tool={company.tool} size={13} />
          {company.name}
        </button>
      ))}
    </nav>
  );
}
