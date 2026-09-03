import { useEffect, useMemo, useState, type ReactNode } from "react";
import { api, type AppInfo, type PresentationSettings } from "../../api";
import { ToolBrandBadge } from "../../popover/brand";
import "./onboarding.css";

/** The native `OnboardingStep`s, in order, with their titles, subtitles and
 *  glyphs. Raw values double as the identifiers demo mode accepts. */
export const STEPS = [
  { id: "welcome", title: "Welcome", subtitle: "What Vibe Bar does, in one paragraph." },
  { id: "subscriptions", title: "Subscriptions", subtitle: "Turn on the plans you pay for." },
  { id: "browserCookies", title: "Browser cookies", subtitle: "Web quotas come from the session your browser already has." },
  { id: "apiKeyProviders", title: "Other plans", subtitle: "Plans tracked with an API key or a console cookie." },
  { id: "pricing", title: "Model pricing", subtitle: "Where the token prices behind the cost numbers come from." },
  { id: "launchAtLogin", title: "Launch at login", subtitle: "Keep the readout in your menu bar from the moment you sign in." },
  { id: "done", title: "All set", subtitle: "Everything here lives in Settings too." },
] as const;
export type StepId = (typeof STEPS)[number]["id"];

/** The four core subscriptions and the surfaces each covers — the native
 *  `OnboardingCoreProviderCard`'s product line. */
const CORE: ReadonlyArray<{ tool: string; vendor: string; products: string }> = [
  { tool: "codex", vendor: "OpenAI", products: "Codex CLI · ChatGPT web" },
  { tool: "claude", vendor: "Anthropic", products: "Claude Code · claude.ai web" },
  { tool: "gemini", vendor: "Google", products: "Gemini web · AntiGravity" },
  { tool: "grok", vendor: "xAI", products: "Grok CLI · grok.com · Cursor" },
];

function Glyph({ id, size = 12 }: { id: StepId; size?: number }) {
  const common = { width: size, height: size, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (id) {
    case "welcome":
      return <svg {...common}><path d="M5 8V3.5a1 1 0 0 1 2 0V7M7 6.5V2.8a1 1 0 0 1 2 0V7M9 6.8V3.6a1 1 0 0 1 2 0V8.5M11 8V5.4a1 1 0 0 1 2 0V10c0 2.8-2 4.5-4.5 4.5S4 12.6 4 10.2V8.2a1 1 0 0 1 2 0"/></svg>;
    case "subscriptions":
      return <svg {...common}><rect x="1.5" y="3.5" width="13" height="9" rx="1.8"/><path d="M1.5 6.5h13M4 10h3"/></svg>;
    case "browserCookies":
      return <svg {...common}><circle cx="8" cy="8" r="6.2"/><path d="M2 8h12M8 1.8c2 2 2 10.4 0 12.4M8 1.8c-2 2-2 10.4 0 12.4"/></svg>;
    case "apiKeyProviders":
      return <svg {...common}><circle cx="5.5" cy="10.5" r="3"/><path d="M7.8 8.2 13.5 2.5M11 5l2 2M9.5 6.5l2 2"/></svg>;
    case "pricing":
      return <svg {...common}><circle cx="8" cy="8" r="6.2"/><path d="M8 4.2v7.6M10 6.2c0-.9-.9-1.4-2-1.4s-2 .5-2 1.3c0 1.9 4 .9 4 2.8 0 .9-.9 1.4-2 1.4s-2-.5-2-1.3"/></svg>;
    case "launchAtLogin":
      return <svg {...common}><path d="M8 2v6M4.3 4.6a5.2 5.2 0 1 0 7.4 0"/></svg>;
    case "done":
      return <svg {...common}><circle cx="8" cy="8" r="6.2"/><path d="M5.2 8.2l1.9 1.9 3.8-4"/></svg>;
  }
}

function Card({ children }: { children: ReactNode }) {
  return <div className="wb-card ob-card">{children}</div>;
}

function Switch({ on, onChange, label }: { on: boolean; onChange: (next: boolean) => void; label: string }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} className={`wb-switch${on ? " on" : ""}`} onClick={() => onChange(!on)} />
  );
}

