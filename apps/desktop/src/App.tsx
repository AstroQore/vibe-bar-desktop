import { useCallback, useEffect, useState } from "react";

import type { AppInfo, CostView, PresentationSettings, QuotaView } from "./api";
import { api, formatRelative } from "./api";
import { UsageStatsPage } from "./workbench/usage/UsageStatsPage";
import { ResetsPage } from "./workbench/resets/ResetsPage";
import { SessionsPage } from "./workbench/sessions/SessionsPage";
import { SettingsPage } from "./workbench/settings/SettingsPage";
import { SkillsPage } from "./workbench/skills/SkillsPage";
import { WorkbenchRoot, useAppearance } from "./workbench/WorkbenchRoot";
import type { WorkbenchPageId } from "./workbench/pages";
import "./workbench/porcelain.css";


/** `?page=<id>` opens that page first — how the tray, `show_main_window`,
 *  and a headless screenshot land on a page without a click. */
function initialPage(): WorkbenchPageId {
  const wanted = new URLSearchParams(window.location.search).get("page");
  const known: WorkbenchPageId[] = ["usageStats", "sessionManager", "resets", "skillsManager", "settings"];
  return known.includes(wanted as WorkbenchPageId) ? (wanted as WorkbenchPageId) : "usageStats";
}

export function App() {
  const [tab, setTab] = useState<WorkbenchPageId>(initialPage());
  const [dark, toggleDark] = useAppearance();
  const [view, setView] = useState<QuotaView | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [presentation, setPresentation] = useState<PresentationSettings | null>(null);
  const [cost, setCost] = useState<CostView | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshToken, setRefreshToken] = useState(0);
  const [menuBarBlocked, setMenuBarBlocked] = useState(false);
  /** Settings chosen here that the native app has since changed. Null the rest
   *  of the time, including for the far commoner case of it changing something
   *  nobody here touched — that is taken on silently, since nothing was lost. */
  const [replacedSettings, setReplacedSettings] = useState<string[] | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);


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
    const unlisten = api.onQuotaUpdated(setView);
    // The popover's Workbench and Settings buttons land here on a page.
    const unlistenNavigate = api.onNavigate((page) => {
      const map: Record<string, WorkbenchPageId> = { settings: "settings", resets: "resets", sessions: "sessionManager", skills: "skillsManager", overview: "usageStats", usage: "usageStats" };
      setTab(map[page] ?? "usageStats");
    });
    // The settings file is shared: re-read it whenever the other client writes,
    // rather than showing what it said when this window opened.
    const unlistenSettings = api.onSettingsChanged((replacedKeys) => {
      api.presentationSettings().then(setPresentation).catch(() => undefined);
      // Added to, not replaced: a later change that costs nothing is not
      // news that the first one cost nothing, and a second loss is a second
      // thing to say. Only dismissing clears it.
      if (replacedKeys?.length) {
        setReplacedSettings((standing) =>
          [...new Set([...(standing ?? []), ...replacedKeys])].sort(),
        );
      }
    });
    return () => {
      unlisten.then((off) => off()).catch(() => undefined);
      unlistenSettings.then((off) => off()).catch(() => undefined);
      unlistenNavigate.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const saveSettings = useCallback((changes: Record<string, unknown>) => {
    // The command returns the settings as they read after the write, which is
    // not always what was asked for: the native app may have changed the same
    // one in between, and it wins.
    api
      .saveSharedSettings(changes)
      .then((settings) => {
        setPresentation(settings);
        setSaveError(null);
      })
      .catch((error: unknown) => {
        // A control that springs back with no explanation reads as a bug in
        // the app rather than a file it could not write.
        setSaveError(String(error));
        api.presentationSettings().then(setPresentation).catch(() => undefined);
      });
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    api.onMenuBarHealth((report) => setMenuBarBlocked(report.state === "blocked")).then((stop) => {
      unlisten = stop;
    }).catch(() => undefined);
    return () => unlisten?.();
  }, []);
  const refresh = useCallback(() => {
    setRefreshing(true);
    setRefreshToken((token) => token + 1);
    Promise.allSettled([
      api.refreshQuota().then(setView),
      api.presentationSettings().then(setPresentation),
      api.refreshCost().then(setCost),
    ]).finally(() => setRefreshing(false));
  }, []);

  const pages = {
    usageStats: <UsageStatsPage refreshToken={refreshToken} />,
    sessionManager: <SessionsPage refreshToken={refreshToken} />,
    resets: view ? <ResetsPage view={view} settings={presentation} /> : <p className="wb-empty" style={{ padding: 22 }}>Loading quota…</p>,
    skillsManager: <SkillsPage refreshToken={refreshToken} dark={dark} />,
    settings: (
      <SettingsPage
        settings={presentation}
        info={info}
        cost={cost}
        view={view}
        dark={dark}
        onSave={saveSettings}
        replacedKeys={replacedSettings}
        saveError={saveError}
        onDismissReplaced={() => setReplacedSettings(null)}
        onRescanCost={() => api.refreshCost().then(setCost)}
        onCheckConnections={() => api.refreshQuota().then(setView)}
        initialSection={(new URLSearchParams(window.location.search).get("section") ?? "system") as never}
      />
    ),
  } as const;
  const status =
    tab === "usageStats" ? (cost && cost.scannedAt > 0 ? `scanned ${formatRelative(cost.scannedAt)}` : "local ledger")
    : tab === "sessionManager" ? "local index"
    : tab === "resets" ? `updated ${formatRelative(view?.lastUpdated)}`
    : tab === "settings" ? "Shared with the native app"
    : null;
  return (
    <WorkbenchRoot
      page={tab}
      onSelect={setTab}
      pages={pages}
      status={menuBarBlocked ? `${status} · menu bar blocked — see Settings › Menu Bar Health` : status}
      onRefresh={tab === "settings" ? null : refresh}
      refreshing={refreshing}
      version={info?.version}
      dark={dark}
      onToggleDark={toggleDark}
    />
  );
}
