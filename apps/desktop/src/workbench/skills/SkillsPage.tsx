import { useEffect, useState } from "react";
import type { SkillBackup, SkillInventoryRow, SkillsInventoryView } from "../../api";
import { api } from "../../api";
import { ToolBrandIcon } from "../../popover/brand";
import { accentFor } from "../../components/ResetHistory";
import { Folder, Puzzle, Refresh, Search, XCircle } from "../icons";
import { MANAGED_APPS, WIRING, type SkillAppTarget, activationState, appCountHelp, appCounts, countSummary, filterSkills, healthBadge, isOn, sourceBadge, wiringLine } from "./model";
import "./skills.css";

const NATIVE_ONLY = "Repository installs, update checks and Discover run in the native app in this release; this client installs from folders, adopts, projects, backs up and uninstalls.";

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

function RowMenu({
  row,
  dark,
  busy,
  onWiring,
  onReveal,
  onAdopt,
  onUninstall,
  onClose,
}: {
  row: SkillInventoryRow;
  dark: boolean;
  busy: boolean;
  onWiring: () => void;
  onReveal: () => void;
  onAdopt: () => void;
  onUninstall: () => void;
  onClose: () => void;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      if (!(event.target as HTMLElement).closest(".sk-menu, .sk-more")) onClose();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [onClose]);
  useEffect(() => {
    if (!armed) return;
    const timer = window.setTimeout(() => setArmed(false), 6000);
    return () => window.clearTimeout(timer);
  }, [armed]);
  return (
    <div className="sk-menu" role="menu">
      <div className="sk-menu-title" style={{ color: accentFor("codex", dark), display: "none" }} />
      <button type="button" onClick={onWiring}>
        How syncing works
      </button>
      <button type="button" onClick={onReveal}>
        <Folder size={12} /> Reveal in Finder
      </button>
      {row.registered ? null : (
        <button type="button" onClick={onAdopt} disabled={busy}>
          Record this folder
        </button>
      )}
      <button type="button" disabled title={NATIVE_ONLY}>
        Update from repository
      </button>
      <button
        type="button"
        className={`danger${armed ? " armed" : ""}`}
        disabled={busy || !row.registered}
        title={row.registered ? (armed ? "Click again to remove this skill's folder and links; a snapshot is taken first" : "Uninstall this skill") : "Record the folder first"}
        onClick={() => {
          if (!armed) {
            setArmed(true);
            return;
          }
          setArmed(false);
          onUninstall();
        }}
      >
        {armed ? "Confirm: uninstall" : "Uninstall…"}
      </button>
      <div className="sk-menu-note">Uninstall snapshots the folder under ~/.vibebar/skill_backups first; Backups puts it back.</div>
    </div>
  );
}

