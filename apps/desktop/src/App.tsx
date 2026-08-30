import { useCallback, useEffect, useState } from "react";

import type { AppInfo, QuotaView } from "./api";
import { api, formatRelative } from "./api";
import { About } from "./components/About";
import { Overview } from "./components/Overview";
import { Sessions } from "./components/Sessions";

type Tab = "overview" | "sessions" | "about";

export function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [view, setView] = useState<QuotaView | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    // Cached view first so the window paints immediately, then live updates
    // arrive from the background refresh loop.
    api.quotaView().then(setView).catch(() => undefined);
    api.appInfo().then(setInfo).catch(() => undefined);
    const unlisten = api.onQuotaUpdated(setView);
    return () => {
      unlisten.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const refresh = useCallback(() => {
    setRefreshing(true);
    api
      .refreshQuota()
      .then(setView)
      .catch(() => undefined)
      .finally(() => setRefreshing(false));
  }, []);

  return (
    <div className="app">
      {/* Drag region for the overlay title bar. */}
      <div className="titlebar" data-tauri-drag-region />

      <header className="header">
        <nav className="tabs" role="tablist">
          {(["overview", "sessions", "about"] as const).map((id) => (
            <button
              key={id}
              className="tab"
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
            >
              {id === "overview" ? "Quota" : id === "sessions" ? "Sessions" : "About"}
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
            <Overview view={view} />
          ) : (
            <p className="empty">Loading quota…</p>
          )
        ) : tab === "sessions" ? (
          <Sessions />
        ) : (
          <About info={info} view={view} />
        )}
      </main>
    </div>
  );
}
