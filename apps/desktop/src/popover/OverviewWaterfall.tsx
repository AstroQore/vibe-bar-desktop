/**
 * Native `OverviewWaterfall` in `.auto` mode: the cards in declaration order,
 * measured, handed to the planner, and placed absolutely in two columns.
 *
 * Measured rather than estimated because the planner's whole job is height
 * balance, and a card's height is whatever its buckets and forecasts add up
 * to. The first plan chooses columns; later passes keep those columns and
 * only re-flow the y positions (`reflow`), so a card that grows pushes its
 * neighbours down without anything jumping across — native's `Session`.
 */
import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { plan, reflow, type Item, type Plan } from "./masonry";
import type { Density } from "./theme";

export interface WaterfallCard { id: string; phase: Item["phase"]; node: ReactNode }

export function OverviewWaterfall({ cards, width, density }: { cards: WaterfallCard[]; width: number; density: Density }) {
  const spacing = density.interSectionSpacing;
  const columnWidth = Math.max(0, (width - spacing) / 2);
  const refs = useRef(new Map<string, HTMLDivElement>());
  const [heights, setHeights] = useState<Record<string, number>>({});
  const columnsRef = useRef<Record<string, number>>({});

  // One observer for every card; a card that changes size re-plans the page.
  useLayoutEffect(() => {
    const observer = new ResizeObserver((entries) => {
      setHeights((prev) => {
        let changed = false;
        const next = { ...prev };
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.cardId;
          if (!id) continue;
          const h = Math.ceil(entry.contentRect.height);
          if (next[id] !== h) { next[id] = h; changed = true; }
        }
        return changed ? next : prev;
      });
    });
    for (const el of refs.current.values()) observer.observe(el);
    return () => observer.disconnect();
  }, [cards.map((c) => c.id).join("|")]);

  const layout: Plan = useMemo(() => {
    const items: Item[] = cards.map((c) => ({ id: c.id, phase: c.phase, height: heights[c.id] ?? 0 }));
    const ids = new Set(items.map((i) => i.id));
    const known = Object.keys(columnsRef.current).filter((id) => ids.has(id));
    // A fresh set of cards chooses columns; the same set only re-flows.
    if (known.length !== items.length || items.some((i) => !(i.id in columnsRef.current))) {
      const first = plan(items, 2, spacing);
      columnsRef.current = Object.fromEntries(Object.entries(first.positions).map(([id, p]) => [id, p.column]));
      return first;
    }
    return reflow(items, columnsRef.current, 2, spacing);
  }, [cards, heights, spacing]);

  const height = Math.max(...layout.columnHeights, 0);
  return (
    <div className="pv-waterfall" style={{ width, height }}>
      {cards.map((card) => {
        const pos = layout.positions[card.id];
        return (
          <div
            key={card.id}
            data-card-id={card.id}
            ref={(el) => { if (el) refs.current.set(card.id, el); else refs.current.delete(card.id); }}
            className="pv-slot"
            style={{
              width: columnWidth,
              transform: `translate(${(pos?.column ?? 0) * (columnWidth + spacing)}px, ${pos?.y ?? 0}px)`,
              visibility: heights[card.id] === undefined ? "hidden" : "visible",
            }}
          >
            {card.node}
          </div>
        );
      })}
    </div>
  );
}
