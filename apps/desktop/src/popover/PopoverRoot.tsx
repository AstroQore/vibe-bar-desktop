/**
 * Native `PopoverRoot`: one tabbed shell for the Overview and the provider
 * pages. Header band, tab strip, four icon buttons, a hairline, then the page
 * at the shell's content width. Every tab uses the same density so switching
 * pages never re-flows the frame.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppInfo, CostView, PresentationSettings, QuotaView, ServiceStatusView } from "../api";
import { api } from "../api";
import { OverviewWaterfall, type WaterfallCard } from "./OverviewWaterfall";
import { CostSummaryCard, IconButton, ProviderQuotaCard, StatusSummaryCard, UpcomingResetsCard, UsageMixCard } from "./cards";
import { CORE_PAGES, overviewDescriptors, visibleCorePages } from "./data";
import { updatedAgo } from "./format";
import { ArrowClockwise, ChartBar, Gear, Grid2x2, MacWindow, RectOnRect, ServerRack } from "./icons";
import { ToolBrandIcon } from "./brand";
import { overviewDensity, popoverDensity, type Density } from "./theme";

export type PageId = "overview" | "openAI" | "claude" | "googleAI" | "grok" | "misc" | "machines";

const PAGE_TITLE: Record<PageId, string> = {
  overview: "Overview", openAI: "OpenAI", claude: "Anthropic", googleAI: "Google AI", grok: "SpaceXAI", misc: "Misc Providers", machines: "Machines",
};
const PAGE_LABEL: Record<PageId, string> = { ...PAGE_TITLE, misc: "Misc" };
const PAGE_SUBTITLE: Partial<Record<PageId, string>> = {
  overview: "All providers · quota & cost",
  misc: "Usage-only · sign in or paste a key",
  machines: "End-to-end encrypted remote usage",
};

export interface PopoverData {
  view: QuotaView | null;
  settings: PresentationSettings | null;
  cost: CostView | null;
  status: ServiceStatusView | null;
  info: AppInfo | null;
  refreshing: boolean;
  statusRefreshing: boolean;
  costRefreshing: boolean;
}

export interface PopoverActions {
  refreshAll: () => void;
  refreshCost: () => void;
  refreshStatus: () => void;
  refreshProvider: (tool: string) => void;
  toggleMini: () => void;
  showWorkbench: () => void;
  showSettings: () => void;
}

/** Density as CSS variables, so leaves read the caller's density and nothing else. */
export function densityVars(d: Density): Record<string, string> {
  return {
    "--pv-padding-h": `${d.popoverPaddingH}px`, "--pv-padding-v": `${d.popoverPaddingV}px`,
    "--pv-section-gap": `${d.interSectionSpacing}px`, "--pv-card-padding": `${d.cardPadding}px`,
    "--pv-card-gap": `${d.cardSpacing}px`, "--pv-bucket-gap": `${d.bucketRowSpacing}px`,
    "--pv-group-gap": `${d.bucketGroupSpacing}px`, "--pv-radius": `${d.cardCornerRadius}px`,
    "--pv-title": `${d.titleFontSize}px`, "--pv-subtitle": `${d.subtitleFontSize}px`,
    "--pv-bucket-title": `${d.bucketTitleFontSize}px`, "--pv-bucket-percent": `${d.bucketPercentFontSize}px`,
    "--pv-countdown": `${d.resetCountdownFontSize}px`, "--pv-bar-height": `${d.bucketBarHeight}px`,
    "--pv-segmented": `${d.segmentedFontSize}px`, "--pv-header-height": `${d.headerHeight}px`,
  };
}

