import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { AppInfo, CostView, EffectiveModelPricingRow, PendingUpdate, PresentationSettings, QuotaView } from "../../api";
import { api } from "../../api";
import { ToolBrandIcon } from "../../popover/brand";
import { companyFor, subProviderFor } from "../../naming";
import { Search, XCircle, Refresh } from "../icons";
import {
  COLOR_BASIS,
  COLOR_BASIS_DETAIL,
  CORE_PROVIDERS,
  DENSITIES,
  LAYOUTS,
  READ_ONLY_NOTE,
  REFRESH_OPTIONS,
  ROUTES,
  UPDATE_CHANNELS,
  type SectionEntry,
  type SectionId,
  filterSections,
  formatInterval,
  formatPerMillion,
  providerList,
  replacedSummary,
  routeStatus,
  routeStatusTitle,
  sections as buildSections,
} from "./model";
import "./settings.css";

const SYMBOLS: Record<string, string> = {
  system: "🖥", costData: "📊", pricing: "💲", mcp: "⛓", remote: "📡", privacy: "✋", menuBar: "▭", menuBarHealth: "🩺", miniWindow: "▣", layout: "◫", browserCookies: "🍪",
};

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="st-section">
      <div className="st-section-title">{title}</div>
      <div className="wb-card st-card">{children}</div>
    </div>
  );
}

function Seg({ options, value, onChange, readonly }: { options: ReadonlyArray<{ id: string; title: string }>; value: string; onChange?: (id: string) => void; readonly?: boolean }) {
  return (
    <span className={`st-seg${readonly ? " readonly" : ""}`} role="radiogroup" title={readonly ? READ_ONLY_NOTE : undefined}>
      {options.map((option) => (
        <button type="button" key={option.id} role="radio" aria-checked={value === option.id} className={value === option.id ? "on" : ""} disabled={readonly || !onChange} onClick={() => onChange?.(option.id)}>
          {option.title}
        </button>
      ))}
    </span>
  );
}

function Check({ label, checked, onChange, disabled, title }: { label: string; checked: boolean; onChange?: (next: boolean) => void; disabled?: boolean; title?: string }) {
  return (
    <label className="st-check" title={title}>
      <input type="checkbox" checked={checked} disabled={disabled || !onChange} onChange={(e) => onChange?.(e.target.checked)} />
      {label}
    </label>
  );
}

function UpdateCheck({ fixture }: { fixture: boolean }) {
  const [state, setState] = useState<{ at: "idle" | "checking" | "installing"; note?: string } | { at: "found"; update: PendingUpdate; note?: string }>({ at: "idle" });
  if (state.at === "found") {
    return (
      <div className="st-line">
        <span className="st-label">{state.update.version} is available</span>
        <button
          type="button"
          className="st-btn prominent"
          onClick={() => {
            const update = state.update;
            setState({ at: "installing" });
            api.installUpdate(update.id).catch((error: unknown) => setState({ at: "found", update, note: `could not install: ${String(error)}` }));
          }}
        >
          Install and relaunch
        </button>
        {state.note ? <span className="st-note">{state.note}</span> : null}
      </div>
    );
  }
  return (
    <div className="st-line">
      <button
        type="button"
        className="st-btn"
        disabled={state.at !== "idle" || fixture}
        onClick={() => {
          setState({ at: "checking" });
          api
            .checkForUpdate()
            .then((update) => setState(update ? { at: "found", update } : { at: "idle", note: "up to date" }))
            .catch((error: unknown) => setState({ at: "idle", note: `could not check: ${String(error)}` }));
        }}
      >
        <Refresh size={12} /> {state.at === "checking" ? "Checking…" : state.at === "installing" ? "Installing…" : "Check for Updates…"}
      </button>
      {state.note ? <span className="st-note">{state.note}</span> : null}
    </div>
  );
}

/** The Workbench Settings page — the native `SettingsView`: a searchable
 *  sidebar of grouped sections and titled cards for the selected one. */
