/**
 * Native `WorkbenchRootView`: a 206pt sidebar, a hairline, and a column of
 * page header plus page. The window is 1180×820 and the sidebar lists the
 * four primary pages with Settings below a rule, the version under that.
 */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState, type ReactNode } from "react";
import { PAGES, PRIMARY_PAGES, type WorkbenchPageId } from "./pages";
import { Bubbles, ChartLine, ClockArrow, Gear, Moon, Puzzle, Refresh, Sun } from "./icons";

export interface WorkbenchProps {
  page: WorkbenchPageId;
  onSelect: (page: WorkbenchPageId) => void;
  /** The page bodies, one per id; the shell draws the frame around them. */
  pages: Record<WorkbenchPageId, ReactNode>;
  /** Native's header status: "local ledger", "12 indexed", "Menu Bar"… */
  status?: string | null;
  onRefresh?: (() => void) | null;
  refreshing?: boolean;
  version?: string;
  /** Starts from the system scheme; the header button overrides it. */
  dark: boolean;
  /** Something drawn over the whole window — the setup assistant — rendered
   *  inside the porcelain scope so it has the tokens. */
  overlay?: ReactNode;
  onToggleDark: () => void;
}

function PageIcon({ id, size = 15 }: { id: WorkbenchPageId; size?: number }) {
  switch (id) {
    case "usageStats": return <ChartLine size={size} />;
    case "sessionManager": return <Bubbles size={size} />;
    case "resets": return <ClockArrow size={size} />;
    case "skillsManager": return <Puzzle size={size} />;
    default: return <Gear size={size} />;
  }
}

function SidebarRow({ id, selected, onSelect }: { id: WorkbenchPageId; selected: boolean; onSelect: () => void }) {
  const page = PAGES[id];
  const accent = page.accent === "secondary" ? "var(--wb-secondary)" : page.accent;
  return (
    <button type="button" className={`wb-row${selected ? " selected" : ""}`} onClick={onSelect} aria-current={selected ? "page" : undefined}>
      <span className="wb-row-icon" style={{ color: selected ? accent : "var(--wb-secondary)" }}><PageIcon id={id} /></span>
      <span className="wb-row-title" style={{ color: selected ? accent : undefined }}>{page.title}</span>
    </button>
  );
}

/** The overlay title bar leaves a 36 pt strip above the content. It has to
 *  move the window like a real title bar; the drag-region attribute covers
 *  the strip itself and a press on it starts the drag explicitly too. */
function beginWindowDrag(event: React.MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, a, input, select, textarea")) return;
  void getCurrentWindow().startDragging().catch(() => undefined);
}

export function WorkbenchRoot({ page, onSelect, pages, status, onRefresh, refreshing, version, dark, onToggleDark, overlay }: WorkbenchProps) {
  const current = PAGES[page];
  return (
    <div className={`wb${dark ? " dark" : ""}`}>
      <aside className="wb-sidebar">
        <div className="wb-sidebar-top" data-tauri-drag-region onMouseDown={beginWindowDrag}>
          {PRIMARY_PAGES.map((id) => <SidebarRow key={id} id={id} selected={page === id} onSelect={() => onSelect(id)} />)}
        </div>
        <div className="wb-sidebar-bottom">
          <div className="wb-sidebar-rule" />
          <SidebarRow id="settings" selected={page === "settings"} onSelect={() => onSelect("settings")} />
          <div className="wb-version">Vibe Bar Desktop · {version ?? "—"}</div>
        </div>
      </aside>
      <div className="wb-vrule" />
      <main className="wb-main">
        <div className="wb-drag-strip" data-tauri-drag-region onMouseDown={beginWindowDrag} aria-hidden="true" />
        <header className="wb-header">
          <div className="wb-header-titles">
            <span className="wb-header-title">{current.title}</span>
            <span className="wb-header-subtitle">{current.subtitle}</span>
          </div>
          <span className="wb-spacer" />
          {status ? <span className="wb-header-status">{status}</span> : null}
          <button type="button" className="wb-iconbtn" title={dark ? "Use light appearance" : "Use dark appearance"} onClick={onToggleDark}>
            {dark ? <Sun size={12} /> : <Moon size={12} />}
          </button>
          {onRefresh ? (
            <button type="button" className="wb-iconbtn" title="Refresh" onClick={onRefresh} disabled={refreshing}>
              <Refresh size={12} />
            </button>
          ) : null}
        </header>
        <section className="wb-page">{pages[page]}</section>
      </main>
      {overlay}
    </div>
  );
}

/** The system scheme until the user overrides it from the header. */
export function useAppearance(): [boolean, () => void] {
  // `?appearance=dark|light` pins the appearance — for screenshots and demos,
  // where the system setting is not the point.
  const [override, setOverride] = useState<boolean | null>(() => {
    const wanted = new URLSearchParams(window.location.search).get("appearance");
    return wanted === "dark" ? true : wanted === "light" ? false : null;
  });
  const [system, setSystem] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false);
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystem(e.matches);
    mq?.addEventListener("change", onChange);
    return () => mq?.removeEventListener("change", onChange);
  }, []);
  const dark = override ?? system;
  return [dark, () => setOverride(!dark)];
}
