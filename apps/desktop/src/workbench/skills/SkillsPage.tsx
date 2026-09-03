import { useEffect, useRef, useState } from "react";
import type { SkillInventoryRow, SkillsInventoryView } from "../../api";
import { api } from "../../api";
import { ToolBrandIcon } from "../../popover/brand";
import { accentFor } from "../../components/ResetHistory";
import { Folder, Puzzle, Refresh, Search, XCircle } from "../icons";
import { MANAGED_APPS, WIRING, type SkillAppTarget, activationState, appCountHelp, appCounts, countSummary, filterSkills, healthBadge, helpText, isOn, sourceBadge, wiringLine } from "./model";
import "./skills.css";

const READ_ONLY = "Installing, updating, importing, and backups are the native app's job in this release: this client reads the shared skill library without writing to it.";

function Dots({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor">
      <circle cx="3" cy="8" r="1.5" />
      <circle cx="8" cy="8" r="1.5" />
      <circle cx="13" cy="8" r="1.5" />
    </svg>
  );
}

function ZipIcon({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 1.5h5.5L13 5v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z" />
      <path d="M7 3v2M7 6v2M7 9v2" />
    </svg>
  );
}

function ImportIcon({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round">
      <path d="M8 2v8M5 7l3 3 3-3" />
      <path d="M2.5 10.5v2a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-2" />
    </svg>
  );
}

function ClockIcon({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 4.5V8l2.5 1.5" />
    </svg>
  );
}