export function SettingsPage({
  settings,
  info,
  cost,
  view,
  dark,
  onSave,
  replacedKeys,
  saveError = null,
  onDismissReplaced,
  onRescanCost,
  onCheckConnections,
  fixture = false,
  pricingFixture,
  initialSection = "system",
}: {
  settings: PresentationSettings | null;
  info: AppInfo | null;
  cost: CostView | null;
  view: QuotaView | null;
  dark: boolean;
  onSave: (changes: Record<string, unknown>) => unknown;
  replacedKeys: string[] | null;
  saveError?: string | null;
  onDismissReplaced: () => void;
  onRescanCost: () => Promise<unknown>;
  onCheckConnections: () => Promise<unknown>;
  fixture?: boolean;
  pricingFixture?: EffectiveModelPricingRow[];
  initialSection?: SectionId;
}) {
  const [search, setSearch] = useState("");
  const [section, setSection] = useState<SectionId>(initialSection);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartNote, setAutostartNote] = useState<string | null>(null);
  const [pricing, setPricing] = useState<EffectiveModelPricingRow[] | null>(pricingFixture ?? null);
  const [busy, setBusy] = useState<string | null>(null);
  const entries = useMemo(() => buildSections(settings), [settings]);
  const visible = filterSections(entries, search);

  useEffect(() => {
    if (fixture) {
      setAutostart(false);
      return;
    }
    api.autostartEnabled().then(setAutostart).catch((error: unknown) => setAutostartNote(String(error)));
  }, [fixture]);
  useEffect(() => {
    if (section !== "pricing" || pricing || fixture) return;
    api.pricingEffective().then(setPricing).catch(() => setPricing([]));
  }, [section, pricing, fixture]);

  const run = async (key: string, task: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await task();
    } finally {
      setBusy(null);
    }
  };

  const row = (entry: SectionEntry) => (
    <button type="button" key={entry.id} className={`st-row${section === entry.id ? " selected" : ""}${entry.enabled === false ? " off" : ""}`} onClick={() => setSection(entry.id)} aria-current={section === entry.id}>
      <span className="st-row-icon">{entry.tool ? <ToolBrandIcon tool={entry.tool} size={17} /> : SYMBOLS[entry.id] ?? "•"}</span>
      <span className="st-row-title">{entry.title}</span>
      {entry.enabled !== undefined ? <span className={`st-row-dot${entry.enabled ? " on" : ""}`} title={entry.enabled ? "Shown" : "Hidden"} /> : null}
      {entry.enabled !== undefined ? <span className="st-row-grip">≡</span> : null}
    </button>
  );
  const groups: Array<{ id: SectionEntry["group"]; title: string }> = [
    { id: "settings", title: "Settings" },
    { id: "core", title: "Core Providers" },
    { id: "misc", title: "Misc Providers" },
  ];
  const menuBar = settings?.menuBar ?? { isVisible: true, showTitle: false, layout: "singleLine" };
  const current = entries.find((e) => e.id === section);
  const providerSection = CORE_PROVIDERS.find((p) => p.id === section);

  const content = () => {
    if (!settings) return <p className="wb-empty">Loading settings…</p>;
    switch (section) {
      case "system":
        return (
          <>
            <Section title="System">
              <div className="st-line">
                <Check label="Launch at login" checked={autostart ?? false} disabled={autostart === null || fixture} onChange={(next) => void run("autostart", async () => setAutostart(await api.setAutostart(next)))} />
                {autostartNote ? <span className="st-note">{autostartNote}</span> : null}
              </div>
              <p className="st-note">Registers this app as a login item; the menu bar icon is back before the first refresh.</p>
              <div className="st-line">
                <button type="button" className="st-btn" disabled title="The setup assistant arrives with a later Desktop release.">
                  Show setup assistant
                </button>
              </div>
            </Section>
            <Section title="Refreshing">
              <div className="st-line">
                <span className="st-label">Percent shows</span>
                <Seg options={[{ id: "remaining", title: "Remaining" }, { id: "used", title: "Used" }]} value={settings.displayMode === "used" ? "used" : "remaining"} onChange={(id) => void onSave({ displayMode: id })} />
              </div>
              <div className="st-line">
                <span className="st-label">Refresh every</span>
                <select className="st-select" value={String(settings.refreshIntervalSeconds)} onChange={(e) => void onSave({ refreshIntervalSeconds: Number(e.target.value) })}>
                  {(REFRESH_OPTIONS.includes(settings.refreshIntervalSeconds as never) ? [...REFRESH_OPTIONS] : [...REFRESH_OPTIONS, settings.refreshIntervalSeconds].sort((a, b) => a - b)).map((seconds) => (
                    <option key={seconds} value={String(seconds)}>
                      {formatInterval(seconds)}
                    </option>
                  ))}
                </select>
              </div>
              <p className="st-note">Opening the popover refreshes all visible providers at most once per cooldown period.</p>
            </Section>
            <Section title="Updates">
              <p className="st-text">Vibe Bar Desktop {info?.version ?? "…"}</p>
              <p className="st-note">Checks run when you ask; a scheduled daily check arrives with a later release. Installing asks first.</p>
              {info?.isDemo ? <p className="st-note">Demo mode: update checks are off.</p> : <UpdateCheck fixture={fixture} />}
              <div className="st-line">
                <span className="st-label">Update channel</span>
                <Seg options={UPDATE_CHANNELS} value={settings.updateChannel} onChange={(id) => void onSave({ updateChannel: id })} />
              </div>
              <p className="st-note">Shared with the Vibe Bar menu-bar app: the same setting in both. Dev previews arrive before a Stable release.</p>
            </Section>
            <Section title="Components">
              <dl className="st-kv">
                <dt>agent-session-kit</dt>
                <dd>agent-session-core · bundled with this build</dd>
                <dt>Data root</dt>
                <dd className="st-mono">{info?.dataRoot ?? "…"}</dd>
                <dt>This client writes</dt>
                <dd className="st-mono">{info ? `${info.dataRoot}/client/desktop` : "…"}</dd>
                <dt>Native app</dt>
                <dd>{info?.nativeApp.installed ? "Installed on this Mac" : "Not installed"}</dd>
              </dl>
              <div className="st-links">
                <button type="button" onClick={() => void api.openUrl("https://github.com/AstroQore/agent-session-kit/releases").catch(() => undefined)} disabled={fixture}>
                  Release notes
                </button>
                <button type="button" onClick={() => void api.openUrl("https://github.com/AstroQore/agent-session-kit").catch(() => undefined)} disabled={fixture}>
                  Repository
                </button>
              </div>
            </Section>
          </>
        );
      case "costData":
        return (
          <>
            <Section title="Cost Data">
              <div className="st-line">
                <span className="st-label">Keep history</span>
                <Seg options={[{ id: "native", title: "Set in the native app" }]} value="native" readonly />
              </div>
              <p className="st-note">Applies to cost history and subscription fill history.</p>
              <div className="st-line">
                <Check label="Privacy mode" checked={cost?.privacySuppressed ?? false} title={READ_ONLY_NOTE} />
              </div>
              <p className="st-note">Privacy mode keeps cost data off disk and clears local cost history, snapshots, and scan cache. {READ_ONLY_NOTE}</p>
              <div className="st-line">
                <button type="button" className="st-btn" disabled={busy === "rescan" || fixture} onClick={() => void run("rescan", onRescanCost)}>
                  <Refresh size={12} /> {busy === "rescan" ? "Rescanning…" : "Rescan cost logs"}
                </button>
                <button type="button" className="st-btn" disabled title="Clearing the shared cost store is the native app's job; this client only keeps its own restart snapshot.">
                  Clear cost data
                </button>
              </div>
              <p className="st-note">Pricing data: {cost?.pricingVersion ?? "—"} · last scan {cost && cost.scannedAt > 0 ? new Date(cost.scannedAt * 1000).toLocaleString() : "never"} · {cost?.scannedFiles ?? 0} files</p>
            </Section>
          </>
        );
      case "pricing":
        return (
          <Section title="Model Pricing">
            <p className="st-note">{pricing ? `${pricing.length} models` : "Loading…"} · rates in USD per million tokens · the table this build prices with; overrides are managed in the native app.</p>
            {pricing && pricing.length > 0 ? (
              <div style={{ overflowX: "auto" }}>
                <table className="st-table">
                  <thead>
                    <tr>
                      <th>Provider</th>
                      <th>Model</th>
                      <th className="num">Input</th>
                      <th className="num">Output</th>
                      <th className="num">Cache read</th>
                      <th className="num">Cache write</th>
                      <th className="num">Fast ×</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pricing.map((row) => (
                      <tr key={`${row.provider}/${row.model}`}>
                        <td>{row.company} · {row.subProvider}</td>
                        <td className="model" title={row.displayLabel ?? row.model}>{row.model}</td>
                        <td className="num">{formatPerMillion(row.inputPerMillion)}</td>
                        <td className="num">{formatPerMillion(row.outputPerMillion)}</td>
                        <td className="num">{formatPerMillion(row.cacheReadPerMillion)}</td>
                        <td className="num">{formatPerMillion(row.cacheWritePerMillion)}</td>
                        <td className="num">{row.fastMultiplier ? `${row.fastMultiplier}×` : "—"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
            <p className="st-note">No local overrides in this client.</p>
          </Section>
        );
      case "mcp":
        return (
          <Section title="MCP Server">
            <div className="st-line">
              <Check label="Enable the local MCP server" checked={false} title="This client serves MCP over stdio on request rather than a socket." />
            </div>
            <p className="st-text">This client answers the same read-only MCP tools over stdio. Point a client at the app binary with <code className="st-mono">--mcp-stdio</code>; the socket server is the native app's.</p>
          </Section>
        );
      case "remote":
        return (
          <Section title="Remote Probes">
            <p className="st-text">Machines appear here after their first encrypted batch is imported.</p>
            <p className="st-note">Remote sync is not available in this client yet; pairing, the join code, and the control center live in the native app.</p>
          </Section>
        );
      case "privacy":
        return (
          <Section title="Privacy">
            <p className="st-text">Tokens are read from local CLI credentials. This client saves no browser or WebView cookies. Settings are shared under ~/.vibebar; this client's own quota cache and cost snapshot stay under ~/.vibebar/client/desktop.</p>
            <div className="st-routes">
              <span className="st-note">Connection health</span>
              {CORE_PROVIDERS.flatMap((provider) =>
                ROUTES[provider.id].map((route) => {
                  const status = routeStatus(route.kind, provider.id, view);
                  return (
                    <div className="st-route" key={`${provider.id}/${route.id}`}>
                      <i style={{ background: status === "found" ? "#34C759" : status === "missing" ? "#FF9500" : "var(--wb-track)" }} />
                      <span>{provider.title} · {route.title}</span>
                      <span>{routeStatusTitle(status)}</span>
                    </div>
                  );
                }),
              )}
            </div>
          </Section>
        );
      case "menuBar":
        return (
          <Section title="Overview">
            <div className="st-line"><Check label="Show in menu bar" checked={menuBar.isVisible} title={READ_ONLY_NOTE} /></div>
            <div className="st-line"><Check label="Show title text" checked={menuBar.showTitle} title={READ_ONLY_NOTE} /></div>
            <div className="st-line"><span className="st-label">Layout</span><Seg options={LAYOUTS} value={menuBar.layout} readonly /></div>
            <div className="st-line"><span className="st-label">Display density</span><Seg options={DENSITIES} value={settings.popoverDensity ?? "regular"} readonly /></div>
            <p className="st-note">{DENSITIES.find((d) => d.id === (settings.popoverDensity ?? "regular"))?.detail}</p>
            <div className="st-line"><span className="st-label">Percent color</span><Seg options={COLOR_BASIS} value={settings.menuBarColorBasis === "actual" ? "actual" : "forecast"} onChange={(id) => void onSave({ menuBarColorBasis: id })} /></div>
            <p className="st-note">{COLOR_BASIS_DETAIL}</p>
            <div className="st-note" style={{ marginTop: 4 }}>Fields</div>
            <p className="st-note">Shown, in this order — first renders leftmost. Rename any field for the menu bar only; empty inherits the default. {READ_ONLY_NOTE}</p>
            <div className="st-fields">
              {(settings.selectedFieldIds ?? []).length === 0 ? <p className="st-note">Desktop uses its default fields until the native app saves a selection.</p> : null}
              {groupFields(settings.selectedFieldIds ?? []).map((group) => (
                <div key={group.company}>
                  <div className="st-company"><ToolBrandIcon tool={group.tool} size={12} /> {group.company}</div>
                  {group.subs.map((sub) => (
                    <div key={sub.title}>
                      <div className="st-sub">{sub.title}</div>
                      {sub.fields.map((field) => (
                        <div className="st-field" key={field.id}>
                          <i />
                          <span className="st-field-title">{field.bucket}</span>
                          <span className="st-field-style">Logo</span>
                          <span className="st-field-label">{settings.customLabels?.[field.id] ?? ""}</span>
                          <span className="st-field-nav">⌃ ⌄ ✕</span>
                        </div>
                      ))}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </Section>
        );
      case "menuBarHealth":
        return (
          <Section title="Menu Bar Health">
            <div className="st-line"><Check label="Alert when macOS blocks the status item" checked title="The watchdog arrives with a later Desktop release." /></div>
            <div className="st-line">
              <button type="button" className="st-btn" disabled>Check Now</button>
              <button type="button" className="st-btn" disabled>Repair &amp; Re-register</button>
            </div>
            <p className="st-note">The menu bar health monitor is not attached in this process.</p>
          </Section>
        );
      case "miniWindow":
        return (
          <Section title="Mini Windows">
            <div className="st-line"><span className="st-label">Layout</span><span className="st-text">{settings.miniDisplayMode}</span></div>
            <div className="st-line"><span className="st-label">Strip density</span><Seg options={[{ id: "roomy", title: "Roomy" }, { id: "twoLine", title: "Two line" }, { id: "narrow", title: "Narrow" }]} value={settings.miniStripDensity} readonly /></div>
            <div className="st-line">
              <button type="button" className="st-btn" onClick={() => void api.toggleMini().catch(() => undefined)} disabled={fixture}>Open / Close</button>
            </div>
            <p className="st-note">Layouts, fields, and geometry are edited in the native app; this client shows the shared window.</p>
          </Section>
        );
      case "layout":
        return (
          <Section title="Layout">
            <p className="st-text">The popover layout editor lives in the native app. This client renders the shared Overview order.</p>
            <dl className="st-kv">
              <dt>Core order</dt><dd>{providerList(settings.coreProviderOrder)}</dd>
              <dt>Visible core</dt><dd>{settings.visibleCoreProviders ? providerList(settings.visibleCoreProviders) : "All"}</dd>
              <dt>Visible misc</dt><dd>{settings.visibleMiscProviders ? providerList(settings.visibleMiscProviders) : "All"}</dd>
            </dl>
          </Section>
        );
      case "browserCookies":
        return (
          <Section title="Browser Cookies">
            <p className="st-text">Cookie-backed providers are read by the native app. This client uses CLI and OAuth credentials only and keeps no cookies.</p>
          </Section>
        );
      default:
        if (providerSection) {
          const plan = settings.providerPlanLabels[providerSection.tool];
          const accounts = view?.accounts.filter((a) => companyFor(a.tool) === providerSection.title) ?? [];
          return (
            <Section title={providerSection.title}>
              <div className="st-line"><span className="st-label">Usage source</span><Seg options={[{ id: "cli", title: "CLI / OAuth (this client)" }]} value="cli" readonly /></div>
              <div className="st-line">
                <button type="button" className="st-btn" disabled title="Browser cookie import is the native app's.">Import from browser</button>
                <button type="button" className="st-btn" disabled title="WebView login is the native app's.">Open WebView login</button>
                <button type="button" className="st-btn" disabled title="This client keeps no cookies.">Delete cookies</button>
              </div>
              {plan ? <p className="st-note">Plan label: {plan}</p> : null}
              {accounts.length > 0 ? (
                <dl className="st-kv">
                  {accounts.map((account) => (
                    <div key={account.accountId} style={{ display: "contents" }}>
                      <dt>{subProviderFor(account.tool)}</dt>
                      <dd>{account.error ? `error · ${account.error.detail ?? account.error.kind}` : `${account.buckets.length} windows · ${account.plan ?? "plan unknown"}`}</dd>
                    </div>
                  ))}
                </dl>
              ) : <p className="st-note">No quota read for {providerSection.title} yet.</p>}
              <div className="st-routes">
                <span className="st-note">Connection health</span>
                {ROUTES[providerSection.id].map((route) => {
                  const status = routeStatus(route.kind, providerSection.id, view);
                  return (
                    <div className="st-route" key={route.id}>
                      <i style={{ background: status === "found" ? "#34C759" : status === "missing" ? "#FF9500" : "var(--wb-track)" }} />
                      <span>{route.title}</span>
                      <span>{routeStatusTitle(status)}</span>
                    </div>
                  );
                })}
              </div>
              <div className="st-line">
                <button type="button" className="st-btn" disabled={busy === "connections" || fixture} onClick={() => void run("connections", onCheckConnections)}>
                  <Refresh size={12} /> {busy === "connections" ? "Checking…" : `Check ${providerSection.title} connections`}
                </button>
              </div>
            </Section>
          );
        }
        if (section.startsWith("misc:")) {
          const instance = settings.miscProviderInstances.find((i) => `misc:${i.id}` === section);
          return (
            <Section title={instance?.name || instance?.tool || "Provider"}>
              <dl className="st-kv">
                <dt>Provider</dt><dd>{instance?.tool}</dd>
                <dt>Shown</dt><dd>{instance?.isVisible ? "Yes" : "Hidden"}</dd>
                <dt>Instance</dt><dd className="st-mono">{instance?.id}</dd>
              </dl>
              <p className="st-note">Credentials and per-instance options are managed in the native app; this client reads the cached quota it publishes.</p>
            </Section>
          );
        }
        return null;
    }
  };

  return (
    <div className="st-page">
      <nav className="st-sidebar" aria-label="Settings sections">
        <div className="wb-field st-search">
          <Search size={12} />
          <input type="search" placeholder="Search settings" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search settings" />
          {search ? (
            <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} onClick={() => setSearch("")} title="Clear">
              <XCircle size={12} />
            </button>
          ) : null}
        </div>
        {groups.map((group) => {
          const rows = visible.filter((e) => e.group === group.id);
          if (rows.length === 0) return null;
          return (
            <div key={group.id}>
              <div className="st-group">{group.title}</div>
              {rows.map(row)}
            </div>
          );
        })}
      </nav>
      <div className="st-content" style={{ ["--dark" as string]: dark ? 1 : 0 }}>
        {replacedKeys && replacedKeys.length > 0 ? (
          <div className="st-banner" role="status">
            <div>
              <b>Another Vibe Bar replaced your change</b>
              {replacedSummary(replacedKeys)}
            </div>
            <button type="button" className="st-btn" onClick={onDismissReplaced}>Dismiss</button>
          </div>
        ) : null}
        {saveError ? <div className="st-banner" role="alert"><div><b>Could not save</b>{saveError}</div></div> : null}
        {current ? content() : <p className="wb-empty">Pick a section.</p>}
      </div>
    </div>
  );
}

/** Group selected field ids as `<tool>.<bucket>` under company and SubProvider. */
function groupFields(ids: string[]): Array<{ company: string; tool: string; subs: Array<{ title: string; fields: Array<{ id: string; bucket: string }> }> }> {
  const groups = new Map<string, { company: string; tool: string; subs: Map<string, Array<{ id: string; bucket: string }>> }>();
  for (const id of ids) {
    const dot = id.indexOf(".");
    const tool = dot > 0 ? id.slice(0, dot) : id;
    const bucket = dot > 0 ? id.slice(dot + 1) : "";
    const company = companyFor(tool);
    const group = groups.get(company) ?? { company, tool, subs: new Map() };
    const sub = subProviderFor(tool, bucket);
    const list = group.subs.get(sub) ?? [];
    list.push({ id, bucket: bucket.replace(/_/g, " ") });
    group.subs.set(sub, list);
    groups.set(company, group);
  }
  return [...groups.values()].map((g) => ({ company: g.company, tool: g.tool, subs: [...g.subs.entries()].map(([title, fields]) => ({ title, fields })) }));
}
