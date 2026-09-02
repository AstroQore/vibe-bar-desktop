import type { SessionRow } from "../../api";
import { ToolBrandBadge } from "../../popover/brand";
import { Folder } from "../icons";
import { type RowGroup, brandTool, companyOf, projectTitle, relativeTime } from "./model";

const COMPANY_TINT: Record<string, string> = { OpenAI: "#5F8F7A", Anthropic: "#D97757", "Google AI": "#4285F4", SpaceXAI: "#6E6E73" };

function excerptNodes(excerpt: string | undefined): React.ReactNode {
  if (!excerpt) return null;
  // The index marks hits as <mark>…</mark>; render those and nothing else as markup.
  const parts = excerpt.split(/(<mark>.*?<\/mark>)/g);
  return parts.map((part, index) => {
    const hit = part.match(/^<mark>(.*)<\/mark>$/);
    return hit ? <mark key={index}>{hit[1]}</mark> : <span key={index}>{part.replace(/<\/?[a-z]+>/g, "")}</span>;
  });
}

export function SessionRowView({
  row,
  selected,
  deleteMode,
  checked,
  now,
  onSelect,
  onToggleCheck,
}: {
  row: SessionRow;
  selected: boolean;
  deleteMode: boolean;
  checked: boolean;
  now: number;
  onSelect: () => void;
  onToggleCheck: () => void;
}) {
  const tint = COMPANY_TINT[companyOf(row.harness)] ?? "var(--wb-accent)";
  const title = row.title?.trim() || row.excerpt?.replace(/<\/?[a-z]+>/g, "").trim() || row.sessionId;
  return (
    <button
      type="button"
      className={`ss-row${selected ? " selected" : ""}`}
      style={{ "--ss-tint": tint } as React.CSSProperties}
      onClick={deleteMode ? onToggleCheck : onSelect}
      aria-pressed={selected}
      title={deleteMode ? "Toggle this session for deletion" : "Show this transcript"}
    >
      {deleteMode ? <span className={`ss-row-check${checked ? " on" : ""}`} aria-hidden="true" /> : <span className="ss-head-badge"><ToolBrandBadge tool={brandTool(row)} iconSize={17} containerSize={24} /></span>}
      <span className="ss-row-body">
        <span className="ss-row-title">{title}</span>
        <span className="ss-row-line">
          {row.harness}
          {row.model ? <span className="ss-model">{row.model}</span> : row.providerVariant ? <span className="ss-model">{row.providerVariant}</span> : null}
        </span>
        {row.excerpt ? <span className="ss-row-excerpt">{excerptNodes(row.excerpt)}</span> : null}
        <span className="ss-row-foot">
          <Folder size={11} />
          {projectTitle(row.projectDir)}
          <span>{relativeTime(row.lastActiveAt, now)}</span>
          {row.messageCount != null ? (
            <span className="ss-count" title={`${row.messageCount} messages`}>
              {row.messageCount}
            </span>
          ) : null}
        </span>
      </span>
    </button>
  );
}

/** The native `SessionListView`: cards in a reading queue, optionally under
 *  project headings, with the empty states that say whose fault it is. */
export function SessionList({
  groups,
  rows,
  selectedRef,
  deleteMode,
  checked,
  now,
  onSelect,
  onToggleCheck,
  empty,
  canLoadMore,
  onLoadMore,
  loading,
}: {
  groups: RowGroup[] | null;
  rows: SessionRow[];
  selectedRef: string | null;
  deleteMode: boolean;
  checked: Set<string>;
  now: number;
  onSelect: (row: SessionRow) => void;
  onToggleCheck: (row: SessionRow) => void;
  empty: { title: string; detail: string } | null;
  canLoadMore: boolean;
  onLoadMore: () => void;
  loading: boolean;
}) {
  const render = (row: SessionRow) => (
    <SessionRowView
      key={row.sessionRef}
      row={row}
      selected={row.sessionRef === selectedRef}
      deleteMode={deleteMode}
      checked={checked.has(row.sessionRef)}
      now={now}
      onSelect={() => onSelect(row)}
      onToggleCheck={() => onToggleCheck(row)}
    />
  );
  return (
    <div className="ss-list" role="list">
      {empty && rows.length === 0 ? (
        <div className="ss-empty">
          <div className="ss-empty-title">{empty.title}</div>
          <div className="ss-empty-detail">{empty.detail}</div>
        </div>
      ) : groups ? (
        groups.map((group) => (
          <div key={group.title}>
            <div className="ss-group">{group.title}</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>{group.rows.map(render)}</div>
          </div>
        ))
      ) : (
        rows.map(render)
      )}
      {canLoadMore ? (
        <button type="button" className="ss-more" onClick={onLoadMore} disabled={loading}>
          {loading ? "Loading…" : "Load more"}
        </button>
      ) : null}
    </div>
  );
}
