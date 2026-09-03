import { useEffect, useMemo, useRef, useState } from "react";
import type { SessionRow, TranscriptMessage, TranscriptPage } from "../../api";
import { ToolBrandBadge } from "../../popover/brand";
import { ChevronDown, ChevronLeft, ChevronRight, Copy, Folder, ListBullet, Search, Terminal } from "../icons";
import {
  type MessagePart,
  type PreferredTerminal,
  brandTool,
  collapsed,
  collapses,
  findMatches,
  foldedLines,
  messageParts,
  messageTime,
  outline,
  pageLabel,
  pageRange,
  projectTitle,
  roleLabel,
  rowTint,
  rowTitle,
  splitHighlights,
} from "./model";

function Highlighted({ text, query }: { text: string; query: string }) {
  const parts = splitHighlights(text, query);
  return <>{parts.map((part, i) => (part.hit ? <mark key={i}>{part.text}</mark> : <span key={i}>{part.text}</span>))}</>;
}

/** A tool call: its name, what it was for, and its arguments as fields —
 *  a code block for the long ones — instead of the JSON the log holds. */
function ToolCall({ part, query }: { part: Extract<MessagePart, { kind: "tool" }>; query: string }) {
  return (
    <div className="ss-tool">
      <div className="ss-tool-head">
        <span className="wb-toolname">{part.name}</span>
        {part.purpose ? <span className="ss-tool-purpose"><Highlighted text={part.purpose} query={query} /></span> : null}
      </div>
      {part.fields.length > 0 ? (
        <div className="ss-tool-fields">
          {part.fields.map((field) => (
            <div key={field.key} className={`ss-tool-field${field.block ? " block" : ""}`}>
              <span className="ss-tool-key">{field.key}</span>
              {field.block ? <code className="wb-code"><Highlighted text={field.value} query={query} /></code> : <span className="ss-tool-value"><Highlighted text={field.value} query={query} /></span>}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** A tool's output: a code block that shows its first lines and folds the rest. */
function ToolResult({ text, query }: { text: string; query: string }) {
  const [open, setOpen] = useState(false);
  const fold = foldedLines(text);
  return (
    <>
      <code className="wb-code quiet"><Highlighted text={open ? text : fold.shown} query={query} /></code>
      {fold.hidden > 0 ? (
        <button type="button" className="ss-card-more" onClick={() => setOpen((value) => !value)}>
          {open ? "Show less" : `Show ${fold.hidden.toLocaleString("en-US")} more lines`}
        </button>
      ) : null}
    </>
  );
}

function MessageCard({ message, index, query, hit, onCopy }: { message: TranscriptMessage; index: number; query: string; hit: boolean; onCopy: (text: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  const folded = collapses(message.text) && !expanded;
  const text = folded ? collapsed(message.text) : message.text;
  const parts = useMemo(() => messageParts(text), [text]);
  const isResult = message.role === "tool" && parts.every((part) => part.kind === "text");
  return (
    <article className={`ss-card ${message.role}${hit ? " hit" : ""}`} id={`transcript-message-${index}`}>
      <span className="ss-card-bar" aria-hidden="true" />
      <div className="ss-card-body">
        <div className="ss-card-head">
          <span className="ss-card-role">{roleLabel(message.role)}</span>
          {message.timestamp ? <span className="ss-card-time">{messageTime(message.timestamp)}</span> : null}
        </div>
        {isResult ? (
          <ToolResult text={text} query={query} />
        ) : (
          parts.map((part, i) =>
            part.kind === "tool" ? <ToolCall key={i} part={part} query={query} /> : <div key={i} className="ss-card-text"><Highlighted text={part.text} query={query} /></div>,
          )
        )}
        {collapses(message.text) ? (
          <button type="button" className="ss-card-more" onClick={() => setExpanded((value) => !value)}>
            {expanded ? "Show less" : `Show more (${message.text.length.toLocaleString("en-US")} chars)`}
          </button>
        ) : null}
      </div>
      <button type="button" className="ss-card-copy" title="Copy this message" aria-label="Copy this message" onClick={() => onCopy(message.text)}>
        <Copy size={12} />
      </button>
    </article>
  );
}

/** The native `TranscriptView`: metadata header, find bar with an outline
 *  toggle, the 80-message pager, and one card per message. */
export function Transcript({
  row,
  page,
  loading,
  error,
  terminal,
  onCopy,
  onOpen,
  onPage,
}: {
  row: SessionRow | null;
  page: TranscriptPage | null;
  loading: boolean;
  error: string | null;
  terminal: PreferredTerminal;
  onCopy: (text: string, note: string) => void;
  onOpen: (command: string) => void;
  onPage: (direction: -1 | 1) => void;
}) {
  const [query, setQuery] = useState("");
  const [hitIndex, setHitIndex] = useState(0);
  const [showOutline, setShowOutline] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const messages = page?.messages ?? [];
  const hits = useMemo(() => findMatches(messages, query), [messages, query]);
  useEffect(() => setHitIndex(0), [query, page?.offset]);
  useEffect(() => {
    if (hits.length === 0) return;
    const target = scroller.current?.querySelector<HTMLElement>(`#transcript-message-${hits[hitIndex]}`);
    target?.scrollIntoView({ block: "center" });
  }, [hits, hitIndex]);
  useEffect(() => {
    setQuery("");
    setShowDetails(false);
  }, [row?.sessionRef]);

  if (!row) {
    return (
      <div className="ss-placeholder">
        <Search size={22} />
        No session selected
      </div>
    );
  }
  const total = page?.totalMessages ?? (page ? page.offset + messages.length + (page.nextCursor ? 1 : 0) : 0);
  const range = page ? { lower: page.offset, upper: page.offset + messages.length } : pageRange(0, 0);
  const tint = rowTint(row);
  const title = rowTitle(row);
  const entries = outline(messages);
  const canOpen = row.resumeCommand != null && terminal !== "copyOnly";
  const known = page?.totalMessages ?? row.messageCount ?? undefined;
  return (
    <div className="ss-transcript" style={{ "--ss-tint": tint } as React.CSSProperties}>
      <header className="ss-head">
        <span className="ss-head-badge"><ToolBrandBadge tool={brandTool(row)} iconSize={20} containerSize={26} /></span>
        <div className="ss-head-titles">
          <div className="ss-head-title" title={title}>{title}</div>
          <div className="ss-head-chips">
            <span className="wb-capsule tall tint" style={{ "--wb-capsule-tint": tint } as React.CSSProperties}>{row.harness}</span>
            {row.model ? <span className="wb-capsule tall mono">{row.model}</span> : null}
            <span className="wb-capsule tall" title={row.projectDir ?? ""}><Folder size={11} /> {projectTitle(row.projectDir)}</span>
            {known != null ? <span className="wb-capsule tall">{known.toLocaleString("en-US")} messages</span> : null}
            <button type="button" className="wb-capsule tall mono ss-id" title="Copy session ID" aria-label={`Session ID ${row.sessionId}. Copy`} onClick={() => onCopy(row.sessionId, "Session ID copied.")}>
              {row.sessionId} <Copy size={10} />
            </button>
          </div>
        </div>
        <div className="ss-head-actions">
          <button type="button" className="wb-pill" title="Copy resume command" disabled={!row.resumeCommand} onClick={() => row.resumeCommand && onCopy(row.resumeCommand, "Resume command copied.")}>
            <Copy size={12} />
          </button>
          <button type="button" className="wb-pill" title={terminal === "copyOnly" ? "Open in is set to Copy only" : `Run it in ${terminal === "iterm2" ? "iTerm2" : "Terminal"}`} disabled={!canOpen} onClick={() => row.resumeCommand && onOpen(row.resumeCommand)}>
            <Terminal size={12} /> Open
          </button>
          <button type="button" className={`wb-pill${showDetails ? " on" : ""}`} aria-expanded={showDetails} onClick={() => setShowDetails((v) => !v)}>
            Details <ChevronDown size={11} />
          </button>
        </div>
      </header>
      {showDetails ? (
        <dl className="ss-details">
          <dt>ID</dt>
          <dd className="mono">{row.sessionId}<button type="button" className="ss-card-copy" style={{ position: "static", opacity: 1 }} title="Copy" onClick={() => onCopy(row.sessionId, "Session ID copied.")}><Copy size={11} /></button></dd>
          <dt>CWD</dt>
          <dd title={row.projectDir ?? ""}>{row.projectDir ?? "No project"}{row.projectDir ? <button type="button" className="ss-card-copy" style={{ position: "static", opacity: 1 }} title="Copy" onClick={() => onCopy(row.projectDir!, "Working directory copied.")}><Copy size={11} /></button> : null}</dd>
          <dt>Source</dt>
          <dd>{row.harness}{row.providerVariant ? ` · ${row.providerVariant}` : ""}</dd>
          <div className="ss-resume">
            <div className="wb-label-caps">Resume</div>
            {row.resumeCommand ? (
              <>
                <code>{row.resumeCommand}</code>
                <div className="ss-resume-actions">
                  <button type="button" className="wb-pill" onClick={() => onCopy(row.resumeCommand!, "Resume command copied.")}><Copy size={12} /> Copy</button>
                  <button type="button" className="wb-pill" disabled={!canOpen} onClick={() => onOpen(row.resumeCommand!)}><Terminal size={12} /> Open in Terminal</button>
                </div>
              </>
            ) : (
              <span className="wb-empty">This session has no command-line entry point.</span>
            )}
          </div>
        </dl>
      ) : null}
      <div className="ss-find" role="search">
        <div className="wb-field">
          <Search size={12} />
          <input type="search" placeholder="Find in transcript" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Find in transcript" />
          {query ? <span className="ss-find-count" aria-live="polite">{hits.length === 0 ? "No matches" : `${hitIndex + 1} of ${hits.length}`}</span> : null}
          {hits.length > 1 ? (
            <span className="ss-find-nav">
              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Previous match" onClick={() => setHitIndex((i) => (i - 1 + hits.length) % hits.length)}><ChevronLeft size={11} /></button>
              <button type="button" className="wb-iconbtn" style={{ width: 20, height: 20 }} title="Next match" onClick={() => setHitIndex((i) => (i + 1) % hits.length)}><ChevronRight size={11} /></button>
            </span>
          ) : null}
        </div>
        <button type="button" className={`ss-outline-toggle${showOutline ? " on" : ""}`} title="Jump to a prompt" aria-label="Show the transcript outline" aria-pressed={showOutline} onClick={() => setShowOutline((v) => !v)}>
          <ListBullet size={14} />
        </button>
        <span className="ss-pager-label">{loading ? "Reading the session log…" : page ? pageLabel(range, Math.max(total, range.upper)) : ""}</span>
        <button type="button" className="wb-pill ss-pager-btn" disabled={!page || page.offset === 0} onClick={() => onPage(-1)}>Previous</button>
        <button type="button" className="wb-pill ss-pager-btn" disabled={!page || (!page.nextCursor && range.upper >= total)} onClick={() => onPage(1)}>Next</button>
      </div>
      <div className="ss-body">
        <div className="ss-messages" ref={scroller}>
          {error ? (
            <div className="ss-placeholder">{error}</div>
          ) : loading && !page ? (
            <div className="ss-placeholder">Reading the session log…</div>
          ) : messages.length === 0 ? (
            <div className="ss-placeholder">This session's log has no readable messages.</div>
          ) : (
            messages.map((message, index) => (
              <MessageCard key={`${page?.offset ?? 0}-${index}`} message={message} index={index} query={query} hit={hits.includes(index)} onCopy={(text) => onCopy(text, "Message copied.")} />
            ))
          )}
        </div>
        {showOutline ? (
          <aside className="ss-outline">
            <h5>Prompts</h5>
            {entries.length === 0 ? (
              <div className="wb-empty">This transcript has no user prompts.</div>
            ) : (
              entries.map((entry) => (
                <button type="button" key={entry.index} onClick={() => scroller.current?.querySelector(`#transcript-message-${entry.index}`)?.scrollIntoView({ block: "start" })}>
                  <b>{entry.seq}</b>
                  <span>{entry.title}</span>
                </button>
              ))
            )}
          </aside>
        ) : null}
      </div>
    </div>
  );
}
