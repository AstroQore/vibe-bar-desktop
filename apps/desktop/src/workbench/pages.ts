/**
 * Native `WorkbenchPage`: the five pages, their titles, subtitles, symbols
 * and sidebar accents, in sidebar order. Settings sits below the primary
 * four, as native draws it.
 */
export type WorkbenchPageId = "usageStats" | "sessionManager" | "resets" | "skillsManager" | "settings";

export interface WorkbenchPage {
  id: WorkbenchPageId;
  title: string;
  subtitle: string;
  /** The SF Symbol native uses; `icons.tsx` carries the equivalent. */
  symbol: string;
  /** Sidebar row accent (native `WorkbenchSidebar.accent(for:)`). */
  accent: string;
}

export const PAGES: Record<WorkbenchPageId, WorkbenchPage> = {
  usageStats: { id: "usageStats", title: "Usage Stats", subtitle: "Local per-request ledger · all providers", symbol: "chart.xyaxis.line", accent: "#4E5FE0" },
  sessionManager: { id: "sessionManager", title: "Sessions", subtitle: "Search and resume local agent sessions", symbol: "bubble.left.and.text.bubble.right", accent: "#14A97C" },
  resets: { id: "resets", title: "Resets", subtitle: "Cycles, refills, and run-out forecasts", symbol: "clock.arrow.circlepath", accent: "#5886DC" },
  skillsManager: { id: "skillsManager", title: "Skills", subtitle: "One shared skill library · every agent CLI", symbol: "puzzlepiece.extension", accent: "#D9890B" },
  settings: { id: "settings", title: "Settings", subtitle: "Appearance, providers, data, privacy, and sync", symbol: "gearshape", accent: "secondary" },
};

export const PRIMARY_PAGES: WorkbenchPageId[] = ["usageStats", "sessionManager", "resets", "skillsManager"];

/** Native: 1180×820 window, 1040×680 minimum, 206pt sidebar, 236pt settings sidebar. */
export const WORKBENCH = { width: 1180, height: 820, minWidth: 1040, minHeight: 680, sidebarWidth: 206, settingsSidebarWidth: 236 } as const;
