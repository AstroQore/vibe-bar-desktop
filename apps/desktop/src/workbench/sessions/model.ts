/**
 * The Sessions page's pure logic: filter vocab, the listing query, folder
 * filters, grouping and sorting, transcript paging and collapsing. Mirrors
 * the native `SessionManagerModel` so the components only render.
 */
import type { SessionListingQuery, SessionRow, TranscriptMessage } from "../../api";

export type DateRange = "all" | "today" | "week" | "month";
export const DATE_RANGES: ReadonlyArray<{ id: DateRange; title: string }> = [
  { id: "all", title: "Any time" },
  { id: "today", title: "Today" },
  { id: "week", title: "7 days" },
  { id: "month", title: "30 days" },
];

export function sinceFor(range: DateRange, now: number): number | undefined {
  switch (range) {
    case "all":
      return undefined;
    case "today": {
      const start = new Date(now * 1000);
      start.setHours(0, 0, 0, 0);
      return Math.floor(start.getTime() / 1000);
    }
    case "week":
      return Math.floor(now - 7 * 86_400);
    case "month":
      return Math.floor(now - 30 * 86_400);
  }
}

export type SortOrder = "recentFirst" | "oldestFirst";
export const SORT_ORDERS: ReadonlyArray<{ id: SortOrder; title: string }> = [
  { id: "recentFirst", title: "Newest first" },
  { id: "oldestFirst", title: "Oldest first" },
];

export type SearchScope = "title" | "user" | "assistant" | "system" | "tool";
export const SEARCH_SCOPES: ReadonlyArray<{ id: SearchScope; title: string }> = [
  { id: "title", title: "Titles and session IDs" },
  { id: "user", title: "User prompts" },
  { id: "assistant", title: "Assistant replies" },
  { id: "system", title: "System prompts" },
  { id: "tool", title: "Tool and file operations" },
];

export type PreferredTerminal = "terminal" | "iterm2" | "copyOnly";
export const TERMINALS: ReadonlyArray<{ id: PreferredTerminal; title: string }> = [
  { id: "terminal", title: "Terminal" },
  { id: "iterm2", title: "iTerm2" },
  { id: "copyOnly", title: "Copy only" },
];

/** Billing company per harness label, for the Company menu and chip tints. */
export const HARNESS_COMPANY: Record<string, string> = {
  Codex: "OpenAI",
  "ChatGPT Work": "OpenAI",
  "Claude Code": "Anthropic",
  "Claude Cowork": "Anthropic",
  "Gemini CLI": "Google AI",
  "Gemini Web": "Google AI",
  AntiGravity: "Google AI",
  "Grok Build": "SpaceXAI",
  Grok: "SpaceXAI",
  Cursor: "SpaceXAI",
  "Grok Bot": "SpaceXAI",
};

/** Raw provider ids the core understands, per company. */
export const COMPANY_PROVIDERS: Record<string, string[]> = {
  OpenAI: ["codex"],
  Anthropic: ["claude", "claudeCowork"],
  "Google AI": ["gemini"],
  SpaceXAI: ["grok", "cursor"],
};

export const COMPANY_ORDER = ["OpenAI", "Anthropic", "Google AI", "SpaceXAI"] as const;

export function companyOf(harness: string): string {
  return HARNESS_COMPANY[harness] ?? "Other";
}

/** Brand icon id for a row: the provider, refined by the variant. */
export function brandTool(row: Pick<SessionRow, "provider" | "harness">): string {
  if (row.harness === "AntiGravity") return "antigravity";
  if (row.harness === "Cursor" || row.harness === "Grok Bot") return "cursor";
  return row.provider === "claudeCowork" ? "claude" : row.provider;
}

export interface FolderFilters {
  include: string[];
  exclude: string[];
}

