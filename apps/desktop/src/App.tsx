import { useCallback, useEffect, useState } from "react";

import type { AppInfo, CostView, PresentationSettings, QuotaView, ServiceStatusView } from "./api";
import { api, formatRelative } from "./api";
import { About } from "./components/About";
import { CostOverview } from "./components/CostOverview";
import { Overview } from "./components/Overview";
import { Sessions } from "./components/Sessions";
import { ServiceStatus } from "./components/ServiceStatus";
import { Settings } from "./components/Settings";

type Tab = "overview" | "sessions" | "settings" | "about";

export function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [view, setView] = useState<QuotaView | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [presentation, setPresentation] = useState<PresentationSettings | null>(null);
  const [serviceStatus, setServiceStatus] = useState<ServiceStatusView | null>(null);
  const [statusRefreshFailed, setStatusRefreshFailed] = useState(false);
  const [cost, setCost] = useState<CostView | null>(null);
  const [refreshing, setRefreshing] = useState(false);

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
    return () => {
      unlisten.then((off) => off()).catch(() => undefined);
    };
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
          {(["overview", "sessions", "settings", "about"] as const).map((id) => (
            <button
              key={id}
              className="tab"
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
            >
              {id === "overview"
                ? "Quota"
                : id === "sessions"
                  ? "Sessions"
                  : id === "settings"
                    ? "Settings"
                    : "About"}
            </button>
          ))}
        </nav>

        {tab === "overview" ? (
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
              <ServiceStatus
                status={serviceStatus}
                refreshFailed={statusRefreshFailed}
              />
              <CostOverview cost={cost} />
              <Overview view={view} settings={presentation} />
            </>
          ) : (
            <p className="empty">Loading quota…</p>
          )
        ) : tab === "sessions" ? (
          <Sessions />
        ) : tab === "settings" ? (
          <Settings settings={presentation} />
        ) : (
          <About info={info} view={view} />
        )}
      </main>
    </div>
  );
}