export function PopoverRoot({ data, actions, initialPage = "overview", now = Date.now() / 1000, dark, onSize }: {
  data: PopoverData; actions: PopoverActions; initialPage?: PageId; now?: number; dark: boolean;
  /** The shell sizes the window to the content; the preview page has no shell. */
  onSize?: (width: number, height: number) => void;
}) {
  const [page, setPage] = useState<PageId>(initialPage);
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!onSize || !rootRef.current) return;
    const el = rootRef.current;
    const report = () => onSize(Math.ceil(el.getBoundingClientRect().width), Math.ceil(el.getBoundingClientRect().height));
    const observer = new ResizeObserver(report);
    observer.observe(el);
    report();
    return () => observer.disconnect();
  }, [onSize]);
  const density = overviewDensity(popoverDensity(data.settings?.popoverDensity));
  const width = density.popoverWidth;
  const contentWidth = Math.max(0, width - density.popoverPaddingH * 2);
  const mode = data.settings?.displayMode === "used" ? "used" : "remaining";
  const refreshInterval = data.settings?.refreshIntervalSeconds ?? 600;

  const visiblePages = useMemo<PageId[]>(() => {
    const core = data.view ? visibleCorePages(data.view, data.settings).map((p) => p.id as PageId) : [];
    return ["overview", ...core, "machines", "misc"];
  }, [data.view, data.settings]);
  useEffect(() => { if (!visiblePages.includes(page)) setPage("overview"); }, [visiblePages, page]);

  const subtitle = [PAGE_SUBTITLE[page], data.refreshing ? "Refreshing…" : updatedAgo(data.view?.lastUpdated, now)].filter(Boolean).join(" · ");

  return (
    <div ref={rootRef} className="pv-root" style={{ width, ...densityVars(density) } as React.CSSProperties}>
      <header className="pv-header" style={{ height: density.headerHeight }}>
        <div className="pv-header-titles">
          <span className="pv-header-title" style={{ fontSize: density.titleFontSize + 2 }}>{PAGE_TITLE[page]}</span>
          <span className="pv-header-subtitle">{subtitle}</span>
        </div>
        <span className="pv-spacer" />
        <nav className="pv-tabs" role="tablist">
          {visiblePages.map((id) => (
            <button key={id} type="button" role="tab" aria-selected={page === id} className={`pv-tab${page === id ? " selected" : ""}`} onClick={() => setPage(id)} title={`Show ${PAGE_LABEL[id]}`}>
              <TabIcon page={id} />
              <span>{PAGE_LABEL[id]}</span>
            </button>
          ))}
        </nav>
        <IconButton title="Refresh" onClick={actions.refreshAll} spinning={data.refreshing}><ArrowClockwise size={12} /></IconButton>
        <IconButton title="Mini" onClick={actions.toggleMini}><RectOnRect size={12} /></IconButton>
        <IconButton title="Open Workbench" onClick={actions.showWorkbench}><MacWindow size={12} /></IconButton>
        <IconButton title="Settings" onClick={actions.showSettings}><Gear size={12} /></IconButton>
      </header>
      <hr className="pv-header-rule" />
      <div className="pv-scroll" style={{ maxHeight: Math.max(360, (window.screen?.availHeight ?? 900) - 150) }}>
        <div className="pv-page" style={{ width: contentWidth }}>
          {page === "overview" ? (
            data.view ? <Overview data={data} actions={actions} density={density} width={contentWidth} now={now} mode={mode} refreshInterval={refreshInterval} dark={dark} /> : <p className="pv-empty">Loading quota…</p>
          ) : page === "misc" ? (
            <p className="pv-empty">Misc providers arrive with the provider settings port.</p>
          ) : page === "machines" ? (
            <p className="pv-empty">Remote machines are not part of this client yet.</p>
          ) : data.view ? (
            <ProviderPage data={data} actions={actions} page={page} now={now} mode={mode} refreshInterval={refreshInterval} />
          ) : <p className="pv-empty">Loading quota…</p>}
        </div>
      </div>
    </div>
  );
}

function TabIcon({ page }: { page: PageId }) {
  const size = 13;
  switch (page) {
    case "overview": return <ChartBar size={size} />;
    case "misc": return <Grid2x2 size={size} />;
    case "machines": return <ServerRack size={size} />;
    default: {
      const core = CORE_PAGES.find((p) => p.id === page);
      return core ? <ToolBrandIcon tool={core.tool} size={size} /> : null;
    }
  }
}

function Overview({ data, actions, density, width, now, mode, refreshInterval, dark }: {
  data: PopoverData; actions: PopoverActions; density: Density; width: number; now: number; mode: "remaining" | "used"; refreshInterval: number; dark: boolean;
}) {
  const view = data.view!;
  const descriptors = overviewDescriptors(view, data.settings, data.cost);
  const coreTools = visibleCorePages(view, data.settings).map((p) => p.tool);
  const cards: WaterfallCard[] = descriptors.map((d) => {
    switch (d.kind.kind) {
      case "costSummary":
        return { id: d.id, phase: d.phase, node: <CostSummaryCard cost={data.cost} now={now} pinnedHeight={density.overviewSummaryHeight} onRefresh={actions.refreshCost} refreshing={data.costRefreshing} /> };
      case "statusSummary":
        return { id: d.id, phase: d.phase, node: <StatusSummaryCard status={data.status} tools={coreTools} now={now} pinnedHeight={density.overviewSummaryHeight} refreshing={data.statusRefreshing} onRefresh={actions.refreshStatus} /> };
      case "upcomingResets":
        return { id: d.id, phase: d.phase, node: <UpcomingResetsCard view={view} settings={data.settings} now={now} /> };
      case "usageMix":
        return { id: d.id, phase: d.phase, node: <UsageMixCard cost={data.cost} dark={dark} onRefresh={actions.refreshCost} refreshing={data.costRefreshing} /> };
      case "quota":
        return { id: d.id, phase: d.phase, node: <ProviderQuotaCard view={view} settings={data.settings} company={d.kind.company} tool={d.kind.tool} now={now} mode={mode} refreshIntervalSeconds={refreshInterval} onRefresh={actions.refreshProvider} refreshing={data.refreshing} /> };
    }
  });
  return <OverviewWaterfall cards={cards} width={width} density={density} />;
}

