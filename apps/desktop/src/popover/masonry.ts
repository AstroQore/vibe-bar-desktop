/**
 * The Overview's two-column placement, ported from the native
 * `OverviewMasonryPlanner` (Sources/VibeBarCore/Services/OverviewMasonryPlanner.swift).
 *
 * Cards are placed in four phases in declaration order:
 *
 * - **summary** — a row, not a balanced pair. Both cards carry one pinned
 *   height, so declaration order decides the pairing; the columns are taken
 *   shorter-first so an asymmetric earlier group is filled from its hole.
 * - **quota** — with exactly four cards every one of the 24 orders is scored
 *   and the best keeps two per column; any other count goes shortest-first.
 * - **cost** — five at most, so every 2^n column assignment is scored.
 * - **auxiliary** — shortest column, in order.
 *
 * `reflow` rebuilds y positions from a previously chosen column assignment
 * with each card's current height, so a card growing (a forecast opening)
 * pushes the cards below it down without any card jumping columns.
 */
export type Phase = "summary" | "quota" | "cost" | "auxiliary";

export interface Item {
  id: string;
  height: number;
  phase: Phase;
}

export interface Position {
  column: number;
  y: number;
}

export interface Plan {
  positions: Record<string, Position>;
  columnHeights: number[];
}

const PHASE_ORDER: Phase[] = ["summary", "quota", "cost", "auxiliary"];

export function plan(items: Item[], columns = 2, spacing = 12): Plan {
  const clean = items.map((i) => ({ ...i, height: Math.max(0, i.height) }));
  if (columns !== 2) return greedyPlan(clean, Math.max(1, columns), spacing);
  const heights = [0, 0];
  const positions: Record<string, Position> = {};

  const summaries = clean.filter((i) => i.phase === "summary");
  const quotas = clean.filter((i) => i.phase === "quota");
  const costs = clean.filter((i) => i.phase === "cost");
  const auxiliary = clean.filter((i) => i.phase === "auxiliary");

  const summaryColumns = heights[0] <= heights[1] ? [0, 1] : [1, 0];
  summaries.forEach((item, offset) => append(item, summaryColumns[offset % 2], spacing, heights, positions));

  if (quotas.length === 4) {
    const order = bestQuotaOrder(quotas, heights, spacing);
    order.forEach((item, offset) => append(item, offset < 2 ? 0 : 1, spacing, heights, positions));
  } else {
    for (const item of quotas) append(item, shortestColumn(heights), spacing, heights, positions);
  }

  if (costs.length > 0) {
    const cols = bestCostColumns(costs, heights, spacing);
    costs.forEach((item, index) => append(item, cols[index], spacing, heights, positions));
  }

  for (const item of auxiliary) append(item, shortestColumn(heights), spacing, heights, positions);
  return { positions, columnHeights: heights };
}

/** Keep every card in the column it was given, at its current height. */
export function reflow(items: Item[], fixedColumns: Record<string, number>, columns = 2, spacing = 12): Plan {
  const count = Math.max(1, columns);
  const heights = Array.from({ length: count }, () => 0);
  const positions: Record<string, Position> = {};
  const ordered = [...items].sort((a, b) => PHASE_ORDER.indexOf(a.phase) - PHASE_ORDER.indexOf(b.phase));
  for (const item of ordered) {
    const column = Math.min(count - 1, Math.max(0, fixedColumns[item.id] ?? shortestColumn(heights)));
    append({ ...item, height: Math.max(0, item.height) }, column, spacing, heights, positions);
  }
  return { positions, columnHeights: heights };
}

function greedyPlan(items: Item[], columns: number, spacing: number): Plan {
  const heights = Array.from({ length: columns }, () => 0);
  const positions: Record<string, Position> = {};
  const ordered = [...items].sort((a, b) => PHASE_ORDER.indexOf(a.phase) - PHASE_ORDER.indexOf(b.phase));
  for (const item of ordered) append(item, shortestColumn(heights), spacing, heights, positions);
  return { positions, columnHeights: heights };
}

function append(item: Item, column: number, spacing: number, heights: number[], positions: Record<string, Position>) {
  const y = heights[column] + (heights[column] > 0 ? spacing : 0);
  positions[item.id] = { column, y };
  heights[column] = y + item.height;
}

