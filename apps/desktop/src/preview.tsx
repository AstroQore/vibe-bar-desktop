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
import { PopoverRoot } from "./popover/PopoverRoot";
import { UsageStatsPage } from "./workbench/usage/UsageStatsPage";
import { FIXTURE_USAGE } from "./workbench/usage/fixture";
import { SessionsPage } from "./workbench/sessions/SessionsPage";
import { FIXTURE_SESSIONS, FIXTURE_TRANSCRIPT } from "./workbench/sessions/fixture";
import { ResetsPage } from "./workbench/resets/ResetsPage";
import { FIXTURE_RESET_HISTORY } from "./workbench/resets/fixture";
import { SkillsPage } from "./workbench/skills/SkillsPage";
import { FIXTURE_SKILLS } from "./workbench/skills/fixture";
import { SettingsPage } from "./workbench/settings/SettingsPage";
import { FIXTURE_INFO, FIXTURE_PRICING } from "./workbench/settings/fixture";
import "./workbench/porcelain.css";
import { FIXTURE_COST, FIXTURE_NOW, FIXTURE_SETTINGS, FIXTURE_STATUS, FIXTURE_VIEW } from "./popover/fixture";
import "./popover/popover.css";

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

const popoverActions = {
  refreshAll: () => undefined, refreshCost: () => undefined, refreshStatus: () => undefined,
  refreshProvider: () => undefined, toggleMini: () => undefined, showWorkbench: () => undefined, showSettings: () => undefined,
};
const popoverData = {
  view: FIXTURE_VIEW, settings: FIXTURE_SETTINGS, cost: FIXTURE_COST, status: FIXTURE_STATUS, info: null,
  refreshing: false, statusRefreshing: false, costRefreshing: false,
};
/** The native docs/screenshots/popover-overview.png, drawn by this client. */
function PopoverPreview() {
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  return (
    <div id="popover-preview" style={{ display: "inline-block", borderRadius: 14, background: dark ? "#1e1e20" : "#f7f7f9", margin: "24px 0 8px" }}>
      <PopoverRoot data={popoverData} actions={popoverActions} now={FIXTURE_NOW} dark={dark} initialPage={(new URLSearchParams(location.search).get("page") as never) ?? "overview"} />
    </div>
  );
}
/** One surface alone on a flat backdrop, for `scripts/capture-screenshots.mjs`:
 *  `?surface=popover` or `?surface=mini&layout=<entry>`; the appearance comes
 *  from the emulated `prefers-color-scheme`. */
function CaptureAlone({ children }: { children: React.ReactNode }) {
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  return (
    <div id="capture" style={{ display: "inline-block", padding: 24, background: dark ? "#1c1c1e" : "#e9e9ee" }}>
      {children}
    </div>
  );
}
function MiniAlone({ entry }: { entry: string }) {
  return (
    <CaptureAlone>
      <div style={{ ...panel, padding: entry === "regular" || entry === "compact" ? 8 : 0 }}>
        <MiniQuotaBody companies={companies} layout={entry.split(":")[0]} density={entry.split(":")[1]} />
      </div>
    </CaptureAlone>
  );
}

function UsagePreview() {
  const dark = new URLSearchParams(location.search).get("dark") === "1";
  return (
    <div className={`wb${dark ? " dark" : ""}`} style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <UsageStatsPage fixture={FIXTURE_USAGE} now={FIXTURE_NOW} />
    </div>
  );
}
function SessionsPreview() {
  const dark = new URLSearchParams(location.search).get("dark") === "1";
  return (
    <div className={`wb${dark ? " dark" : ""}`} style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <SessionsPage fixture={{ listing: FIXTURE_SESSIONS, transcript: FIXTURE_TRANSCRIPT }} now={FIXTURE_NOW} />
    </div>
  );
}
function ResetsPreview() {
  const dark = new URLSearchParams(location.search).get("dark") === "1";
  return (
    <div className={`wb${dark ? " dark" : ""}`} style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <ResetsPage view={FIXTURE_VIEW} settings={FIXTURE_SETTINGS as never} now={FIXTURE_NOW} history={FIXTURE_RESET_HISTORY} />
    </div>
  );
}
function SkillsPreview() {
  const dark = new URLSearchParams(location.search).get("dark") === "1";
  return (
    <div className={`wb${dark ? " dark" : ""}`} style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <SkillsPage fixture={FIXTURE_SKILLS} dark={dark} />
    </div>
  );
}
function SettingsPreview() {
  const dark = new URLSearchParams(location.search).get("dark") === "1";
  const settings = { ...(FIXTURE_SETTINGS as never as Record<string, unknown>), menuBar: { isVisible: true, showTitle: false, layout: "twoRows" }, miscProviderInstances: [{ id: "copilot-1", tool: "copilot", name: "Copilot", isVisible: true }, { id: "opencodego-1", tool: "openCodeGo", name: "OpenCode Go", isVisible: true }] } as never;
  return (
    <div className={`wb${dark ? " dark" : ""}`} style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <SettingsPage settings={settings} info={FIXTURE_INFO} cost={FIXTURE_COST} view={FIXTURE_VIEW} dark={dark} onSave={() => undefined} replacedKeys={null} onDismissReplaced={() => undefined} onRescanCost={async () => undefined} onCheckConnections={async () => undefined} fixture pricingFixture={FIXTURE_PRICING} initialSection={(new URLSearchParams(location.search).get("section") ?? "system") as never} />
    </div>
  );
}
const surface = new URLSearchParams(location.search).get("surface");
const layoutParam = new URLSearchParams(location.search).get("layout") ?? "regular";
createRoot(document.getElementById("root")!).render(
  surface === "popover" ? (
    <CaptureAlone>
      <PopoverPreview />
    </CaptureAlone>
  ) : surface === "mini" ? (
    <MiniAlone entry={layoutParam} />
  ) : (
  <>
    {surface === "usage" ? <UsagePreview /> : surface === "sessions" ? <SessionsPreview /> : surface === "resets" ? <ResetsPreview /> : surface === "skills" ? <SkillsPreview /> : surface === "settings" ? <SettingsPreview /> : <PopoverPreview />}
  <div style={{ display: "flex", gap: 32, padding: 20, alignItems: "flex-start", flexWrap: "wrap" }}>
    {[
      "regular",
      "compact",
      "ledger",
      "tile",
      "focus",
      "rail",
      // The strip is one layout at three densities, and they differ enough
      // that seeing one says little about the others.
      "strip:roomy",
      "strip:twoLine",
      "strip:narrow",
    ].map((entry) => (
      <div key={entry}>
        <p style={{ font: "12px system-ui", opacity: 0.6, margin: "0 0 6px" }}>{entry}</p>
        <div style={{ ...panel, padding: entry === "regular" || entry === "compact" ? 8 : 0 }}>
          <MiniQuotaBody
            companies={companies}
            layout={entry.split(":")[0]}
            density={entry.split(":")[1]}
          />
        </div>
      </div>
    ))}
  </div>
  </>
  ),
);