function ProviderPage({ data, actions, page, now, mode, refreshInterval }: {
  data: PopoverData; actions: PopoverActions; page: PageId; now: number; mode: "remaining" | "used"; refreshInterval: number;
}) {
  const core = CORE_PAGES.find((p) => p.id === page)!;
  return (
    <div className="pv-stack">
      <ProviderQuotaCard view={data.view!} settings={data.settings} company={core.company} tool={core.tool} now={now} mode={mode} refreshIntervalSeconds={refreshInterval} onRefresh={actions.refreshProvider} refreshing={data.refreshing} />
    </div>
  );
}

/** The live popover: the shell's data, the shell's commands. */
export function PopoverApp() {
  const [view, setView] = useState<QuotaView | null>(null);
  const [settings, setSettings] = useState<PresentationSettings | null>(null);
  const [cost, setCost] = useState<CostView | null>(null);
  const [status, setStatus] = useState<ServiceStatusView | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [costRefreshing, setCostRefreshing] = useState(false);
  const [statusRefreshing, setStatusRefreshing] = useState(false);
  const [dark, setDark] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false);

  useEffect(() => {
    api.quotaView().then(setView).catch(() => undefined);
    api.presentationSettings().then(setSettings).catch(() => undefined);
    api.costView().then(setCost).catch(() => undefined);
    api.statusSnapshot().then(setStatus).catch(() => undefined);
    api.appInfo().then(setInfo).catch(() => undefined);
    const off = api.onQuotaUpdated(setView);
    const offSettings = api.onSettingsChanged(() => { api.presentationSettings().then(setSettings).catch(() => undefined); });
    // Transient, as native's popover: the moment attention goes elsewhere,
    // it is gone. The window's own focus event is the first line; this is
    // the one that also fires when another *application* comes forward.
    const onBlur = () => { api.hidePopover().catch(() => undefined); };
    window.addEventListener("blur", onBlur);
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const onScheme = (e: MediaQueryListEvent) => setDark(e.matches);
    mq?.addEventListener("change", onScheme);
    return () => {
      off.then((f) => f()).catch(() => undefined);
      offSettings.then((f) => f()).catch(() => undefined);
      mq?.removeEventListener("change", onScheme);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  const refreshAll = useCallback(() => {
    setRefreshing(true); setCostRefreshing(true); setStatusRefreshing(true);
    Promise.allSettled([
      api.refreshQuota().then(setView),
      api.refreshCost().then(setCost).finally(() => setCostRefreshing(false)),
      api.refreshStatus().then(setStatus).finally(() => setStatusRefreshing(false)),
    ]).finally(() => setRefreshing(false));
  }, []);
  const refreshCost = useCallback(() => { setCostRefreshing(true); api.refreshCost().then(setCost).catch(() => undefined).finally(() => setCostRefreshing(false)); }, []);
  const refreshStatus = useCallback(() => { setStatusRefreshing(true); api.refreshStatus().then(setStatus).catch(() => undefined).finally(() => setStatusRefreshing(false)); }, []);
  const refreshProvider = useCallback(() => { setRefreshing(true); api.refreshQuota().then(setView).catch(() => undefined).finally(() => setRefreshing(false)); }, []);

  const actions: PopoverActions = useMemo(() => ({
    refreshAll, refreshCost, refreshStatus, refreshProvider,
    toggleMini: () => { api.toggleMini().catch(() => undefined); },
    showWorkbench: () => { api.showMainWindow("overview").catch(() => undefined); },
    showSettings: () => { api.showMainWindow("settings").catch(() => undefined); },
  }), [refreshAll, refreshCost, refreshStatus, refreshProvider]);

  const onSize = useCallback((width: number, height: number) => {
    api.resizePopover(width, height).catch(() => undefined);
  }, []);

  return (
    <PopoverRoot
      data={{ view, settings, cost, status, info, refreshing, costRefreshing, statusRefreshing }}
      actions={actions}
      dark={dark}
      onSize={onSize}
    />
  );
}
