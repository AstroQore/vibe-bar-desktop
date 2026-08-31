/**
 * A page for looking at the mini-window layouts side by side.
 *
 * Not shipped: `index.html` is the app's only build entry, so this is reached
 * with `pnpm dev` and `/preview.html`. It exists because the layouts are the
 * one part of this client where the tests cannot see the bug — labels that
 * wrap, bars that stop lining up, a forecast marker invisible against the fill
 * it sits on were all found here and none of them by a unit test.
 *
 * The data is synthetic and deliberately awkward: a long label, a bucket
 * forecast past its limit, and one SubProvider with a named model group.
 */
import { createRoot } from "react-dom/client";

import "./styles.css";
import type { AccountQuota, PresentationSettings, QuotaView } from "./api";
import { MiniQuotaBody, arrange } from "./components/MiniQuota";

const NOW = Math.floor(Date.now() / 1000);

function bucket(id: string, title: string, used: number, groupTitle?: string) {
  return {
    id,
    title,
    shortLabel: title,
    usedPercent: used,
    resetAt: NOW + 86_400 * 2 + 3600 * 5,
    rawWindowSeconds: 604_800,
    groupTitle,
    forecast: {
      verdict: used > 70 ? "atRisk" : "enough",
      projectedUsedPercent: Math.min(120, used + 25),
      // What the marker reads. Setting only `projectedUsedPercent` here once
      // made every layout look as though it had no forecast at all.
      plannedUsedPercent: Math.min(100, used + 18),
      confidence: "medium",
    },
  } as unknown as AccountQuota["buckets"][number];
}

const view = {
  accounts: [
    {
      accountId: "a-codex",
      tool: "codex",
      queriedAt: NOW - 60,
      origin: "live",
      buckets: [bucket("five_hour", "5 Hours", 38), bucket("weekly", "Weekly", 82)],
    },
    {
      accountId: "a-claude",
      tool: "claude",
      queriedAt: NOW - 60,
      origin: "live",
      buckets: [
        bucket("five_hour", "5 Hours", 12),
        bucket("weekly", "Weekly", 61),
        bucket("weekly_opus", "Weekly", 94, "Opus"),
      ],
    },
  ] as unknown as AccountQuota[],
  generatedAt: NOW,
  hasSharedData: true,
} as unknown as QuotaView;

const settings = { displayMode: "remaining", customLabels: {} } as unknown as PresentationSettings;
const companies = arrange(view, settings, [
  "codex.five_hour",
  "codex.weekly",
  "claude.five_hour",
  "claude.weekly",
  "claude.weekly_opus",
]);

const panel = {
  border: "1px solid rgba(128,128,128,0.28)",
  borderRadius: 10,
  overflow: "hidden",
  width: "fit-content",
} as const;

createRoot(document.getElementById("root")!).render(
  <div style={{ display: "flex", gap: 32, padding: 20, alignItems: "flex-start", flexWrap: "wrap" }}>
    {["regular", "compact", "ledger"].map((layout) => (
      <div key={layout}>
        <p style={{ font: "12px system-ui", opacity: 0.6, margin: "0 0 6px" }}>{layout}</p>
        <div style={{ ...panel, padding: layout === "ledger" ? 0 : 8 }}>
          <MiniQuotaBody companies={companies} layout={layout} />
        </div>
      </div>
    ))}
  </div>,
);