function WiringModal({ dark, onClose }: { dark: boolean; onClose: () => void }) {
  return (
    <div className="sk-modal-backdrop" onClick={onClose} role="presentation">
      <div className="sk-modal" role="dialog" aria-label={WIRING.title} onClick={(e) => e.stopPropagation()}>
        <h3>{WIRING.title}</h3>
        <h4>One source of truth.</h4>
        <p>{WIRING.sourceOfTruth}</p>
        <h4>Projections.</h4>
        <p>{WIRING.projections}</p>
        <h4>Native switches.</h4>
        <p>{WIRING.nativeSwitches}</p>
        <div className="sk-wiring">
          {MANAGED_APPS.map((app) => (
            <div className="sk-wiring-row" key={app.id}>
              <b style={{ display: "inline-flex", alignItems: "center", gap: 5, color: accentFor(app.id, dark) }}>
                <ToolBrandIcon tool={app.id} size={12} /> {app.displayName}
              </b>
              <span>{wiringLine(app)}</span>
            </div>
          ))}
        </div>
        <p>{WIRING.footer}</p>
        <div className="sk-modal-actions">
          <button type="button" className="wb-pill prominent" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function RowMenu({ row, dark, onWiring, onReveal, onClose }: { row: SkillInventoryRow; dark: boolean; onWiring: () => void; onReveal: () => void; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [onClose]);
  void dark;
  return (
    <div className="sk-menu" ref={ref} role="menu" aria-label={`More actions for ${row.name}`}>
      <button type="button" onClick={onWiring}>
        Wiring Details…
      </button>
      <button type="button" onClick={onReveal}>
        <Folder size={12} /> Reveal in Finder
      </button>
      <button type="button" disabled title={READ_ONLY}>
        Update from repository
      </button>
      <button type="button" className="danger" disabled title={READ_ONLY}>
        Uninstall…
      </button>
      <div className="sk-menu-note">Update and uninstall run in the native app; this client keeps the library read-only.</div>
    </div>
  );
}

function SkillRow({ row, dark, onWiring, onReveal }: { row: SkillInventoryRow; dark: boolean; onWiring: () => void; onReveal: (row: SkillInventoryRow) => void }) {
  const [menu, setMenu] = useState(false);
  const health = healthBadge(row);
  return (
    <div className="sk-row">
      <div className="sk-details">
        <div className="sk-name-line">
          <span className="sk-name">{row.name}</span>
          <span className="sk-badge" title={row.source === "local" ? "Installed locally" : `From ${row.source}`}>
            {sourceBadge(row)}
          </span>
          {health ? (
            <span className="sk-badge warn" title="The scan could not read this directory as a skill">
              {health}
            </span>
          ) : null}
        </div>
        {row.description ? <div className="sk-desc">{row.description}</div> : null}
      </div>
      <div className="sk-circles" role="group" aria-label={`Where ${row.name} is available`}>
        {MANAGED_APPS.map((app: SkillAppTarget) => {
          const state = activationState(row, app);
          return (
            <span
              key={app.id}
              className={`sk-circle ${state}${isOn(state) ? " on" : ""}`}
              style={{ "--tint": accentFor(app.id, dark) } as React.CSSProperties}
              title={helpText(app, state)}
              aria-label={`${app.displayName}: ${state}`}
            >
              <ToolBrandIcon tool={app.id} size={13} opacity={isOn(state) ? 1 : 0.7} />
            </span>
          );
        })}
      </div>
      <div style={{ position: "relative" }}>
        <button type="button" className="sk-more" aria-label={`More actions for ${row.name}`} aria-expanded={menu} onClick={() => setMenu((v) => !v)}>
          <Dots />
        </button>
        {menu ? (
          <RowMenu
            row={row}
            dark={dark}
            onWiring={() => {
              setMenu(false);
              onWiring();
            }}
            onReveal={() => {
              setMenu(false);
              onReveal(row);
            }}
            onClose={() => setMenu(false)}
          />
        ) : null}
      </div>
    </div>
  );
}

/** The Workbench Skills page — the native `SkillsManagerPage` composition
 *  over the shared skill library this client reads. */
export function SkillsPage({ refreshToken = 0, fixture, dark = false }: { refreshToken?: number; fixture?: SkillsInventoryView; dark?: boolean }) {
  const [view, setView] = useState<SkillsInventoryView | null>(fixture ?? null);
  const [loading, setLoading] = useState(!fixture);
  const [search, setSearch] = useState("");
  const [wiring, setWiring] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const load = async () => {
    if (fixture) return;
    setLoading(true);
    try {
      setView(await api.skillsInventory());
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken]);

  const notify = (note: string) => {
    setToast(note);
    window.setTimeout(() => setToast((current) => (current === note ? null : current)), 3_000);
  };
  const reveal = async (row: SkillInventoryRow) => {
    if (fixture) return notify(`Preview: would reveal ${row.directory}.`);
    try {
      await api.revealPath(row.directory);
    } catch (cause) {
      notify(String(cause));
    }
  };

  const rows = view?.skills ?? [];
  const shown = filterSkills(rows, search);
  const counts = appCounts(rows);

  return (
    <div className="sk-page" style={{ position: "relative" }}>
      <div className="wb-toolbar sk-toolbar">
        <div className="sk-toolbar-row">
          <div className="wb-field sk-search">
            <Search size={12} />
            <input type="search" placeholder="Filter installed skills" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Filter installed skills" />
            {search ? (
              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Clear the filter" onClick={() => setSearch("")}>
                <XCircle size={12} />
              </button>
            ) : null}
          </div>
          <button type="button" className="wb-pill" disabled title={READ_ONLY}>
            <Refresh size={12} /> Check Updates
          </button>
          <button type="button" className="wb-pill" disabled title={READ_ONLY}>
            <ZipIcon /> Install from ZIP
          </button>
          <button type="button" className="wb-pill" disabled title={READ_ONLY}>
            <ImportIcon /> Import Existing
          </button>
          <button type="button" className="wb-pill" disabled title={READ_ONLY}>
            <ClockIcon /> Backups
          </button>
          <button type="button" className="wb-pill prominent" disabled title="Browse configured repositories and the skills.sh index — in the native app for now" style={{ opacity: 0.6 }}>
            <Search size={12} /> Discover
          </button>
          <button type="button" className="wb-iconbtn" title="Rescan the skill library" onClick={() => void load()} disabled={loading || !!fixture}>
            <Refresh size={13} />
          </button>
        </div>
        <div className="sk-toolbar-row">
          <div className="sk-apps">
            {MANAGED_APPS.map((app) => (
              <span key={app.id} className={`sk-app${counts[app.id] === 0 ? " zero" : ""}`} style={{ "--tint": accentFor(app.id, dark) } as React.CSSProperties} title={appCountHelp(app, rows)} aria-label={`${app.displayName}, sees ${counts[app.id]} skills`}>
                <ToolBrandIcon tool={app.id} size={13} />
                {counts[app.id]}
              </span>
            ))}
            <button type="button" className="sk-help" title="How skill syncing works — roots, links, and native switches" onClick={() => setWiring(true)}>
              ?
            </button>
          </div>
          <span className="sk-count">{countSummary(shown.length, rows.length)}</span>
        </div>
      </div>
      {view && rows.length === 0 ? (
        <section className="wb-card">
          <div className="sk-empty">
            <Puzzle size={26} />
            <div className="sk-empty-title">No skills recorded yet</div>
            <div className="sk-empty-detail">Vibe Bar keeps one copy of every skill in ~/.agents/skills and links it into each agent CLI. Import what is already on this Mac, or install something new from a repository.</div>
            <div className="sk-empty-actions">
              <button type="button" className="wb-pill" disabled title={READ_ONLY}>
                Import Existing
              </button>
              <button type="button" className="wb-pill prominent" disabled title={READ_ONLY} style={{ opacity: 0.6 }}>
                Discover
              </button>
            </div>
          </div>
        </section>
      ) : shown.length === 0 && view ? (
        <section className="wb-card">
          <div className="sk-empty">
            <div className="sk-empty-title">No skill matches “{search}”</div>
          </div>
        </section>
      ) : (
        <section className="wb-card sk-list" aria-label="Installed skills">
          {shown.map((row) => (
            <SkillRow key={row.directory} row={row} dark={dark} onWiring={() => setWiring(true)} onReveal={(r) => void reveal(r)} />
          ))}
        </section>
      )}
      {view && view.warnings.length > 0 ? <div className="sk-warnings">{view.warnings.length} scan note{view.warnings.length === 1 ? "" : "s"}: {view.warnings.slice(0, 3).join(" · ")}</div> : null}
      {!view && loading ? <p className="wb-empty">Reading the skill library…</p> : null}
      {wiring ? <WiringModal dark={dark} onClose={() => setWiring(false)} /> : null}
      {toast ? <div className="ss-toast" role="status">{toast}</div> : null}
    </div>
  );
}
