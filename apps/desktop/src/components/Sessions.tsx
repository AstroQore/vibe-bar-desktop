import { useEffect, useMemo, useRef, useState } from "react";

import type { SessionListing, SessionRow, TranscriptPage } from "../api";
import { api, formatRelative } from "../api";

const PAGE_SIZE = 40;

export function Sessions() {
  const [listing, setListing] = useState<SessionListing | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SessionRow | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    // Debounced so typing doesn't fire a query per keystroke against a
    // multi-gigabyte index.
    const timer = setTimeout(() => {
      const request = query.trim()
        ? api.sessionSearch(query.trim(), 100)
        : api.sessionList(100);
      request
        .then((result) => {
          if (!cancelled) setListing(result);
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, query ? 220 : 0);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  if (selected) {
    return <Transcript session={selected} onBack={() => setSelected(null)} />;
  }

  return (
    <>
      <div className="toolbar">
        <input
          type="search"
          placeholder="Search sessions…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          aria-label="Search sessions"
        />
      </div>

      {listing?.indexNote ? <p className="banner">{listing.indexNote}</p> : null}

      {listing ? (
        <p className="status-line" style={{ marginBottom: 8 }}>
          {listing.source === "indexed"
            ? `${listing.indexedTotal?.toLocaleString() ?? "?"} sessions in the shared index`
            : "Scanned from Codex and Claude Code logs on this machine"}
        </p>
      ) : null}

      {loading && !listing ? (
        <p className="empty">Loading sessions…</p>
      ) : listing && listing.rows.length === 0 ? (
        <p className="empty">
          {query ? "No sessions match that search." : "No sessions found yet."}
        </p>
      ) : (
        listing?.rows.map((row, index) => (
          <button
            className="session-row"
            key={`${row.provider}:${row.rowId ?? row.sessionId}:${index}`}
            disabled={!row.sessionRef}
            title={
              row.sessionRef
                ? undefined
                : "Transcript temporarily unavailable; reload after older references expire."
            }
            onClick={() => setSelected(row)}
          >
            <span className="session-title">
              {row.title ?? row.sessionId}
            </span>
            <span className="session-meta">
              <span>{row.harness}</span>
              {row.projectDir ? <span>{basename(row.projectDir)}</span> : null}
              <span>{formatRelative(row.lastActiveAt)}</span>
              {row.messageCount && row.messageCount > 0 ? (
                <span>{row.messageCount} msgs</span>
              ) : null}
            </span>
            {row.excerpt ? (
              <span className="session-excerpt">{stripMarks(row.excerpt)}</span>
            ) : null}
          </button>
        ))
      )}
    </>
  );
}

function Transcript({
  session,
  onBack,
}: {
  session: SessionRow;
  onBack: () => void;
}) {
  const [page, setPage] = useState<TranscriptPage | null>(null);
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [matchIndex, setMatchIndex] = useState(0);
  const messageRefs = useRef(new Map<number, HTMLDivElement>());

  useEffect(() => {
    let cancelled = false;
    setError(null);
    api
      .sessionTranscript(session.sessionRef, offset, PAGE_SIZE)
      .then((result) => {
        if (!cancelled) setPage(result);
      })
      .catch((cause) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [session.sessionRef, offset]);

  const matches = useMemo(() => {
    const needle = findQuery.trim().toLocaleLowerCase();
    if (!needle || !page) return [];
    return page.messages.flatMap((message, index) =>
      message.text.toLocaleLowerCase().includes(needle) ? [index] : [],
    );
  }, [findQuery, page]);

  // A changed page or session is a new bounded document. Start the find
  // cursor over rather than carrying an index into unrelated messages.
  useEffect(() => {
    setMatchIndex(0);
  }, [findQuery, offset, session.sessionRef]);

  useEffect(() => {
    if (matches.length === 0) {
      if (matchIndex !== 0) setMatchIndex(0);
      return;
    }
    if (matchIndex >= matches.length) {
      setMatchIndex(0);
    }
  }, [matchIndex, matches]);

  useEffect(() => {
    const messageIndex = matches[matchIndex];
    if (messageIndex === undefined) return;
    messageRefs.current.get(messageIndex)?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
  }, [matchIndex, matches]);

  const total = page?.totalMessages;
  const shown = page?.messages.length ?? 0;
  const hasMore = page
    ? page.truncated
      ? shown === PAGE_SIZE
      : total !== undefined && offset + shown < total
    : false;

  return (
    <>
      <div className="toolbar">
        <button onClick={onBack}>← Back</button>
        {session.resumeCommand ? (
          <button
            onClick={() => {
              navigator.clipboard
                .writeText(session.resumeCommand!)
                .then(() => setCopied(true))
                .catch(() => setCopied(false));
            }}
            title={session.resumeCommand}
          >
            {copied ? "Copied" : "Copy resume command"}
          </button>
        ) : null}
        <span className="status-line" style={{ marginLeft: "auto" }}>
          {total !== undefined && total > 0
            ? `Messages ${offset + 1}–${offset + shown} of ${total}`
            : page?.truncated
              ? shown > 0
                ? `Messages ${offset + 1}–${offset + shown} (scan limit reached)`
                : "Scan limit reached before this page."
              : ""}
        </span>
      </div>

      <div className="transcript-find" role="search">
        <input
          type="search"
          value={findQuery}
          onChange={(event) => setFindQuery(event.target.value)}
          placeholder="Find in this page…"
          aria-label="Find in this transcript page"
        />
        {findQuery.trim() ? (
          <span className="transcript-find-count" aria-live="polite">
            {matches.length === 0 ? "No matches" : `${matchIndex + 1}/${matches.length}`}
          </span>
        ) : null}
        <button
          type="button"
          disabled={matches.length === 0}
          onClick={() =>
            setMatchIndex((current) =>
              matches.length === 0 ? 0 : (current - 1 + matches.length) % matches.length,
            )
          }
        >
          Previous match
        </button>
        <button
          type="button"
          disabled={matches.length === 0}
          onClick={() =>
            setMatchIndex((current) =>
              matches.length === 0 ? 0 : (current + 1) % matches.length,
            )
          }
        >
          Next match
        </button>
        {findQuery ? (
          <button type="button" onClick={() => setFindQuery("")}>
            Clear
          </button>
        ) : null}
      </div>

      {error ? (
        <p className="empty">Could not read this transcript: {error}</p>
      ) : !page ? (
        <p className="empty">Loading transcript…</p>
      ) : (
        <>
          {page.messages.length === 0 ? (
            <p className="empty">
              No readable messages. {session.harness} may store this conversation in
              a format this build cannot render yet.
            </p>
          ) : (
            page.messages.map((message, index) => (
              <div
                className={`transcript-message ${message.role}${
                  matches[matchIndex] === index ? " transcript-match-current" : ""
                }`}
                key={`${offset}-${index}`}
                ref={(element) => {
                  if (element) messageRefs.current.set(index, element);
                  else messageRefs.current.delete(index);
                }}
              >
                <div className="transcript-role">{message.role}</div>
                <div className="transcript-text">{message.text}</div>
              </div>
            ))
          )}
          <div className="toolbar" style={{ marginTop: 12 }}>
            <button
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
            >
              Previous
            </button>
            <button
              disabled={!hasMore}
              onClick={() => setOffset(offset + PAGE_SIZE)}
            >
              Next
            </button>
          </div>
        </>
      )}
    </>
  );
}

/** FTS excerpts arrive with `<b>` markers around the match. */
function stripMarks(text: string): string {
  return text.replace(/<\/?b>/g, "");
}

function basename(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
