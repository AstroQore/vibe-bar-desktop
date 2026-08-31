import { useEffect, useState } from "react";

import type { PresentationSettings, QuotaBucket, QuotaForecast, QuotaView } from "../api";
import { api, formatRemaining, quotaBarColor } from "../api";
import { companyFor, groupLabelFor, subProviderFor } from "../naming";
import { useDarkMode } from "../theme";
import { FORECAST_VERDICT, providerAccent } from "../tokens";
import { RingGauge } from "./RingGauge";

const DEFAULT_FIELDS = ["codex.weekly", "claude.weekly", "claude.five_hour"];
/** Beyond this the window is wider than the screen it sits on the edge of. */
const MAX_CELLS = 12;

/** One dial: a bucket, and what is said about it. */
interface Cell {
  id: string;
  bucket: QuotaBucket;
  label: string;
  value: number;
  showsUsed: boolean;
}

/** The quota axis as this window draws it — company, SubProvider, model group,
 *  then the windows themselves. */
interface Group {
  label: string | null;
  cells: Cell[];
}
interface SubProvider {
  name: string;
  groups: Group[];
}
interface Company {
  name: string;
  tool: string;
  subProviders: SubProvider[];
}

export function MiniQuota() {
  const [view, setView] = useState<QuotaView | null>(null);
  const [settings, setSettings] = useState<PresentationSettings | null>(null);

  useEffect(() => {
    const refreshSettings = () =>
      api.presentationSettings().then(setSettings).catch(() => undefined);
    api.quotaView().then(setView).catch(() => undefined);
    refreshSettings();
    const unlistenQuota = api.onQuotaUpdated((next) => {
      setView(next);
      refreshSettings();
    });
    const unlistenShown = api.onMiniShown(refreshSettings);
    return () => {
      unlistenQuota.then((off) => off()).catch(() => undefined);
      unlistenShown.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const fields = settings?.selectedFieldIds.length
    ? settings.selectedFieldIds
    : [
        ...new Set([
          ...DEFAULT_FIELDS,
          ...(view?.accounts.flatMap((account) =>
            account.buckets[0] ? [`${account.tool}.${account.buckets[0].id}`] : [],
          ) ?? []),
        ]),
      ];
  const companies = view ? arrange(view, settings, fields.slice(0, MAX_CELLS)) : [];

  return (
    <main className="mini-quota" data-tauri-drag-region>
      <div className="mini-title" data-tauri-drag-region="deep">
        <span>Vibe Bar</span>
        <button
          className="mini-close"
          aria-label="Hide Mini"
          data-tauri-drag-region="false"
          onClick={() => void api.hideMini().catch(() => undefined)}
        >
          ×
        </button>
      </div>
      <MiniQuotaBody
        companies={companies}
        loading={view === null}
        layout={settings?.miniDisplayMode}
      />
    </main>
  );
}

/**
 * The arranged tree, without the plumbing that fetches it.
 *
 * Split out so the layout can be mounted on its own — the quota axis is the
 * part worth looking at, and it should not need a running Tauri host to see.
 */
export function MiniQuotaBody({
  companies,
  loading = false,
  layout = "regular",
}: {
  companies: Company[];
  loading?: boolean;
  /** "regular" draws ring gauges; "compact" the same arrangement as vertical
   *  bars, sized for a corner. Native's other five fall back to regular in the
   *  core, so only these two arrive here. */
  layout?: string;
}) {
  const dark = useDarkMode();
  if (layout === "ledger" && !loading && companies.length > 0) {
    return <MiniLedger companies={companies} dark={dark} />;
  }
  return (
    <>
      {loading ? (
        <p className="mini-empty">Loading quota…</p>
      ) : companies.length === 0 ? (
        <p className="mini-empty">No configured quota is available.</p>
      ) : (
        <div className="mini-companies">
          {companies.map((company, index) => (
            <section className="mini-company" key={company.name}>
              {index > 0 && <span className="mini-company-rule" aria-hidden />}
              <h2 className="mini-company-name">
                <span
                  className="mini-company-dot"
                  style={{ background: providerAccent(company.tool, dark) }}
                  aria-hidden
                />
                {company.name}
              </h2>
              <div className="mini-subproviders">
                {company.subProviders.map((subProvider) => (
                  <div className="mini-subprovider" key={subProvider.name}>
                    <h3 className="mini-subprovider-name">{subProvider.name}</h3>
                    <div className="mini-groups">
                      {subProvider.groups.map((group, groupIndex) => (
                        <div className="mini-group" key={group.label ?? groupIndex}>
                          {/* Named only where the SubProvider has more than one, so a
                              heading always means "these are different models" — "ALL"
                              over a lone column says nothing. The line is kept either
                              way, so dials in neighbouring columns stay level. */}
                          <div className="mini-group-name">
                            {subProvider.groups.length > 1 ? group.label ?? " " : " "}
                          </div>
                          <div className="mini-cells">
                            {group.cells.map((cell) =>
                              layout === "compact" ? (
                                <MiniCompactCell key={cell.id} cell={cell} />
                              ) : (
                                <MiniCell key={cell.id} cell={cell} />
                              ),
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </>
  );
}

/**
 * The same three tiers as vertical bars, sized for a corner.
 *
 * Same arrangement as the rings — company → SubProvider → group is decided in
 * `arrange`, not here — and the same facts per cell. What changes is the
 * gauge: a bar fills from the bottom, which fits a narrow column where a ring
 * needs a square. The forecast marker is a line across the bar rather than a
 * notch on an arc, for the same reason.
 */
/**
 * One row per quota bucket — fixed width, grows downward.
 *
 * The same tree as the other layouts, laid out vertically: company, then
 * SubProvider, then the group where one is named, then a row per bucket. The
 * tiers are indents rather than columns, and every bar starts at the same x so
 * the column reads as one scale — which is why the group header and the rows
 * share an indent instead of the header stepping in further than its own
 * children.
 */
function MiniLedger({ companies, dark }: { companies: Company[]; dark: boolean }) {
  return (
    <div className="mini-ledger">
      {companies.map((company) => (
        <section className="mini-ledger-company" key={company.name}>
          <h2 className="mini-ledger-company-name">
            <span
              className="mini-company-dot"
              style={{ background: providerAccent(company.tool, dark) }}
              aria-hidden
            />
            {company.name}
          </h2>
          {company.subProviders.map((subProvider) => (
            <div key={subProvider.name}>
              <h3 className="mini-ledger-sub-name">{subProvider.name}</h3>
              {subProvider.groups.map((group, groupIndex) => (
                <div key={group.label ?? groupIndex}>
                  {/* Only where the SubProvider has more than one: a heading
                      always means "these are different models", the same rule
                      the other layouts follow. */}
                  {subProvider.groups.length > 1 && group.label ? (
                    <h4 className="mini-ledger-group-name">{group.label}</h4>
                  ) : null}
                  {group.cells.map((cell) => (
                    <MiniLedgerRow key={cell.id} cell={cell} />
                  ))}
                </div>
              ))}
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

function MiniLedgerRow({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const planned = forecast ? plannedFor(forecast, showsUsed) : undefined;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;

  return (
    <div className="mini-ledger-row">
      <span className="mini-ledger-label" title={cell.label}>
        {cell.label}
      </span>
      <span className="mini-ledger-track">
        <span
          className="mini-ledger-fill"
          style={{ width: `${Math.min(100, Math.max(0, value))}%`, background: colour }}
        />
        {/* Same guard as the ring and the compact bar: a projection at or past
            the limit is not drawn, because a mark on the rim would read as
            "expected to land exactly there". */}
        {planned !== undefined && planned > 0 && planned < 100 ? (
          <span
            className="mini-ledger-mark"
            style={{ left: `${planned}%`, background: verdictColour ?? colour }}
          />
        ) : null}
      </span>
      <span className="mini-ledger-value" style={{ color: colour }}>
        {Math.round(value)}%
      </span>
      <span className="mini-ledger-reset">{formatRemaining(bucket.resetAt) || "—"}</span>
    </div>
  );
}

function MiniCompactCell({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;
  const planned = forecast ? plannedFor(forecast, showsUsed) : undefined;

  return (
    <div className="mini-compact-cell">
      <div
        className="mini-compact-bar"
        role="img"
        aria-label={`${cell.label} ${Math.round(value)}%`}
      >
        <div
          className="mini-compact-fill"
          style={{ height: `${Math.min(100, Math.max(0, value))}%`, background: colour }}
        />
        {/* Where the forecast expects to be at the reset. Drawn only when it
            is on the bar: a marker pinned to the rim would read as a
            prediction of exactly full, which is not what a projection past
            100% means. */}
        {planned !== undefined && planned > 0 && planned < 100 ? (
          <div
            className="mini-compact-mark"
            style={{ bottom: `${planned}%`, background: verdictColour ?? colour }}
          />
        ) : null}
      </div>
      <div className="mini-compact-value" style={{ color: colour }}>
        {Math.round(value)}%
      </div>
      <div className="mini-compact-label">{cell.label}</div>
    </div>
  );
}

function MiniCell({ cell }: { cell: Cell }) {
  const { bucket, value, showsUsed } = cell;
  const colour = quotaBarColor(value, showsUsed);
  const forecast = bucket.forecast;
  const verdictColour = forecast ? FORECAST_VERDICT[forecast.verdict] : undefined;
  // The cell is 62px wide and this line is the widest thing in it. Native
  // shrinks the text rather than clipping it, which is worth copying: "out 3d"
  // and "out 3d 14h" are different answers, and an ellipsis hides which.
  const line = forecast ? forecastLine(forecast) : " ";

  return (
    <div className="mini-cell">
      <RingGauge
        percent={value}
        expected={forecast ? plannedFor(forecast, showsUsed) : undefined}
        color={colour}
        markerColor={verdictColour}
      >
        <span style={{ color: colour }}>{Math.round(value)}%</span>
      </RingGauge>
      <div className="mini-cell-label">{cell.label}</div>
      <div
        className={`mini-cell-forecast${forecastFit(line)}`}
        style={verdictColour ? { color: verdictColour } : undefined}
      >
        {line}
      </div>
      <div className="mini-cell-reset">{formatRemaining(bucket.resetAt) || " "}</div>
    </div>
  );
}

/**
 * Which size class the forecast line needs to fit 62 pixels.
 *
 * Character count is a rough proxy — "may run out 5d 11h" and
 * "surplus · 92% left" are both eighteen characters and differ by five pixels
 * — so the steps are set from the widest wording at each length rather than
 * the average. The alternative is measuring after layout, which is more
 * machinery than a handful of bounded strings deserve.
 */
function forecastFit(line: string): string {
  if (line.length > 17) return " is-longest";
  if (line.length > 13) return " is-long";
  return "";
}

/** Where the same number is expected to be by now, in whichever direction the
 *  dial is drawn. */
function plannedFor(forecast: QuotaForecast, showsUsed: boolean): number {
  return showsUsed ? forecast.plannedUsedPercent : 100 - forecast.plannedUsedPercent;
}

/**
 * The one line a dial has room for.
 *
 * Terser than the popover's wording, which is the native mini's choice too:
 * under a 48-point ring there is space for a verdict or a number, rarely both.
 */
export function forecastLine(forecast: QuotaForecast, now = Date.now() / 1000): string {
  const runOut = formatRemaining(forecast.runOutAt, now);
  if (runOut) {
    return forecast.verdict === "watch" ? `may run out ${runOut}` : `out ${runOut}`;
  }
  const left = Math.round(Math.max(0, 100 - forecast.projectedUsedPercent));
  switch (forecast.verdict) {
    case "enough":
      return `${left}% left`;
    case "surplus":
      return `surplus · ${left}% left`;
    case "watch":
      return "watch";
    case "atRisk":
      return "risk";
    default:
      return `learning · ${left}% left`;
  }
}

/**
 * Resolve the chosen fields into the quota axis.
 *
 * Grouping is the point of this layout: a dial on its own says how full
 * something is but not what it is. All three levels come from the shared
 * naming contract, so a bucket sits under the same headings here as it does in
 * the native app.
 */
export function arrange(
  view: QuotaView,
  settings: PresentationSettings | null,
  fields: string[],
): Company[] {
  const companies: Company[] = [];
  const showsUsed = settings?.displayMode === "used";

  for (const field of fields) {
    const separator = field.indexOf(".");
    if (separator <= 0 || separator === field.length - 1) continue;
    const tool = field.slice(0, separator);
    const bucketId = field.slice(separator + 1);
    const bucket = view.accounts
      .filter((account) => account.tool === tool)
      .flatMap((account) => account.buckets)
      .find((candidate) => candidate.id === bucketId);
    if (!bucket) continue;

    const remaining = Math.max(0, 100 - bucket.usedPercent);
    const cell: Cell = {
      id: field,
      bucket,
      // Just the window. The group heading above already says Spark or Fable,
      // and repeating it under every dial costs the width the forecast line
      // needs. A custom label still wins, because the user chose it.
      label: settings?.customLabels[field] || bucket.title,
      value: showsUsed ? bucket.usedPercent : remaining,
      showsUsed,
    };

    const companyName = companyFor(tool);
    let company = companies.find((candidate) => candidate.name === companyName);
    if (!company) {
      company = { name: companyName, tool, subProviders: [] };
      companies.push(company);
    }
    const subProviderName = subProviderFor(tool, bucketId);
    let subProvider = company.subProviders.find((c) => c.name === subProviderName);
    if (!subProvider) {
      subProvider = { name: subProviderName, groups: [] };
      company.subProviders.push(subProvider);
    }
    const groupLabel = groupLabelFor(tool, bucketId, bucket.groupTitle);
    let group = subProvider.groups.find((candidate) => candidate.label === groupLabel);
    if (!group) {
      group = { label: groupLabel, cells: [] };
      subProvider.groups.push(group);
    }
    group.cells.push(cell);
  }
  return companies;
}
