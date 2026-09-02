import { describe, expect, it } from "vitest";
import { plan, reflow, step, type Item } from "./masonry";

const item = (id: string, height: number, phase: Item["phase"]): Item => ({ id, height, phase });

describe("the overview's two-column placement", () => {
  it("lays the summary band out as a row, left then right", () => {
    const p = plan([item("cost", 178, "summary"), item("status", 178, "summary")]);
    expect(p.positions.cost).toEqual({ column: 0, y: 0 });
    expect(p.positions.status).toEqual({ column: 1, y: 0 });
  });

  it("keeps two of four quota cards per column and balances the columns", () => {
    // Heights chosen so greedy shortest-first would put three on one side.
    const p = plan([
      item("openai", 580, "quota"),
      item("anthropic", 560, "quota"),
      item("google", 1080, "quota"),
      item("spacexai", 300, "quota"),
    ]);
    const left = Object.entries(p.positions).filter(([, v]) => v.column === 0).map(([k]) => k);
    const right = Object.entries(p.positions).filter(([, v]) => v.column === 1).map(([k]) => k);
    expect(left).toHaveLength(2);
    expect(right).toHaveLength(2);
    // Google is the tall one; it has to sit opposite the two mid cards.
    expect(left.includes("google")).not.toBe(right.includes("google"));
    // {google, spacexai} against {openai, anthropic}: 1392 vs 1152. Every
    // other two-per-column split is further apart.
    expect(Math.abs(p.columnHeights[0] - p.columnHeights[1])).toBe(240);
  });

  it("spaces stacked cards but not the first in a column", () => {
    const p = plan([item("a", 100, "quota"), item("b", 100, "quota"), item("c", 100, "quota")], 2, 14);
    expect(p.positions.a.y).toBe(0);
    expect(p.positions.b.y).toBe(0);
    expect(p.positions.c).toEqual({ column: 0, y: 114 });
  });

  it("chooses cost columns by the bottom edge first", () => {
    // Seeded after quotas: left 600, right 200. Two cost cards of 300 and
    // 100 — the bottom edge is lowest with both on the right: 200+12+300+12+100
    // = 624, against 712 or worse for every split.
    const p = plan([
      item("q1", 600, "quota"),
      item("q2", 200, "quota"),
      item("c1", 300, "cost"),
      item("c2", 100, "cost"),
    ]);
    expect(p.positions.c1.column).toBe(1);
    expect(p.positions.c2.column).toBe(1);
    expect(Math.max(...p.columnHeights)).toBe(624);
  });

  it("puts auxiliary cards on the shortest column", () => {
    const p = plan([item("q1", 500, "quota"), item("q2", 100, "quota"), item("mix", 200, "auxiliary")]);
    expect(p.positions.mix.column).toBe(1);
  });

  it("reflows heights without moving a card across columns", () => {
    const first = plan([item("a", 100, "quota"), item("b", 100, "quota"), item("c", 100, "quota")]);
    const columns = Object.fromEntries(Object.entries(first.positions).map(([k, v]) => [k, v.column]));
    // `a` grows: `c`, below it, moves down; nothing changes column.
    const second = reflow([item("a", 400, "quota"), item("b", 100, "quota"), item("c", 100, "quota")], columns);
    expect(second.positions.a.column).toBe(first.positions.a.column);
    expect(second.positions.c.column).toBe(first.positions.c.column);
    expect(second.positions.c.y).toBe(412);
  });

  it("reproduces the native Overview's arrangement for the screenshot's cards", () => {
    // Measured from the native docs/screenshots/popover-overview.png at the
    // regular density: Cost | Status row, then OpenAI+Anthropic+SpaceXAI on
    // the left against Google AI on the right, Usage Mix under Google AI.
    const p = plan([
      item("cost", 178, "summary"),
      item("status", 178, "summary"),
      item("openai", 580, "quota"),
      item("anthropic", 580, "quota"),
      item("google", 1085, "quota"),
      item("spacexai", 250, "quota"),
      item("mix", 340, "auxiliary"),
    ]);
    expect(p.positions.cost.column).toBe(0);
    expect(p.positions.status.column).toBe(1);
    expect(p.positions.google.column).toBe(1);
    expect(p.positions.openai.column).toBe(0);
    expect(p.positions.anthropic.column).toBe(0);
    expect(p.positions.spacexai.column).toBe(1);
    expect(p.positions.mix.column).toBe(0);
  });

  it("does not freeze columns until every card has been measured", () => {
    const items = [item("a", 0, "quota"), item("b", 0, "quota"), item("c", 0, "quota"), item("d", 0, "quota")];
    const first = step({ columns: {} }, items, new Set());
    expect(first.session.columns).toEqual({});
    // Half measured: still no freeze, still a fresh plan.
    const half = step(first.session, items.map((i) => (i.id < "c" ? { ...i, height: 300 } : i)), new Set(["a", "b"]));
    expect(half.session.columns).toEqual({});
    // All measured: the plan that counts, and the columns it chose stick.
    const measured = [item("a", 580, "quota"), item("b", 560, "quota"), item("c", 1080, "quota"), item("d", 300, "quota")];
    const full = step(half.session, measured, new Set(["a", "b", "c", "d"]));
    expect(Object.keys(full.session.columns).sort()).toEqual(["a", "b", "c", "d"]);
    expect(full.plan.positions.c.column).not.toBe(full.plan.positions.a.column);
    // A later height change re-flows within those columns.
    const grown = step(full.session, measured.map((i) => (i.id === "a" ? { ...i, height: 900 } : i)), new Set(["a", "b", "c", "d"]));
    expect(grown.session).toBe(full.session);
    for (const id of ["a", "b", "c", "d"]) expect(grown.plan.positions[id].column).toBe(full.plan.positions[id].column);
  });

  it("re-plans when the card set changes", () => {
    const measured = [item("a", 100, "quota"), item("b", 100, "quota")];
    const frozen = step({ columns: {} }, measured, new Set(["a", "b"]));
    const changed = step(frozen.session, [...measured, item("c", 100, "quota")], new Set(["a", "b", "c"]));
    expect(Object.keys(changed.session.columns).sort()).toEqual(["a", "b", "c"]);
  });
});