function SkillRow({
  row,
  dark,
  busy,
  onWiring,
  onReveal,
  onToggle,
  onAdopt,
  onUninstall,
}: {
  row: SkillInventoryRow;
  dark: boolean;
  busy: boolean;
  onWiring: () => void;
  onReveal: (row: SkillInventoryRow) => void;
  onToggle: (row: SkillInventoryRow, app: SkillAppTarget, on: boolean) => void;
  onAdopt: (row: SkillInventoryRow) => void;
  onUninstall: (row: SkillInventoryRow) => void;
}) {
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
          const slot = row.apps?.[app.id];
          const foreign = slot?.state === "foreign";
          const projected = slot?.state === "projected" || slot?.state === "copy";
          // A harness that reads ~/.agents/skills itself sees every skill
          // whatever this slot holds, and turning native activation on or off
          // is the native app's job. Showing a switch here would offer an
          // "off" that cannot take the skill away.
          const shared = app.discoversSharedRoot;
          const title = shared
            ? `${app.displayName}: reads the shared skills root directly, so every skill is already available; its own activation switch lives in the native app`
            : foreign
              ? `${app.displayName}: something else sits at this slot; Vibe Bar leaves it alone`
              : !row.registered
                ? `${app.displayName}: record this folder to project it`
                : projected
                  ? `${app.displayName}: projected — click to take it out`
                  : `${app.displayName}: click to project`;
          return (
            <button
              type="button"
              key={app.id}
              className={`sk-circle ${state}${isOn(state) ? " on" : ""}${foreign ? " foreign" : ""}`}
              style={{ "--tint": accentFor(app.id, dark) } as React.CSSProperties}
              title={title}
              aria-label={`${app.displayName}: ${state}`}
              aria-pressed={shared ? undefined : projected}
              disabled={shared || busy || foreign || !row.registered}
              onClick={() => onToggle(row, app, !projected)}
            >
              <ToolBrandIcon tool={app.id} size={13} opacity={isOn(state) ? 1 : 0.7} />
            </button>
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
            busy={busy}
            onWiring={() => {
              setMenu(false);
              onWiring();
            }}
            onReveal={() => {
              setMenu(false);
              onReveal(row);
            }}
            onAdopt={() => {
              setMenu(false);
              onAdopt(row);
            }}
            onUninstall={() => {
              setMenu(false);
              onUninstall(row);
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
  const [busy, setBusy] = useState(false);
  const [backups, setBackups] = useState<SkillBackup[] | null>(null);
  const run = async (label: string, action: () => Promise<unknown>) => {
    if (fixture) return notify(`Preview: would ${label}.`);
    setBusy(true);
    try {
      await action();
      await load();
    } catch (cause) {
      notify(String(cause));
    } finally {
      setBusy(false);
    }
  };
  const toggle = (row: SkillInventoryRow, app: SkillAppTarget, on: boolean) =>
    run(`${on ? "project" : "unproject"} ${row.name} for ${app.displayName}`, async () => {
      const changed = await api.skillsSetProjection(row.id, app.id, on);
      notify(changed ? `${row.name}: ${on ? "projected into" : "taken out of"} ${app.displayName}.` : `${row.name}: ${app.displayName}'s entry is not Vibe Bar's, so it was left alone.`);
    });
  const adopt = (row: SkillInventoryRow) =>
    run(`record ${row.directory}`, async () => {
      await api.skillsAdopt(row.directory, []);
      notify(`${row.name} is now recorded; project it where you want it.`);
    });
  const uninstall = (row: SkillInventoryRow) =>
    run(`uninstall ${row.name}`, async () => {
      const result = await api.skillsUninstall(row.id);
      const kept = Object.entries(result.removedByApp).filter(([, removed]) => !removed).map(([app]) => app);
      notify(kept.length === 0 ? `${row.name} uninstalled; a snapshot is under Backups.` : `${row.name} uninstalled; left alone in ${kept.join(", ")} (not Vibe Bar's). Snapshot under Backups.`);
    });
  const installFromFolder = async () => {
    if (fixture) return notify("Preview: would ask for a folder.");
    const folder = await api.pickFolder("Choose a skill folder (it must contain SKILL.md)");
    if (!folder) return;
    const name = folder.replace(/[\/\\]+$/, "").split(/[\/\\]/).pop() ?? "";
    await run(`install ${name}`, async () => {
      await api.skillsInstallLocal(folder, name, MANAGED_APPS.map((app) => app.id));
      notify(`${name} installed and projected into every managed app.`);
    });
  };
  const importExisting = () => {
    const unrecorded = (view?.skills ?? []).filter((row) => !row.registered && row.health === "healthy");
    if (unrecorded.length === 0) return notify("Every folder in ~/.agents/skills is already recorded.");
    void run(`record ${unrecorded.length} folders`, async () => {
      for (const row of unrecorded) await api.skillsAdopt(row.directory, []);
      notify(`Recorded ${unrecorded.length} ${unrecorded.length === 1 ? "folder" : "folders"} already in ~/.agents/skills.`);
    });
  };
  const showBackups = async () => {
    if (fixture) return setBackups([]);
    try {
      setBackups(await api.skillsBackups());
    } catch (cause) {
      notify(String(cause));
    }
  };
  const restore = (backup: SkillBackup) =>
    run(`restore ${backup.directoryName}`, async () => {
      await api.skillsRestoreBackup(backup.path);
      setBackups(null);
      notify(`${backup.skillName} restored from its snapshot.`);
    });
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
          <button type="button" className="wb-pill" disabled title={NATIVE_ONLY}>
            <Refresh size={12} /> Check Updates
          </button>
          <button type="button" className="wb-pill" disabled={busy || !!fixture} title="Copy a folder with a SKILL.md into ~/.agents/skills and project it into every managed app" onClick={() => void installFromFolder()}>
            <ZipIcon /> Install from Folder
          </button>
          <button type="button" className="wb-pill" disabled={busy || !!fixture} title="Record the folders already in ~/.agents/skills that the registry does not know" onClick={importExisting}>
            <ImportIcon /> Import Existing
          </button>
          <button type="button" className="wb-pill" disabled={busy} title="Snapshots taken before uninstalls" onClick={() => void showBackups()}>
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
              <button type="button" className="wb-pill" disabled={busy || !!fixture} onClick={importExisting}>
                Import Existing
              </button>
              <button type="button" className="wb-pill prominent" disabled title={NATIVE_ONLY} style={{ opacity: 0.6 }}>
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
            <SkillRow key={row.directory} row={row} dark={dark} busy={busy} onWiring={() => setWiring(true)} onReveal={(r) => void reveal(r)} onToggle={toggle} onAdopt={adopt} onUninstall={uninstall} />
          ))}
        </section>
      )}
      {view && view.warnings.length > 0 ? <div className="sk-warnings">{view.warnings.length} scan note{view.warnings.length === 1 ? "" : "s"}: {view.warnings.slice(0, 3).join(" · ")}</div> : null}
      {!view && loading ? <p className="wb-empty">Reading the skill library…</p> : null}
      {wiring ? <WiringModal dark={dark} onClose={() => setWiring(false)} /> : null}
      {backups ? (
        <div className="sk-modal-backdrop" onClick={() => setBackups(null)} role="presentation">
          <div className="sk-modal" role="dialog" aria-label="Skill backups" onClick={(e) => e.stopPropagation()}>
            <h3>Backups</h3>
            <p>Snapshots taken before each uninstall, newest first; the twenty most recent are kept. Restore puts a folder back into ~/.agents/skills when nothing is there under that name.</p>
            {backups.length === 0 ? (
              <p className="sk-empty-detail">No snapshots yet.</p>
            ) : (
              <div className="sk-backups">
                {backups.map((backup) => (
                  <div className="sk-backup" key={backup.path}>
                    <div className="sk-backup-titles">
                      <span className="sk-name">{backup.skillName}</span>
                      <span className="sk-desc">{backup.directoryName} · {new Date((backup.createdAt + 978307200) * 1000).toLocaleString("en-US", { dateStyle: "medium", timeStyle: "short" })}</span>
                    </div>
                    <button type="button" className="wb-pill" disabled={busy} onClick={() => void restore(backup)}>
                      Restore
                    </button>
                  </div>
                ))}
              </div>
            )}
            <div className="sk-modal-actions">
              <button type="button" className="wb-pill prominent" onClick={() => setBackups(null)}>
                Done
              </button>
            </div>
          </div>
        </div>
      ) : null}
      {toast ? <div className="ss-toast" role="status">{toast}</div> : null}
    </div>
  );
}
