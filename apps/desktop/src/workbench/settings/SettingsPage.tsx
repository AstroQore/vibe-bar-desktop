import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { AppInfo, CostView, EffectiveModelPricingRow, MenuBarHealthReport, PendingUpdate, PresentationSettings, QuotaView } from "../../api";
import { api } from "../../api";
import { ToolBrandIcon } from "../../popover/brand";
import { bucketLabelFor, companyFor, subProviderFor } from "../../naming";
import { Antenna, ChartBar, Cookie, Dollar, Hand, MenuBarIcon, Monitor, Nodes, Refresh, Search, Split, Stethoscope, Windows, XCircle } from "../icons";
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
  MINI_LAYOUTS,
  RETENTION_OPTIONS,
  STRIP_DENSITIES,
  replacedSummary,
  routeStatus,
  routeStatusTitle,
  sections as buildSections,
} from "./model";
import "./settings.css";

const FIELD_STYLES: ReadonlyArray<{ id: string; title: string }> = [
  { id: "labelAndPercent", title: "Label" },
  { id: "logoAndPercent", title: "Logo" },
  { id: "logoLabelAndPercent", title: "Logo and label" },
];

const SYMBOLS: Record<string, ReactNode> = {
  system: <Monitor size={15} />,
  costData: <ChartBar size={14} />,
  pricing: <Dollar size={14} />,
  mcp: <Nodes size={14} />,
  remote: <Antenna size={14} />,
  privacy: <Hand size={14} />,
  menuBar: <MenuBarIcon size={14} />,
  menuBarHealth: <Stethoscope size={14} />,
  miniWindow: <Windows size={14} />,
  layout: <Split size={14} />,
  browserCookies: <Cookie size={14} />,
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
    <span className={`wb-seg${readonly ? " readonly" : ""}`} role="radiogroup" title={readonly ? READ_ONLY_NOTE : undefined}>
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
  useEffect(() => {
    if (fixture) return;
    let live = true;
    const apply = (update: PendingUpdate | null) => {
      if (!live) return;
      setState((current) => {
        if (current.at === "installing") return current;
        if (update) return { at: "found", update };
        return current.at === "found" ? { at: "idle", note: "no longer available" } : current;
      });
    };
    // Listen first, then ask: a check that finishes between the two would
    // otherwise be missed by both.
    const off = api.onUpdateAvailable(apply).then((unlisten) => {
      void api.pendingUpdate().then(apply).catch(() => undefined);
      return unlisten;
    });
    return () => {
      live = false;
      void off.then((unlisten) => unlisten());
    };
  }, [fixture]);
  if (state.at === "found") {
    return (
      <div className="st-line">
        <span className="st-label">{state.update.version} is available</span>
        <button
          type="button"
          className="wb-pill prominent"
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
        className="wb-pill"
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
  onShowAssistant,
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
  onShowAssistant?: () => void;
  fixture?: boolean;
  pricingFixture?: EffectiveModelPricingRow[];
  initialSection?: SectionId;
}) {
  const [search, setSearch] = useState("");
  const [section, setSection] = useState<SectionId>(initialSection);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartNote, setAutostartNote] = useState<string | null>(null);
  const [pricing, setPricing] = useState<EffectiveModelPricingRow[] | null>(pricingFixture ?? null);
  const [health, setHealth] = useState<MenuBarHealthReport | null>(null);
  const [raw, setRaw] = useState<Record<string, unknown> | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const reloadRaw = () => api.sharedSettingsRaw().then(setRaw).catch(() => undefined);
  useEffect(() => {
    void reloadRaw();
  }, [settings]);
  /** Nested objects are edited whole, so an edit must be built on the file's
   *  current value: the page has to have read it, and successive edits run
   *  one after another, each on top of what the previous write produced. */
  const ready = raw !== null;
  const save = (changes: Record<string, unknown>) => {
    if (!ready) return Promise.resolve();
    // The next edit sees this one at once, before the write lands.
    setRaw((current) => (current ? { ...current, ...changes } : current));
    const run = queue.current.then(async () => {
      try {
        await onSave(changes);
      } finally {
        await reloadRaw();
      }
    });
    queue.current = run.catch(() => undefined);
    return run;
  };
  const menuBarItems = (Array.isArray(raw?.menuBarItems) ? (raw!.menuBarItems as Record<string, unknown>[]) : []) as Record<string, unknown>[];
  const item0: Record<string, unknown> = menuBarItems[0] ?? { kind: "primary", isVisible: true, showTitle: false, layout: "singleLine", selectedFieldIds: [], customLabels: {}, fieldStyles: {} };
  const saveItem = (patch: Record<string, unknown>) => {
    if (!ready) return Promise.resolve();
    const next = menuBarItems.length > 0 ? menuBarItems.map((item, index) => (index === 0 ? { ...item, ...patch } : item)) : [{ ...item0, ...patch }];
    return save({ menuBarItems: next });
  };
  const selectedFieldIds = (Array.isArray(item0.selectedFieldIds) ? (item0.selectedFieldIds as string[]) : []) as string[];
  const customLabels = ((item0.customLabels ?? {}) as Record<string, string>);
  const fieldStyles = ((item0.fieldStyles ?? {}) as Record<string, string>);
  const costData = ((raw?.costData ?? {}) as Record<string, unknown>);
  const miniWindow = ((raw?.miniWindow ?? {}) as Record<string, unknown>);
  const miniWindows = (Array.isArray(miniWindow.windows) ? (miniWindow.windows as Record<string, unknown>[]) : []) as Record<string, unknown>[];
  const coreOrder = (Array.isArray(raw?.coreProviderOrder) ? (raw!.coreProviderOrder as string[]) : settings?.coreProviderOrder ?? ["codex", "claude", "gemini", "grok"]) as string[];
  const visibleCore = (Array.isArray(raw?.visibleCoreProviders) ? (raw!.visibleCoreProviders as string[]) : null) as string[] | null;
  const visibleMisc = (Array.isArray(raw?.visibleMiscProviders) ? (raw!.visibleMiscProviders as string[]) : null) as string[] | null;
  /** Visibility of a misc instance lives on the instance (`isVisible`), which
   *  is what `presentation()` reads; the id list is kept in step for readers
   *  of the older key. */
  const saveMiscVisible = (id: string, next: boolean) => {
    const list = (Array.isArray(raw?.miscProviderInstances) ? (raw!.miscProviderInstances as Record<string, unknown>[]) : []) as Record<string, unknown>[];
    const instances = list.map((i) => (i.id === id ? { ...i, isVisible: next } : i));
    const visibleIds = instances.filter((i) => i.isVisible !== false).map((i) => String(i.id));
    return save({ miscProviderInstances: instances, visibleMiscProviders: visibleIds });
  };
  const knownFields = (view?.accounts ?? []).flatMap((account) =>
    account.buckets.map((bucket) => ({ id: `${account.tool}.${bucket.id}`, tool: account.tool, title: bucketLabelFor(account.tool, bucket.id, bucket.title, bucket.shortLabel, bucket.groupTitle, " · ") })),
  );
  const fieldTitle = (id: string) => knownFields.find((f) => f.id === id)?.title ?? id.slice(id.indexOf(".") + 1).replace(/_/g, " ");
  const [healthNote, setHealthNote] = useState<string | null>(null);
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
    if (section !== "menuBarHealth" || fixture) return;
    let unlisten: (() => void) | undefined;
    api.menuBarHealth().then(setHealth).catch(() => undefined);
    api.onMenuBarHealth(setHealth).then((stop) => {
      unlisten = stop;
    }).catch(() => undefined);
    return () => unlisten?.();
  }, [section, fixture]);
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
  const current = entries.find((e) => e.id === section);
  const providerSection = CORE_PROVIDERS.find((p) => p.id === section);

  const NESTED: SectionId[] = ["menuBar", "menuBarHealth", "miniWindow", "layout", "costData"];
  const content = () => {
    if (!settings) return <p className="wb-empty">Loading settings…</p>;
    if (!ready && (NESTED.includes(section) || section.startsWith("misc:") || CORE_PROVIDERS.some((c) => c.id === section))) {
      return <p className="wb-empty">Reading the shared settings…</p>;
    }
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
                <button type="button" className="wb-pill" disabled={!onShowAssistant} title="Walk through the first-run choices again" onClick={onShowAssistant}>
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
                <select className="wb-select" value={String(settings.refreshIntervalSeconds)} onChange={(e) => void onSave({ refreshIntervalSeconds: Number(e.target.value) })}>
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
              <p className="st-note">A check runs shortly after launch and then daily, and when you ask. Nothing is installed until you say so.</p>
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
                <select className="wb-select" value={String(Number(costData.retentionDays ?? 0))} onChange={(e) => void save({ costData: { ...costData, retentionDays: Number(e.target.value) } })}>
                  {RETENTION_OPTIONS.map((option) => (
                    <option key={option.days} value={String(option.days)}>{option.title}</option>
                  ))}
                </select>
              </div>
              <p className="st-note">Applies to the native app's cost history and subscription fill history; this client keeps no history beyond its restart snapshot, so the value is shared, not enforced here.</p>
              <div className="st-line">
                <Check label="Privacy mode" checked={Boolean(costData.privacyModeEnabled)} onChange={(next) => void save({ costData: { ...costData, privacyModeEnabled: next } }).then(() => onRescanCost())} />
              </div>
              <p className="st-note">Privacy mode keeps cost data off disk and clears local cost history, snapshots, and scan cache; this client drops its restart snapshot and re-reads at once.</p>
              <div className="st-line">
                <button type="button" className="wb-pill" disabled={busy === "rescan" || fixture} onClick={() => void run("rescan", onRescanCost)}>
                  <Refresh size={12} /> {busy === "rescan" ? "Rescanning…" : "Rescan cost logs"}
                </button>
                <button type="button" className="wb-pill" disabled title="Clearing the shared cost store is the native app's job; this client only keeps its own restart snapshot.">
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
      case "menuBar": {
        const move = (id: string, delta: -1 | 1) => {
          const index = selectedFieldIds.indexOf(id);
          const target = index + delta;
          if (index < 0 || target < 0 || target >= selectedFieldIds.length) return;
          const next = [...selectedFieldIds];
          [next[index], next[target]] = [next[target], next[index]];
          void saveItem({ selectedFieldIds: next });
        };
        const groups = groupFields(selectedFieldIds);
        const unselected = knownFields.filter((f) => !selectedFieldIds.includes(f.id));
        return (
          <Section title="Overview">
            <div className="st-line"><Check label="Show in menu bar" checked={item0.isVisible !== false} onChange={(next) => void saveItem({ isVisible: next })} /></div>
            <div className="st-line"><Check label="Show title text" checked={Boolean(item0.showTitle)} onChange={(next) => void saveItem({ showTitle: next })} /></div>
            <div className="st-line"><span className="st-label">Layout</span><Seg options={LAYOUTS} value={String(item0.layout ?? "singleLine")} onChange={(id) => void saveItem({ layout: id })} /></div>
            <div className="st-line"><span className="st-label">Display density</span><Seg options={DENSITIES} value={String(raw?.popoverDensity ?? settings.popoverDensity ?? "regular")} onChange={(id) => void save({ popoverDensity: id })} /></div>
            <p className="st-note">{DENSITIES.find((d) => d.id === String(raw?.popoverDensity ?? settings.popoverDensity ?? "regular"))?.detail}</p>
            <div className="st-line"><span className="st-label">Percent color</span><Seg options={COLOR_BASIS} value={settings.menuBarColorBasis === "actual" ? "actual" : "forecast"} onChange={(id) => void save({ menuBarColorBasis: id })} /></div>
            <p className="st-note">{COLOR_BASIS_DETAIL}</p>
            <div className="st-note" style={{ marginTop: 4 }}>Fields</div>
            <p className="st-note">Shown, in this order — first renders leftmost. Rename any field for the menu bar only; empty inherits the default.</p>
            <div className="st-fields">
              {selectedFieldIds.length === 0 ? <p className="st-note">No fields yet — tick a bucket below to add it.</p> : null}
              {groups.map((group) => (
                <div key={group.company}>
                  <div className="st-company"><ToolBrandIcon tool={group.tool} size={12} /> {group.company}</div>
                  {group.subs.map((sub) => (
                    <div key={sub.title}>
                      <div className="st-sub">{sub.title}</div>
                      {sub.fields.map((field) => {
                        const index = selectedFieldIds.indexOf(field.id);
                        return (
                          <div className="st-field" key={field.id}>
                            <i />
                            <span className="st-field-title">{fieldTitle(field.id)}</span>
                            <select className="wb-select st-field-style" value={fieldStyles[field.id] ?? "logoAndPercent"} onChange={(e) => void saveItem({ fieldStyles: { ...fieldStyles, [field.id]: e.target.value } })} aria-label={`Style for ${fieldTitle(field.id)}`}>
                              {FIELD_STYLES.map((style) => (
                                <option key={style.id} value={style.id}>{style.title}</option>
                              ))}
                            </select>
                            <input
                              className="st-field-label"
                              placeholder="Default label"
                              defaultValue={customLabels[field.id] ?? ""}
                              key={`${field.id}:${customLabels[field.id] ?? ""}`}
                              aria-label={`Menu bar label for ${fieldTitle(field.id)}`}
                              onBlur={(e) => {
                                const value = e.target.value.trim();
                                if ((customLabels[field.id] ?? "") === value) return;
                                const next = { ...customLabels };
                                if (value) next[field.id] = value;
                                else delete next[field.id];
                                void saveItem({ customLabels: next });
                              }}
                            />
                            <span className="st-field-nav">
                              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Move up" disabled={index <= 0} onClick={() => move(field.id, -1)}>⌃</button>
                              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Move down" disabled={index >= selectedFieldIds.length - 1} onClick={() => move(field.id, 1)}>⌄</button>
                              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Remove from the menu bar" onClick={() => void saveItem({ selectedFieldIds: selectedFieldIds.filter((id) => id !== field.id) })}>✕</button>
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  ))}
                </div>
              ))}
            </div>
            <p className="st-note">Not in the menu bar — tick a bucket to add it. Discovered buckets appear here from the current quota reads.</p>
            <div className="st-fields">
              {unselected.length === 0 ? <p className="st-note">Every known bucket is already in the menu bar.</p> : null}
              {groupFields(unselected.map((f) => f.id)).map((group) => (
                <div key={group.company}>
                  <div className="st-company"><ToolBrandIcon tool={group.tool} size={12} /> {group.company}</div>
                  {group.subs.map((sub) => (
                    <div key={sub.title}>
                      <div className="st-sub">{sub.title}</div>
                      {sub.fields.map((field) => (
                        <div className="st-line" key={field.id} style={{ paddingLeft: 16, minHeight: 24 }}>
                          <Check label={fieldTitle(field.id)} checked={false} onChange={() => void saveItem({ selectedFieldIds: [...selectedFieldIds, field.id] })} />
                        </div>
                      ))}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </Section>
        );
      }
      case "menuBarHealth": {
        const stateCopy: Record<MenuBarHealthReport["state"], string> = {
          checking: "Checking menu bar status…",
          healthy: "Vibe Bar Desktop is visible in the menu bar",
          blocked: "macOS appears to be blocking Vibe Bar Desktop",
          unavailable: "Menu bar status is unavailable",
        };
        const report = health;
        const dot = report?.state === "healthy" ? "#34C759" : report?.state === "blocked" ? "#FF3B30" : report?.state === "unavailable" ? "var(--wb-track)" : "#FF9500";
        return (
          <Section title="Menu Bar Health">
            <div className="st-line">
              <Check label="Alert when macOS blocks the status item" checked={!Boolean(raw?.menuBarBlockAlertSuppressed)} onChange={(next) => void save({ menuBarBlockAlertSuppressed: !next })} />
            </div>
            <p className="st-note">Off means alerts were dismissed with “Don't check again”; health checks remain visible here.</p>
            <div className="st-line">
              <Check label="Automatically repair confirmed allow-list blocks" checked={Boolean(raw?.menuBarAutoRepairEnabled)} onChange={(next) => void save({ menuBarAutoRepairEnabled: next })} />
            </div>
            <p className="st-note">Off by default. When enabled, three consecutive blocked probes run the narrow repair, restart Control Center, and re-register only this app's status item. Full Disk Access is required.</p>
            <div className="st-status">
              <i style={{ background: dot }} />
              <span>{report ? stateCopy[report.state] : fixture ? stateCopy.unavailable : "Checking menu bar status…"}</span>
            </div>
            {report ? <p className="st-note">{report.message}{report.checkedAt > 0 ? ` · Checked ${new Date(report.checkedAt * 1000).toLocaleTimeString()}` : ""}</p> : null}
            {healthNote ? <p className="st-note">{healthNote}</p> : null}
            <div className="st-line">
              <button type="button" className="wb-pill" disabled={fixture || busy === "health"} onClick={() => void run("health", async () => setHealth(await api.menuBarCheckNow()))}>
                <Refresh size={12} /> Check Now
              </button>
              <button
                type="button"
                className="wb-pill"
                disabled={fixture || busy === "repair"}
                title="Removes stale cross-app references to this app from Control Center's allow-list, restarts Control Center, and re-registers the status item."
                onClick={() =>
                  void run("repair", async () => {
                    try {
                      setHealth(await api.menuBarRepair());
                      setHealthNote(null);
                    } catch (error) {
                      setHealthNote(String(error));
                    }
                  })
                }
              >
                Repair &amp; Re-register
              </button>
              <button
                type="button"
                className="wb-pill"
                disabled={!report?.repairCommand}
                onClick={() => {
                  if (report?.repairCommand) void navigator.clipboard.writeText(report.repairCommand).then(() => setHealthNote("Repair command copied.")).catch(() => setHealthNote("Could not reach the clipboard."));
                }}
              >
                Copy Repair Command
              </button>
            </div>
            <p className="st-note">On macOS 26, a hidden app can retain this app in its Control Center menuItemLocations and apply its own isAllowed=false state to it. Repair removes only that stale cross-app reference; it never changes another app's show/hide setting.</p>
            {report?.needsFullDiskAccess ? (
              <div className="st-line">
                <button type="button" className="wb-pill" onClick={() => void api.openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").catch((error: unknown) => setHealthNote(String(error)))}>
                  Open Full Disk Access settings
                </button>
              </div>
            ) : null}
          </Section>
        );
      }
      case "miniWindow":
        return (
          <Section title="Mini Windows">
            <div className="st-line"><span className="st-label">Layout</span><Seg options={MINI_LAYOUTS} value={String(miniWindow.displayMode ?? settings.miniDisplayMode)} onChange={(id) => void save({ miniWindow: { ...miniWindow, displayMode: id } })} /></div>
            <div className="st-line"><span className="st-label">Strip density</span><Seg options={STRIP_DENSITIES} value={String(miniWindows[0]?.stripDensity ?? settings.miniStripDensity)} onChange={(id) => void save({ miniWindow: { ...miniWindow, windows: miniWindows.length > 0 ? miniWindows.map((w, i) => (i === 0 ? { ...w, stripDensity: id } : w)) : [{ stripDensity: id }] } })} /></div>
            <div className="st-line">
              <button type="button" className="wb-pill" onClick={() => void api.toggleMini().catch(() => undefined)} disabled={fixture}>Open / Close</button>
            </div>
            <p className="st-note">Layout and density are shared with the native app; window geometry and per-window fields still live there.</p>
          </Section>
        );
      case "layout": {
        const moveCore = (tool: string, delta: -1 | 1) => {
          const index = coreOrder.indexOf(tool);
          const target = index + delta;
          if (index < 0 || target < 0 || target >= coreOrder.length) return;
          const next = [...coreOrder];
          [next[index], next[target]] = [next[target], next[index]];
          void save({ coreProviderOrder: next });
        };
        const coreVisible = (tool: string) => visibleCore === null || visibleCore.includes(tool);
        const miscVisible = (id: string) => visibleMisc === null || visibleMisc.includes(id);
        return (
          <Section title="Layout">
            <p className="st-text">The popover shows the core providers in this order; untick one to hide its card and tab everywhere.</p>
            <div className="st-fields">
              {coreOrder.map((tool, index) => (
                <div className="st-field" key={tool}>
                  <ToolBrandIcon tool={tool} size={13} />
                  <span className="st-field-title">{companyFor(tool)} · {subProviderFor(tool)}</span>
                  <Check label="Visible" checked={coreVisible(tool)} onChange={(next) => void save({ visibleCoreProviders: coreOrder.filter((t) => (t === tool ? next : coreVisible(t))) })} />
                  <span className="st-field-nav">
                    <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Move up" disabled={index === 0} onClick={() => moveCore(tool, -1)}>⌃</button>
                    <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Move down" disabled={index === coreOrder.length - 1} onClick={() => moveCore(tool, 1)}>⌄</button>
                  </span>
                </div>
              ))}
            </div>
            <div className="st-note" style={{ marginTop: 4 }}>Misc providers</div>
            <div className="st-fields">
              {settings.miscProviderInstances.length === 0 ? <p className="st-note">No misc provider instances configured.</p> : null}
              {settings.miscProviderInstances.map((instance) => (
                <div className="st-field" key={instance.id}>
                  {instance.tool ? <ToolBrandIcon tool={instance.tool} size={13} /> : <i />}
                  <span className="st-field-title">{instance.name || instance.tool}</span>
                  <Check label="Visible" checked={miscVisible(instance.id)} onChange={(next) => void saveMiscVisible(instance.id, next)} />
                </div>
              ))}
            </div>
            <p className="st-note">The page layout editor — modules per popover page, presets — remains in the native app.</p>
          </Section>
        );
      }
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
                <button type="button" className="wb-pill" disabled title="Browser cookie import is the native app's.">Import from browser</button>
                <button type="button" className="wb-pill" disabled title="WebView login is the native app's.">Open WebView login</button>
                <button type="button" className="wb-pill" disabled title="This client keeps no cookies.">Delete cookies</button>
              </div>
              <div className="st-line">
                <span className="st-label">Plan label</span>
                <input
                  className="st-field-label"
                  style={{ minWidth: 160 }}
                  placeholder={plan ? "" : "Shown on the card"}
                  defaultValue={plan ?? ""}
                  key={`${providerSection.tool}:${plan ?? ""}`}
                  aria-label={`Plan label for ${providerSection.title}`}
                  onBlur={(e) => {
                    const value = e.target.value.trim();
                    if ((plan ?? "") === value) return;
                    const labels = { ...(((raw?.providerPlanLabels ?? settings.providerPlanLabels) as Record<string, string>) ?? {}) };
                    if (value) labels[providerSection.tool] = value;
                    else delete labels[providerSection.tool];
                    void save({ providerPlanLabels: labels });
                  }}
                />
              </div>
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
                <button type="button" className="wb-pill" disabled={busy === "connections" || fixture} onClick={() => void run("connections", onCheckConnections)}>
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
                <dt>Instance</dt><dd className="st-mono">{instance?.id}</dd>
              </dl>
              {instance ? (
                <div className="st-line">
                  <Check label="Show this provider" checked={visibleMisc === null || visibleMisc.includes(instance.id)} onChange={(next) => void saveMiscVisible(instance.id, next)} />
                  <input
                    className="st-field-label"
                    style={{ minWidth: 160 }}
                    placeholder="Display name"
                    defaultValue={instance.name}
                    key={`${instance.id}:${instance.name}`}
                    aria-label="Display name"
                    onBlur={(e) => {
                      const value = e.target.value.trim();
                      if (value === instance.name || !value) return;
                      const list = (Array.isArray(raw?.miscProviderInstances) ? (raw!.miscProviderInstances as Record<string, unknown>[]) : []) as Record<string, unknown>[];
                      void save({ miscProviderInstances: list.map((i) => (i.id === instance.id ? { ...i, name: value } : i)) });
                    }}
                  />
                </div>
              ) : null}
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
            <button type="button" className="wb-pill" onClick={onDismissReplaced}>Dismiss</button>
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