/** The first-run setup assistant: a step list on the left, one step's
 *  content and its Back / Skip / Continue on the right — the native
 *  `OnboardingView` in the porcelain language. Everything it changes goes
 *  through the shared settings writer, so both clients see it. */
export function OnboardingAssistant({
  info,
  settings,
  onClose,
  onSettingsChanged,
}: {
  info: AppInfo | null;
  settings: PresentationSettings | null;
  onClose: () => void;
  onSettingsChanged: () => void;
}) {
  const [step, setStep] = useState<StepId>("welcome");
  const [visited, setVisited] = useState<Set<StepId>>(new Set(["welcome"]));
  const [raw, setRaw] = useState<Record<string, unknown> | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  const [pricingRows, setPricingRows] = useState<number | null>(null);
  const [finishing, setFinishing] = useState(false);
  const index = STEPS.findIndex((s) => s.id === step);
  const current = STEPS[index];

  useEffect(() => {
    api.sharedSettingsRaw().then(setRaw).catch(() => setRaw({}));
    api.autostartEnabled().then(setAutostart).catch(() => setAutostart(false));
    api.pricingEffective().then((rows) => setPricingRows(rows.length)).catch(() => setPricingRows(0));
  }, []);

  const save = async (changes: Record<string, unknown>) => {
    setRaw((currentRaw) => ({ ...(currentRaw ?? {}), ...changes }));
    await api.saveSharedSettings(changes);
    onSettingsChanged();
  };

  const visibleCore = useMemo(() => {
    const list = raw?.visibleCoreProviders;
    return Array.isArray(list) ? new Set(list as string[]) : new Set(CORE.map((c) => c.tool));
  }, [raw]);
  const setCoreVisible = (tool: string, on: boolean) => {
    const next = new Set(visibleCore);
    if (on) next.add(tool);
    else next.delete(tool);
    void save({ visibleCoreProviders: CORE.map((c) => c.tool).filter((t) => next.has(t)) });
  };

  const instances = useMemo(() => {
    const list = raw?.miscProviderInstances;
    return Array.isArray(list) ? (list as Record<string, unknown>[]) : (settings?.miscProviderInstances ?? []).map((i) => ({ ...i }));
  }, [raw, settings]);
  const setMiscVisible = (id: string, on: boolean) => {
    const next = instances.map((i) => (i.id === id ? { ...i, isVisible: on } : i));
    void save({ miscProviderInstances: next, visibleMiscProviders: next.filter((i) => i.isVisible !== false).map((i) => String(i.id)) });
  };

  const go = (id: StepId) => {
    setStep(id);
    setVisited((v) => new Set([...v, id]));
  };
  const finish = async () => {
    setFinishing(true);
    try {
      // The shared flag both clients honour; the native gate reads it too.
      await save({ hasCompletedOnboarding: true });
    } finally {
      setFinishing(false);
      onClose();
    }
  };

  const visibleMiscCount = instances.filter((i) => i.isVisible !== false).length;
  const content: ReactNode = (() => {
    switch (step) {
      case "welcome":
        return (
          <>
            <Card>
              <p className="ob-text">
                Vibe Bar sits in your menu bar and shows, at a glance, how much of each AI subscription you have left, what your coding agents are spending, and which sessions and skills live on this Mac. It reads the credentials the Codex, Claude Code, Gemini and Grok CLIs already keep here, adds web quotas from your browser's cookies when you ask it to, and never sends any of it anywhere but the provider it came from.
              </p>
              <div className="ob-rule" />
              <Feature title="Subscription quotas" detail="Codex, Claude Code, Gemini, Grok and a shelf of API-key plans, each with its reset countdown." />
              <Feature title="Token cost" detail="Priced locally from the agents' own session logs against a merged model price catalog." />
              <Feature title="Sessions and skills" detail="Browse, search and tidy agent sessions and shared skills from the Workbench." />
              <Feature title="Local MCP server" detail="Your agents can ask Vibe Bar for quota and cost over a Unix socket in your home directory." />
            </Card>
            <p className="ob-note">This takes about two minutes. Every choice here can be changed later in Settings, and the assistant is one click away under Settings → System.</p>
          </>
        );
      case "subscriptions":
        return (
          <>
            <p className="ob-text">Turn on the subscriptions you use. A provider that is off stays out of the Overview and the menu bar; turning one off later keeps its credentials and history.</p>
            <div className="ob-grid">
              {CORE.map((core) => (
                <div key={core.tool} className="wb-card ob-provider">
                  <div className="ob-provider-head">
                    <ToolBrandBadge tool={core.tool} iconSize={16} containerSize={22} />
                    <div className="ob-provider-titles">
                      <div className="ob-provider-vendor">{core.vendor}</div>
                      <div className="ob-provider-products">{core.products}</div>
                    </div>
                  </div>
                  <div className="ob-line">
                    <span className="ob-label">Show in Overview</span>
                    <Switch on={visibleCore.has(core.tool)} onChange={(on) => setCoreVisible(core.tool, on)} label={`Show ${core.vendor} in Overview`} />
                  </div>
                </div>
              ))}
            </div>
          </>
        );
      case "browserCookies":
        return (
          <>
            <Card>
              <p className="ob-text">Web quotas — the ones the provider shows on its own site — come from the session your browser already has. Vibe Bar reads the cookie stores of the browsers on this Mac (Chrome and other Chromium browsers, Safari, Firefox), keeps only the few cookies each provider needs, and stores them in your login Keychain, never in a file.</p>
              <div className="ob-rule" />
              {CORE.map((core) => (
                <div key={core.tool} className="ob-line">
                  <ToolBrandBadge tool={core.tool} iconSize={14} containerSize={20} />
                  <span className="ob-label">{core.vendor}</span>
                  <span className="ob-status">Not imported yet</span>
                </div>
              ))}
            </Card>
            <p className="ob-note">Sign in to chatgpt.com, claude.ai, gemini.google.com or grok.com in your browser first — an import can only find a session that exists. Providers you have not signed in to simply report nothing.</p>
            <p className="ob-note">Importing from browsers is the native app's job in this Desktop release: run its import once and both clients read the same Keychain. Desktop's own importer arrives with a later release.</p>
          </>
        );
      case "apiKeyProviders":
        return (
          <>
            <p className="ob-text">These plans are tracked with an API key or a console cookie rather than a CLI login. Tick the ones you have — a ticked provider gets a card on the Misc page. Keys and cookies are entered in Settings, under each provider.</p>
            {instances.length === 0 ? (
              <p className="ob-note">No API-key plans are configured yet; add one later under Settings → Misc Providers.</p>
            ) : (
              <Card>
                {instances.map((instance) => (
                  <div key={String(instance.id)} className="ob-line">
                    <ToolBrandBadge tool={String(instance.tool)} iconSize={14} containerSize={20} />
                    <span className="ob-label">{String(instance.name ?? instance.tool)}</span>
                    <Switch on={instance.isVisible !== false} onChange={(on) => setMiscVisible(String(instance.id), on)} label={`Show ${String(instance.name ?? instance.tool)} on the Misc page`} />
                  </div>
                ))}
              </Card>
            )}
          </>
        );
      case "pricing":
        return (
          <>
            <Card>
              <p className="ob-text">Token cost is computed on this Mac from your agents' session logs, priced with a catalog merged from several public price lists — higher-priority entries win when a model appears in more than one, your own overrides beat them all, and a bundled table is the offline floor. Catalogs refresh in the background.</p>
              <div className="ob-line">
                <span className="ob-label">Catalog</span>
                <span className="ob-status">{pricingRows === null ? "…" : pricingRows > 0 ? `Merged · ${pricingRows.toLocaleString("en-US")} models` : "Not fetched yet — the bundled table is in use."}</span>
              </div>
            </Card>
            <p className="ob-note">Per-model prices and your overrides live under Settings → Model Pricing.</p>
          </>
        );
      case "launchAtLogin":
        return (
          <>
            <Card>
              <div className="ob-line">
                <span className="ob-label">Launch Vibe Bar at login</span>
                <Switch
                  on={autostart === true}
                  label="Launch Vibe Bar at login"
                  onChange={(on) => {
                    setAutostartError(null);
                    api.setAutostart(on).then(setAutostart).catch((error: unknown) => setAutostartError(String(error)));
                  }}
                />
              </div>
              {autostartError ? <p className="ob-note ob-error">{autostartError}</p> : null}
            </Card>
            <p className="ob-note">Vibe Bar is a menu-bar app with no Dock icon. Starting it at login keeps the quota readout and the local MCP server available from the moment you sign in; macOS may ask you to approve the login item in System Settings the first time.</p>
          </>
        );
      case "done":
        return (
          <>
            <p className="ob-text">That is everything the menu bar needs. Finish opens the Workbench with whatever quota is already readable; the rest fills in on the first refresh.</p>
            <Card>
              <Summary title="Subscriptions" value={CORE.filter((c) => visibleCore.has(c.tool)).map((c) => c.vendor).join(", ") || "None"} />
              <Summary title="Browser cookies" value="Imported by the native app" />
              <Summary title="API-key providers" value={`${visibleMiscCount} on the Misc page`} />
              <Summary title="Model pricing" value={pricingRows ? `${pricingRows.toLocaleString("en-US")} models` : "Bundled table"} />
              <Summary title="Launch at login" value={autostart ? "On" : "Off"} />
            </Card>
            <p className="ob-note">Every one of these lives in Settings, and the assistant is one click away under Settings → System.</p>
          </>
        );
    }
  })();

  return (
    <div className="ob-backdrop" role="dialog" aria-modal="true" aria-label="Setup assistant">
      <div className="ob-window">
        <aside className="ob-steps">
          <div className="ob-steps-head">
            <span className="ob-steps-title">Setup</span>
            <span className="ob-steps-version">{info?.version ?? ""}</span>
          </div>
          {STEPS.map((s, i) => {
            const selected = s.id === step;
            const completed = visited.has(s.id) && i < index;
            return (
              <button type="button" key={s.id} className={`ob-step${selected ? " selected" : ""}${completed ? " completed" : ""}`} onClick={() => go(s.id)}>
                <span className="ob-step-glyph">{completed && !selected ? <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M3.5 8.5l3 3 6-7"/></svg> : <Glyph id={s.id} size={11} />}</span>
                <span className="ob-step-title">{s.title}</span>
              </button>
            );
          })}
        </aside>
        <div className="ob-pane">
          <div className="ob-head">
            <div className="ob-title">{current.title}</div>
            <div className="ob-subtitle">{current.subtitle}</div>
          </div>
          <div className="ob-content">{content}</div>
          <div className="ob-footer">
            {index > 0 ? <button type="button" className="wb-pill" onClick={() => go(STEPS[index - 1].id)}>Back</button> : null}
            <span className="wb-spacer" />
            {step !== "done" ? <button type="button" className="wb-pill" disabled={finishing} onClick={() => void finish()}>Skip for now</button> : null}
            {step !== "done" ? (
              <button type="button" className="wb-pill prominent" onClick={() => go(STEPS[index + 1].id)}>Continue</button>
            ) : (
              <button type="button" className="wb-pill prominent" disabled={finishing} onClick={() => void finish()}>{finishing ? "Finishing…" : "Finish"}</button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function Feature({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="ob-feature">
      <span className="ob-feature-dot" aria-hidden="true" />
      <div>
        <div className="ob-feature-title">{title}</div>
        <div className="ob-feature-detail">{detail}</div>
      </div>
    </div>
  );
}

function Summary({ title, value }: { title: string; value: string }) {
  return (
    <div className="ob-summary">
      <span className="ob-summary-title">{title}</span>
      <span className="ob-summary-value">{value}</span>
    </div>
  );
}
