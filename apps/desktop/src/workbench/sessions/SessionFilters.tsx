import { useState } from "react";
import type { HarnessCount } from "../../api";
import { CheckCircle, Refresh, Search, Sliders, Trash, XCircle } from "../icons";
import { Menu, MenuItem } from "../usage/Menu";
import {
  COMPANY_ORDER,
  DATE_RANGES,
  SEARCH_SCOPES,
  SORT_ORDERS,
  TERMINALS,
  type DateRange,
  type FolderFilters,
  type PreferredTerminal,
  type SearchScope,
  type SortOrder,
  companyOf,
  harnessSummary,
  parseFolderList,
} from "./model";

export interface SessionFilterState {
  search: string;
  scopes: SearchScope[];
  folders: FolderFilters;
  companies: string[] | null;
  harnesses: string[] | null;
  range: DateRange;
  sort: SortOrder;
  groupByProject: boolean;
  terminal: PreferredTerminal;
}

/** The native `SessionFiltersBar`: search, scope, folders, index status,
 *  rescan, the All chip, and the company/harness/when/sort/options menus. */
export function SessionFilters({
  state,
  counts,
  status,
  scanning,
  onChange,
  onRescan,
  deleteMode,
  onToggleDeleteMode,
  checkedCount,
  onDelete,
  deleteDisabledReason,
}: {
  state: SessionFilterState;
  counts: HarnessCount[];
  status: string;
  scanning: boolean;
  onChange: (next: SessionFilterState) => void;
  onRescan: () => void;
  deleteMode: boolean;
  onToggleDeleteMode: () => void;
  checkedCount: number;
  onDelete: () => void;
  deleteDisabledReason: string | null;
}) {
  const [folderDraft, setFolderDraft] = useState({ include: state.folders.include.join(", "), exclude: state.folders.exclude.join(", ") });
  const harnesses = counts.map((c) => c.harness);
  const companies = COMPANY_ORDER.filter((company) => counts.some((c) => companyOf(c.harness) === company));
  const allOn = state.harnesses === null && state.companies === null;
  const foldersActive = state.folders.include.length > 0 || state.folders.exclude.length > 0;
  const companySummary = state.companies === null ? "All" : state.companies.length === companies.length ? "All" : `${state.companies.length}/${companies.length}`;
  const terminalTitle = TERMINALS.find((t) => t.id === state.terminal)?.title ?? "Terminal";

  return (
    <div className="wb-toolbar ss-toolbar">
      <div className="ss-toolbar-row">
        <div className="wb-field ss-search">
          <Search size={12} />
          <input
            type="search"
            placeholder="Search sessions"
            value={state.search}
            onChange={(event) => onChange({ ...state, search: event.target.value })}
            aria-label="Search sessions"
          />
          {state.search ? (
            <button type="button" className="wb-iconbtn" title="Clear the search" onClick={() => onChange({ ...state, search: "" })}>
              <XCircle size={12} />
            </button>
          ) : null}
        </div>
        <Menu title="Scope" detail={`${state.scopes.length}`} ariaLabel="Choose what the search reads" width={240}>
          {() => (
            <>
              {SEARCH_SCOPES.map((scope) => (
                <MenuItem
                  key={scope.id}
                  checked={state.scopes.includes(scope.id)}
                  onSelect={() =>
                    onChange({
                      ...state,
                      scopes: state.scopes.includes(scope.id) ? state.scopes.filter((s) => s !== scope.id) : [...state.scopes, scope.id],
                    })
                  }
                >
                  {scope.title}
                </MenuItem>
              ))}
              <div className="ss-menu-note">Message text is indexed locally. This client searches every scope the index holds; per-scope narrowing lands with the role-aware index.</div>
            </>
          )}
        </Menu>
        <Menu title="Folders" detail={foldersActive ? "Filtered" : "All"} ariaLabel="Filter by project directory" width={376}>
          {(close) => (
            <div className="ss-folders">
              <h4>Directory filters</h4>
              <label>
                Include paths containing
                <input value={folderDraft.include} placeholder="/project/a, /project/b" onChange={(e) => setFolderDraft({ ...folderDraft, include: e.target.value })} />
              </label>
              <label>
                Exclude paths containing
                <input value={folderDraft.exclude} placeholder="/archive, /vendor" onChange={(e) => setFolderDraft({ ...folderDraft, exclude: e.target.value })} />
              </label>
              <div className="ss-folders-actions">
                <button
                  type="button"
                  onClick={() => {
                    setFolderDraft({ include: "", exclude: "" });
                    onChange({ ...state, folders: { include: [], exclude: [] } });
                  }}
                >
                  Clear
                </button>
                <button
                  type="button"
                  className="primary"
                  onClick={() => {
                    onChange({ ...state, folders: { include: parseFolderList(folderDraft.include), exclude: parseFolderList(folderDraft.exclude) } });
                    close();
                  }}
                >
                  Done
                </button>
              </div>
            </div>
          )}
        </Menu>
        <button
          type="button"
          className={`wb-pill${allOn ? " on" : ""}`}
          title={allOn ? "Click to select no harness" : "Click to show sessions from every harness"}
          onClick={() => onChange({ ...state, companies: null, harnesses: allOn ? [] : null })}
        >
          All
        </button>
        <Menu title="Company" detail={companySummary} ariaLabel="Filter by billing company" width={200}>
          {() => (
            <>
              {companies.map((company) => {
                const on = state.companies === null || state.companies.includes(company);
                return (
                  <MenuItem
                    key={company}
                    checked={on}
                    onSelect={() => {
                      const current = state.companies ?? [...companies];
                      const next = on ? current.filter((c) => c !== company) : [...current, company];
                      onChange({ ...state, companies: companies.every((c) => next.includes(c)) ? null : next, harnesses: null });
                    }}
                  >
                    {company}
                  </MenuItem>
                );
              })}
            </>
          )}
        </Menu>
        <Menu title="Harness" detail={harnessSummary(state.harnesses, harnesses.length)} ariaLabel="Filter by harness" width={220}>
          {() => (
            <>
              {counts.map((count) => {
                const on = state.harnesses === null || state.harnesses.includes(count.harness);
                return (
                  <MenuItem
                    key={count.harness}
                    checked={on}
                    onSelect={() => {
                      const current = state.harnesses ?? harnesses;
                      const next = on ? current.filter((h) => h !== count.harness) : [...current, count.harness];
                      onChange({ ...state, harnesses: harnesses.every((h) => next.includes(h)) ? null : next, companies: null });
                    }}
                  >
                    {count.harness}&nbsp;&nbsp;<span style={{ opacity: 0.6 }}>{count.count}</span>
                  </MenuItem>
                );
              })}
            </>
          )}
        </Menu>
        <Menu title="When" detail={DATE_RANGES.find((r) => r.id === state.range)?.title ?? "Any time"} ariaLabel="Choose how far back to list sessions" width={160}>
          {(close) => (
            <>
              {DATE_RANGES.map((range) => (
                <MenuItem
                  key={range.id}
                  checked={state.range === range.id}
                  onSelect={() => {
                    onChange({ ...state, range: range.id });
                    close();
                  }}
                >
                  {range.title}
                </MenuItem>
              ))}
            </>
          )}
        </Menu>
        <Menu title="Sort" detail={`${SORT_ORDERS.find((s) => s.id === state.sort)?.title.replace(/ first$/, "")}${state.groupByProject ? " · grouped" : ""}`} ariaLabel="Choose how the list is ordered" width={190}>
          {() => (
            <>
              {SORT_ORDERS.map((order) => (
                <MenuItem key={order.id} checked={state.sort === order.id} onSelect={() => onChange({ ...state, sort: order.id })}>
                  {order.title}
                </MenuItem>
              ))}
              <div style={{ height: 0.5, background: "var(--wb-hairline)", margin: "4px 0" }} />
              <MenuItem checked={state.groupByProject} onSelect={() => onChange({ ...state, groupByProject: !state.groupByProject })}>
                Group by project
              </MenuItem>
              {state.sort === "oldestFirst" ? <div className="ss-menu-note">Oldest first orders the sessions loaded so far; the index hands out pages newest-first, so load more to reach older ones.</div> : null}
            </>
          )}
        </Menu>
        <Menu icon={<Sliders size={13} />} title="" detail="" ariaLabel={`Terminal and index options (opens in ${terminalTitle})`} width={230}>
          {() => (
            <>
              <div className="wb-label-caps" style={{ padding: "4px 8px 2px" }}>Open in</div>
              {TERMINALS.map((terminal) => (
                <MenuItem key={terminal.id} checked={state.terminal === terminal.id} onSelect={() => onChange({ ...state, terminal: terminal.id })}>
                  {terminal.title}
                </MenuItem>
              ))}
              <div style={{ height: 0.5, background: "var(--wb-hairline)", margin: "4px 0" }} />
              <MenuItem checked disabled onSelect={() => {}}>
                Index message text
              </MenuItem>
              <MenuItem disabled onSelect={() => {}}>
                Rebuild index…
              </MenuItem>
              <div className="ss-menu-note">The session index is shared with the native app, which builds it. This client reads it and rescans the logs itself when it is missing.</div>
              <div style={{ height: 0.5, background: "var(--wb-hairline)", margin: "4px 0" }} />
              <MenuItem checked={deleteMode} disabled={deleteDisabledReason !== null && !deleteMode} onSelect={onToggleDeleteMode}>
                Select sessions to delete
              </MenuItem>
              {deleteDisabledReason ? <div className="ss-menu-note">{deleteDisabledReason}</div> : null}
            </>
          )}
        </Menu>
        <span className="wb-spacer" />
        {deleteMode ? (
          <>
            <button type="button" className="wb-pill danger" disabled={checkedCount === 0 || deleteDisabledReason !== null} title={deleteDisabledReason ?? "Delete the checked sessions"} onClick={onDelete}>
              <Trash size={12} /> {checkedCount === 0 ? "Delete" : `Delete ${checkedCount}`}
            </button>
            <button type="button" className="wb-pill" title="Leave selection mode" onClick={onToggleDeleteMode}>
              <CheckCircle size={12} /> Done
            </button>
          </>
        ) : (
          <span className="ss-index">
            {scanning ? <progress /> : null}
            {status}
            <button type="button" className="wb-iconbtn" style={{ width: 22, height: 22 }} title="Rescan the session logs on disk" onClick={onRescan}>
              <Refresh size={12} />
            </button>
          </span>
        )}
      </div>
    </div>
  );
}
