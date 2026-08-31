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
 * contract, so a page never turns up under a name the other client does not
 * use.
 */
export interface ProviderPage {
  name: string;
  tool: string;
}

/** The companies with quota to show, in the order the cards below the row use
 *  — the one `orderedVisibleAccounts` decides, so the row and the list agree. */
export function visibleCompanies(
  view: QuotaView,
  settings: PresentationSettings | null,
): ProviderPage[] {
  const companies: ProviderPage[] = [];
  for (const account of orderedVisibleAccounts(view.accounts, settings)) {
    const name = companyFor(account.tool);
    if (!companies.some((candidate) => candidate.name === name)) {
      companies.push({ name, tool: account.tool });
    }
  }
  return companies;
}

/**
 * Which page is actually open, given what there is to show.
 *
 * A selection outlives the thing it selected: a refresh can drop the company
 * that was open, or the presentation settings can hide it, and the stored
 * string would then filter the list down to nothing with no control left to
 * escape it — the row hides itself below two companies.
 */
export function activeCompany(companies: ProviderPage[], selected: string): string {
  return companies.some((company) => company.name === selected) ? selected : "";
}

/**
 * Whether the surface is looking at a single provider.
 *
 * True on a provider's own page, and true when there is only one company to
 * begin with — the overview *is* that provider's page then, and hiding the
 * detail would leave a single-provider install with nowhere to see it.
 */
export function showsProviderDetail(companies: ProviderPage[], selected: string): boolean {
  return activeCompany(companies, selected) !== "" || companies.length === 1;
}

export function ProviderTabs({
  companies,
  selected,
  onSelect,
}: {
  companies: ProviderPage[];
  selected: string;
  onSelect: (company: string) => void;
}) {
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