function appendedHeight(current: number, item: number, spacing: number): number {
  return current + (current > 0 ? spacing : 0) + item;
}

function shortestColumn(heights: number[]): number {
  let best = 0;
  for (let i = 1; i < heights.length; i += 1) if (heights[i] < heights[best]) best = i;
  return best;
}

function stacked(seed: number, items: Item[], spacing: number): number {
  return items.reduce((h, i) => appendedHeight(h, i.height, spacing), seed);
}

/** Lexicographic: first balance, then the taller column. */
function quotaScore(order: Item[], seed: number[], spacing: number): [number, number] {
  const left = stacked(seed[0] ?? 0, order.slice(0, 2), spacing);
  const right = stacked(seed[1] ?? 0, order.slice(2), spacing);
  return [Math.abs(left - right), Math.max(left, right)];
}

function lexLess(a: number[], b: number[]): boolean {
  for (let i = 0; i < Math.min(a.length, b.length); i += 1) {
    if (a[i] < b[i]) return true;
    if (a[i] > b[i]) return false;
  }
  return a.length < b.length;
}

function bestQuotaOrder(items: Item[], seed: number[], spacing: number): Item[] {
  let best = items;
  let bestScore = quotaScore(items, seed, spacing);
  for (const candidate of permutations(items)) {
    const score = quotaScore(candidate, seed, spacing);
    if (lexLess(score, bestScore)) {
      best = candidate;
      bestScore = score;
    }
  }
  return best;
}

/** Lexicographic: the bottom edge, then balance, then fewer cards on the right. */
function bestCostColumns(items: Item[], seed: number[], spacing: number): number[] {
  let bestColumns = items.map(() => 0);
  let bestScore = [Infinity, Infinity, Infinity];
  const combinations = 1 << items.length;
  for (let mask = 0; mask < combinations; mask += 1) {
    const heights = [seed[0] ?? 0, seed[1] ?? 0];
    const columns: number[] = [];
    items.forEach((item, index) => {
      const column = (mask >> index) & 1;
      columns.push(column);
      heights[column] = appendedHeight(heights[column], item.height, spacing);
    });
    const score = [Math.max(heights[0], heights[1]), Math.abs(heights[0] - heights[1]), columns.reduce((a, b) => a + b, 0)];
    if (lexLess(score, bestScore)) {
      bestScore = score;
      bestColumns = columns;
    }
  }
  return bestColumns;
}

function permutations<T>(values: T[]): T[][] {
  if (values.length <= 1) return [values];
  return values.flatMap((head, index) => {
    const rest = [...values.slice(0, index), ...values.slice(index + 1)];
    return permutations(rest).map((tail) => [head, ...tail]);
  });
}

export interface Session {
  /** Column per card, frozen once every card has been measured. */
  columns: Record<string, number>;
}

/**
 * One pass of the waterfall's session: plan while any card is unmeasured or
 * the card set has changed, freeze columns the first time every card has a
 * real height, and only re-flow after that.
 *
 * Freezing on the first plan — before `ResizeObserver` had reported — meant
 * the exhaustive balancing ran on zeros and the measured heights only ever
 * re-flowed those columns. The plan that counts is the first one with real
 * numbers in it.
 */
export function step(session: Session, items: Item[], measured: Set<string>, columns = 2, spacing = 12): { plan: Plan; session: Session } {
  const ids = new Set(items.map((i) => i.id));
  const frozenIds = Object.keys(session.columns);
  const sameSet = frozenIds.length === items.length && frozenIds.every((id) => ids.has(id));
  const allMeasured = items.every((i) => measured.has(i.id));
  if (sameSet && frozenIds.length > 0) {
    return { plan: reflow(items, session.columns, columns, spacing), session };
  }
  const fresh = plan(items, columns, spacing);
  if (!allMeasured || items.length === 0) return { plan: fresh, session: { columns: {} } };
  return {
    plan: fresh,
    session: { columns: Object.fromEntries(Object.entries(fresh.positions).map(([id, p]) => [id, p.column])) },
  };
}