/** `/a, /b` → `["/a", "/b"]`; whitespace and empties dropped. */
export function parseFolderList(text: string): string[] {
  return text
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

export function applyFolderFilters<T extends { projectDir?: string }>(rows: T[], filters: FolderFilters): T[] {
  if (filters.include.length === 0 && filters.exclude.length === 0) return rows;
  return rows.filter((row) => {
    const dir = row.projectDir ?? "";
    if (filters.include.length > 0 && !filters.include.some((part) => dir.includes(part))) return false;
    if (filters.exclude.some((part) => dir.includes(part))) return false;
    return true;
  });
}

/** The last path component, or the native placeholders. */
export function projectTitle(projectDir: string | undefined): string {
  if (!projectDir) return "No project";
  const trimmed = projectDir.replace(/[\\/]+$/, "");
  const last = trimmed.split(/[\\/]/).pop();
  return last && last.length > 0 ? last : projectDir;
}

export function sortRows(rows: SessionRow[], order: SortOrder): SessionRow[] {
  const sorted = [...rows].sort((a, b) => (a.lastActiveAt ?? 0) - (b.lastActiveAt ?? 0));
  return order === "oldestFirst" ? sorted : sorted.reverse();
}

export interface RowGroup {
  title: string;
  rows: SessionRow[];
}

/** Group rows by project title, groups ordered by their newest session. */
export function groupRows(rows: SessionRow[]): RowGroup[] {
  const groups = new Map<string, RowGroup>();
  for (const row of rows) {
    const title = projectTitle(row.projectDir);
    const group = groups.get(title) ?? { title, rows: [] };
    group.rows.push(row);
    groups.set(title, group);
  }
  return [...groups.values()].sort((a, b) => newest(b.rows) - newest(a.rows));
}

function newest(rows: SessionRow[]): number {
  return rows.reduce((max, row) => Math.max(max, row.lastActiveAt ?? 0), 0);
}

/** `1h ago`, `3d ago` — the list row's timestamp. */
export function relativeTime(at: number | undefined, now: number): string {
  if (!at) return "";
  const seconds = Math.max(0, now - at);
  if (seconds < 60) return "just now";
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h ago`;
  if (seconds < 30 * 86_400) return `${Math.floor(seconds / 86_400)}d ago`;
  return `${Math.floor(seconds / (30 * 86_400))}mo ago`;
}

/** Build the core query from the page's filter state. `null` selections
 *  mean everything; an empty harness list means nothing, like the native
 *  All chip. */
export function buildListingQuery(state: {
  search: string;
  companies: string[] | null;
  harnesses: string[] | null;
  range: DateRange;
  offset: number;
  limit: number;
  now: number;
}): SessionListingQuery {
  const providers = state.companies === null ? undefined : state.companies.flatMap((company) => COMPANY_PROVIDERS[company] ?? []);
  return {
    query: state.search.trim() || undefined,
    providers,
    harnesses: state.harnesses ?? undefined,
    since: sinceFor(state.range, state.now),
    offset: state.offset,
    limit: state.limit,
  };
}

export function harnessSummary(selected: string[] | null, available: number): string {
  if (selected === null) return "All";
  return selected.length === available ? "All" : `${selected.length}/${available}`;
}

export function indexStatusText(shown: number, total: number | undefined, indexed: boolean): string {
  if (!indexed) return "index unavailable";
  if (total != null && total > shown) return `${shown} of ${total} sessions`;
  return shown === 1 ? "1 session" : `${shown} sessions`;
}

/** The transcript page window, native `TranscriptPageWindow`: 80 messages,
 *  clamped so the last page is full when it can be. */
export const TRANSCRIPT_PAGE = 80;

export function pageRange(start: number, count: number, size = TRANSCRIPT_PAGE): { lower: number; upper: number } {
  if (count <= 0) return { lower: 0, upper: 0 };
  const clampedStart = Math.max(0, Math.min(start, Math.max(0, count - size)));
  return { lower: clampedStart, upper: Math.min(count, clampedStart + size) };
}

export function pageLabel(range: { lower: number; upper: number }, count: number): string {
  return `Messages ${range.lower + 1}–${range.upper} of ${count}`;
}

export const COLLAPSE_THRESHOLD = 3_000;
export const COLLAPSED_LENGTH = 1_500;

export function collapses(text: string): boolean {
  return text.length > COLLAPSE_THRESHOLD;
}

export function collapsed(text: string): string {
  return `${text.slice(0, COLLAPSED_LENGTH)}…`;
}

export function roleLabel(role: TranscriptMessage["role"]): string {
  switch (role) {
    case "user":
      return "You";
    case "assistant":
      return "Assistant";
    case "tool":
      return "Tool";
    case "system":
      return "System";
    default:
      return "Note";
  }
}

/** Indices of messages containing the needle, case-insensitively. */
export function findMatches(messages: TranscriptMessage[], needle: string): number[] {
  const query = needle.trim().toLowerCase();
  if (query.length === 0) return [];
  const hits: number[] = [];
  messages.forEach((message, index) => {
    if (message.text.toLowerCase().includes(query)) hits.push(index);
  });
  return hits;
}

/** Split `text` around case-insensitive matches of `needle` for highlighting. */
export function splitHighlights(text: string, needle: string): Array<{ text: string; hit: boolean }> {
  const query = needle.trim();
  if (query.length === 0) return [{ text, hit: false }];
  const lower = text.toLowerCase();
  const target = query.toLowerCase();
  const parts: Array<{ text: string; hit: boolean }> = [];
  let cursor = 0;
  for (;;) {
    const at = lower.indexOf(target, cursor);
    if (at < 0) break;
    if (at > cursor) parts.push({ text: text.slice(cursor, at), hit: false });
    parts.push({ text: text.slice(at, at + query.length), hit: true });
    cursor = at + query.length;
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor), hit: false });
  return parts;
}

/** `HH:mm:ss` for a message stamp in any ISO form the readers emit. */
export function messageTime(stamp: string | undefined): string {
  if (!stamp) return "";
  const date = new Date(stamp);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (v: number) => (v < 10 ? `0${v}` : `${v}`);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** The outline: user prompts with their message index and a one-line title. */
export function outline(messages: TranscriptMessage[]): Array<{ index: number; seq: number; title: string }> {
  let seq = 0;
  const entries: Array<{ index: number; seq: number; title: string }> = [];
  messages.forEach((message, index) => {
    if (message.role !== "user") return;
    seq += 1;
    const firstLine = message.text.split("\n").find((line) => line.trim().length > 0) ?? "";
    entries.push({ index, seq, title: firstLine.trim().slice(0, 120) });
  });
  return entries;
}
