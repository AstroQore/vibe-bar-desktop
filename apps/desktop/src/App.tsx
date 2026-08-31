import { useCallback, useEffect, useMemo, useState } from "react";

import type { AppInfo, CostView, PresentationSettings, QuotaView, ServiceStatusView } from "./api";
import { api, formatRelative } from "./api";
import { About } from "./components/About";
import { CostOverview } from "./components/CostOverview";
import {
  ProviderTabs,
  activeCompany,
  showsProviderDetail,
  visibleCompanies,
} from "./components/ProviderTabs";
import { Overview } from "./components/Overview";
import { Resets } from "./components/Resets";
import { Sessions } from "./components/Sessions";
import { ServiceStatus } from "./components/ServiceStatus";
import { Settings } from "./components/Settings";
import { Skills } from "./components/Skills";

type Tab = "overview" | "resets" | "sessions" | "skills" | "settings" | "about";

export function App() {
  const [tab, setTab] = useState<Tab>("overview");
  /** Which provider page the Quota tab is showing; empty is the overview.
   *  A second level rather than more top-level tabs, which is how the native
   *  popover arranges it: the provider row belongs to the quota surface. */
  const [company, setCompany] = useState("");
  const [view, setView] = useState<QuotaView | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [presentation, setPresentation] = useState<PresentationSettings | null>(null);
  const [serviceStatus, setServiceStatus] = useState<ServiceStatusView | null>(null);
  const [statusRefreshFailed, setStatusRefreshFailed] = useState(false);
  const [cost, setCost] = useState<CostView | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  /** Settings chosen here that the native app has since changed. Null the rest
   *  of the time, including for the far commoner case of it changing something
   *  nobody here touched — that is taken on silently, since nothing was lost. */
  const [replacedSettings, setReplacedSettings] = useState<string[] | null>(null);
  // Computed once: the row, the filter, the cost card and the detail flag all
  // have to agree about which pages exist, or a stale selection leaves the
  // list empty with no control to escape it.
  const companies = useMemo(
    () => (view ? visibleCompanies(view, presentation) : []),
    [view, presentation],
  );
  const page = activeCompany(companies, company);
  const detailed = showsProviderDetail(companies, company);

  const showsQuotaRefresh = tab === "overview" || tab === "resets";

  useEffect(() => {
    // Cached view first so the window paints immediately, then live updates
    // arrive from the background refresh loop.
    api.quotaView().then(setView).catch(() => undefined);
    api.appInfo().then(setInfo).catch(() => undefined);
    api.presentationSettings().then(setPresentation).catch(() => undefined);
    api
      .costView()
      .then(setCost)
      .catch(() => undefined);
    api
      .statusSnapshot()
      .then(setServiceStatus)
      .catch(() => undefined)
      .finally(() => {
        api
          .refreshStatus()
          .then((status) => {
            setServiceStatus(status);
            setStatusRefreshFailed(false);
          })
          .catch(() => setStatusRefreshFailed(true));
      });
    const unlisten = api.onQuotaUpdated(setView);
    // The settings file is shared: re-read it whenever the other client writes,
    // rather than showing what it said when this window opened.
    const unlistenSettings = api.onSettingsChanged((replacedKeys) => {
      api.presentationSettings().then(setPresentation).catch(() => undefined);
      if (replacedKeys?.length) setReplacedSettings(replacedKeys);
    });
    return () => {
      unlisten.then((off) => off()).catch(() => undefined);
      unlistenSettings.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const saveSettings = useCallback((changes: Record<string, unknown>) => {
    // The command returns the settings as they read after the write, which is
    // not always what was asked for: the native app may have changed the same
    // one in between, and it wins.
    api
      .saveSharedSettings(changes)
      .then(setPresentation)
      .catch(() => api.presentationSettings().then(setPresentation).catch(() => undefined));
  }, []);

  const refresh = useCallback(() => {
    setRefreshing(true);
    Promise.allSettled([
      api.refreshQuota().then(setView),
      api.presentationSettings().then(setPresentation),
      api.refreshCost().then(setCost),
      api.refreshStatus().then((status) => {
        setServiceStatus(status);
        setStatusRefreshFailed(false);
      }),
    ])
      .then((results) => {
        if (results[3]?.status === "rejected") setStatusRefreshFailed(true);
      })
      .finally(() => setRefreshing(false));
  }, []);

  return (
    <div className="app">
      {/* Drag region for the overlay title bar. */}
      <div className="titlebar" data-tauri-drag-region />

      <header className="header">
        <nav className="tabs" role="tablist">
          {(["overview", "resets", "sessions", "skills", "settings", "about"] as const).map((id) => (
            <button
              key={id}
              className="tab"
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
            >
              {id === "overview"
                ? "Quota"
                : id === "resets"
                  ? "Resets"
                  : id === "sessions"
                    ? "Sessions"
                    : id === "skills"
                      ? "Skills"
                      : id === "settings"
                        ? "Settings"
                        : "About"}
            </button>
          ))}
        </nav>

        {showsQuotaRefresh ? (
          <>
            <span className="status-line">
              updated {formatRelative(view?.lastUpdated)}
            </span>
            <button onClick={refresh} disabled={refreshing}>
              {refreshing ? "Refreshing…" : "Refresh"}
            </button>
          </>
        ) : null}
      </header>

      <main className="content">
        {tab === "overview" ? (
          view ? (
            <>
              <ProviderTabs
                companies={companies}
                selected={company}
                onSelect={setCompany}
              />
              <ServiceStatus
                status={serviceStatus}
                refreshFailed={statusRefreshFailed}
                company={page || undefined}
              />
              {/* The cost card is a whole-machine total, so it belongs to the
                  overview rather than to any one provider's page. */}
              {page === "" && <CostOverview cost={cost} />}
              <Overview
                view={view}
                settings={presentation}
                company={page || undefined}
                detailed={detailed}
              />
            </>
          ) : (
            <p className="empty">Loading quota…</p>
          )
        ) : tab === "resets" ? (
          view ? (
            <Resets view={view} settings={presentation} />
          ) : (
            <p className="empty">Loading quota…</p>
          )
        ) : tab === "sessions" ? (
          <Sessions />
        ) : tab === "skills" ? (
          <Skills />
        ) : tab === "settings" ? (
          <Settings
            settings={presentation}
            replacedKeys={replacedSettings}
            onSave={saveSettings}
            onDismissReplaced={() => setReplacedSettings(null)}
          />
        ) : (
          <About info={info} view={view} />
        )}
      </main>
    </div>
  );
}
