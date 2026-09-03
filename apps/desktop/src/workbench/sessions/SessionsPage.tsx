import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SessionListing, SessionRow, TranscriptCursor, TranscriptPage } from "../../api";
import { api } from "../../api";
import { SessionFilters, type SessionFilterState } from "./SessionFilters";
import { SessionList } from "./SessionList";
import { Transcript } from "./Transcript";
import { TRANSCRIPT_PAGE, applyFolderFilters, buildListingQuery, groupRows, indexStatusText, sortRows } from "./model";
import "./sessions.css";

const PAGE = 250;

const DEFAULTS: SessionFilterState = {
  search: "",
  scopes: ["title", "user", "assistant"],
  folders: { include: [], exclude: [] },
  companies: null,
  harnesses: null,
  range: "all",
  sort: "recentFirst",
  groupByProject: false,
  terminal: (() => {
    try {
      const stored = localStorage.getItem("vibebar.sessions.terminal");
      return stored === "iterm2" || stored === "copyOnly" ? stored : "terminal";
    } catch {
      return "terminal";
    }
  })(),
};

/** The Workbench Sessions page — the native `SessionManagerPage` composition.
 *  `fixture` renders synthetic data for the preview page without Tauri. */
export function SessionsPage({
  refreshToken = 0,
  fixture,
  now: fixedNow,
  dark = false,
}: {
  refreshToken?: number;
  fixture?: { listing: SessionListing; transcript: TranscriptPage };
  now?: number;
  dark?: boolean;
}) {
  const [filters, setFilters] = useState<SessionFilterState>(DEFAULTS);
  const [listing, setListing] = useState<SessionListing | null>(fixture?.listing ?? null);
  const [rows, setRows] = useState<SessionRow[]>(fixture?.listing.rows ?? []);
  const [loading, setLoading] = useState(!fixture);
  const [selected, setSelected] = useState<SessionRow | null>(fixture?.listing.rows[0] ?? null);
  const [page, setPage] = useState<TranscriptPage | null>(fixture?.transcript ?? null);
  const [pageLoading, setPageLoading] = useState(false);
  const [pageError, setPageError] = useState<string | null>(null);
  const [history, setHistory] = useState<TranscriptCursor[]>([]);
  const [deleteMode, setDeleteMode] = useState(false);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  // Deletion is two clicks: the first arms the button, the second — within
  // a few seconds — removes the checked sessions' log files for good.
  const [deleteArmed, setDeleteArmed] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [listWidth, setListWidth] = useState(380);
  const generation = useRef(0);
  const transcriptGeneration = useRef(0);
  const now = fixedNow ?? Math.floor(Date.now() / 1000);

  const load = useCallback(
    async (offset: number) => {
      if (fixture) return;
      const id = ++generation.current;
      setLoading(true);
      try {
        const next = await api.sessionListing(buildListingQuery({ ...filters, offset, limit: PAGE, now: Math.floor(Date.now() / 1000) }));
        if (id !== generation.current) return;
        setListing(next);
        setRows((current) => (offset === 0 ? next.rows : [...current, ...next.rows]));
      } finally {
        if (id === generation.current) setLoading(false);
      }
    },
    [filters, fixture],
  );
  useEffect(() => {
    void load(0);
  }, [load, refreshToken]);

  useEffect(() => {
    try {
      localStorage.setItem("vibebar.sessions.terminal", filters.terminal);
    } catch {
      /* per-viewer convenience only */
    }
  }, [filters.terminal]);

  const openTranscript = useCallback(
    async (row: SessionRow, offset: number, cursor?: TranscriptCursor) => {
      if (fixture) return;
      // A slow read for the previous selection must not land on the new one.
      const id = ++transcriptGeneration.current;
      setPageLoading(true);
      setPageError(null);
      try {
        const next = await api.sessionTranscript(row.sessionRef, offset, TRANSCRIPT_PAGE, cursor);
        if (id !== transcriptGeneration.current) return;
        setPage(next);
      } catch (cause) {
        if (id !== transcriptGeneration.current) return;
        setPageError(`Could not read this transcript: ${String(cause)}`);
      } finally {
        if (id === transcriptGeneration.current) setPageLoading(false);
      }
    },
    [fixture],
  );

  const select = (row: SessionRow) => {
    setSelected(row);
    setHistory([]);
    setPage(null);
    void openTranscript(row, 0);
  };

  const notify = (note: string) => {
    setToast(note);
    window.setTimeout(() => setToast((current) => (current === note ? null : current)), 2_200);
  };

  useEffect(() => {
    if (!deleteArmed) return;
    const timer = window.setTimeout(() => setDeleteArmed(false), 6000);
    return () => window.clearTimeout(timer);
  }, [deleteArmed]);

  const deleteChecked = async () => {
    if (checked.size === 0 || deleting) return;
    if (!deleteArmed) {
      setDeleteArmed(true);
      return;
    }
    setDeleteArmed(false);
    setDeleting(true);
    try {
      const refs = [...checked];
      const reports = await api.sessionDelete(refs);
      const gone = new Set(reports.filter((report) => report.deleted).map((report) => report.sessionRef));
      const failed = reports.filter((report) => !report.deleted);
      setRows((current) => current.filter((row) => !gone.has(row.sessionRef)));
      if (selected && gone.has(selected.sessionRef)) setSelected(null);
      setChecked(new Set(failed.map((report) => report.sessionRef)));
      if (failed.length === 0) setDeleteMode(false);
      const first = failed[0]?.reason;
      notify(
        failed.length === 0
          ? `Deleted ${gone.size} ${gone.size === 1 ? "session" : "sessions"}.`
          : `Deleted ${gone.size}; ${failed.length} not deleted${first ? ` — ${first}` : ""}.`,
      );
      void load(0);
    } catch (error) {
      notify(`Could not delete: ${String(error)}`);
    } finally {
      setDeleting(false);
    }
  };
  const copy = async (text: string, note: string) => {
    try {
      await navigator.clipboard.writeText(text);
      notify(note);
    } catch {
      notify("Could not reach the clipboard.");
    }
  };
  const open = async (command: string) => {
    if (fixture) return notify("Preview: would run the resume command.");
    try {
      await api.openInTerminal(command, filters.terminal === "iterm2" ? "iterm2" : "terminal");
      notify(`Sent to ${filters.terminal === "iterm2" ? "iTerm2" : "Terminal"}.`);
    } catch (cause) {
      notify(String(cause));
    }
  };

  const visible = useMemo(() => sortRows(applyFolderFilters(rows, filters.folders), filters.sort), [rows, filters.folders, filters.sort]);
  const groups = filters.groupByProject ? groupRows(visible) : null;
  const indexed = listing?.source === "indexed";
  const noHarness = filters.harnesses !== null && filters.harnesses.length === 0;
  const status = listing ? indexStatusText(visible.length, listing.indexedTotal ?? undefined, indexed) : "…";
  const empty =
    listing && visible.length === 0
      ? {
          title: !indexed && listing.indexNote ? "Session index unavailable" : noHarness ? "No harness selected — pick one above" : "No sessions match",
          detail: !indexed && listing.indexNote ? listing.indexNote : filters.search ? "Nothing in the indexed sessions matches that search." : "No session logs were found on this Mac for any of the harnesses Vibe Bar scans.",
        }
      : null;

  const startDrag = (event: React.PointerEvent) => {
    const startX = event.clientX;
    const startWidth = listWidth;
    const move = (e: PointerEvent) => setListWidth(Math.max(300, Math.min(620, startWidth + e.clientX - startX)));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className="ss-page" style={{ position: "relative" }}>
      <SessionFilters
        state={filters}
        counts={listing?.harnessCounts ?? []}
        status={status}
        scanning={loading}
        onChange={(next) => {
          setFilters(next);
          setChecked(new Set());
        }}
        onRescan={() => void load(0)}
        deleteMode={deleteMode}
        onToggleDeleteMode={() => {
          setDeleteMode((v) => !v);
          setChecked(new Set());
          setDeleteArmed(false);
        }}
        checkedCount={checked.size}
        deleteArmed={deleteArmed}
        deleting={deleting}
        onDelete={() => void deleteChecked()}
        deleteDisabledReason={null}
      />
      <div className="ss-split">
        <div className="ss-list-pane" style={{ width: listWidth }}>
          <SessionList
            groups={groups}
            rows={visible}
            selectedRef={selected?.sessionRef ?? null}
            deleteMode={deleteMode}
            checked={checked}
            now={now}
            dark={dark}
            onSelect={select}
            onToggleCheck={(row) =>
              setChecked((current) => {
                const next = new Set(current);
                if (next.has(row.sessionRef)) next.delete(row.sessionRef);
                else next.add(row.sessionRef);
                return next;
              })
            }
            empty={empty}
            canLoadMore={!fixture && !filters.search && listing != null && listing.rows.length === PAGE}
            onLoadMore={() => void load(rows.length)}
            loading={loading}
          />
        </div>
        <div className="ss-splitter" role="separator" aria-orientation="vertical" onPointerDown={startDrag} />
        <div className="ss-transcript-pane">
          <Transcript
            row={selected}
            page={page}
            loading={pageLoading}
            error={pageError}
            terminal={filters.terminal}
            dark={dark}
            onCopy={(text, note) => void copy(text, note)}
            onOpen={(command) => void open(command)}
            onPage={(direction) => {
              if (!selected || !page) return;
              if (direction === 1 && page.nextCursor) {
                setHistory((h) => [...h, page.nextCursor!]);
                void openTranscript(selected, page.offset + page.messages.length, page.nextCursor);
              } else if (direction === -1) {
                const previous = history.slice(0, -2);
                const cursor = previous[previous.length - 1];
                setHistory(previous);
                void openTranscript(selected, Math.max(0, page.offset - TRANSCRIPT_PAGE), cursor);
              }
            }}
          />
        </div>
      </div>
      {toast ? <div className="ss-toast" role="status">{toast}</div> : null}
    </div>
  );
}
