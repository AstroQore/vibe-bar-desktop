/** Synthetic sessions and a transcript for `/preview.html?surface=sessions`. */
import type { SessionListing, SessionRow, TranscriptPage } from "../../api";
import { FIXTURE_NOW } from "../../popover/fixture";

const H = 3_600;
function row(
  i: number,
  provider: string,
  harness: string,
  model: string,
  title: string,
  project: string,
  excerpt: string,
  ago: number,
  messages: number,
): SessionRow {
  return {
    rowId: i,
    provider,
    harness,
    sessionId: `fc1a1239-7ea6-5513-9d60-66065c19c5${String(i).padStart(2, "0")}`,
    model,
    title,
    projectDir: `/Users/example/Coding/${project}`,
    lastActiveAt: FIXTURE_NOW - ago,
    sessionRef: `fixture-${i}`,
    messageCount: messages,
    resumeCommand: provider === "codex" ? `codex resume fc1a1239-7ea6-5513-9d60-66065c19c5${String(i).padStart(2, "0")}` : provider === "claude" ? `claude --resume fc1a1239-7ea6-5513-9d60-66065c19c5${String(i).padStart(2, "0")}` : undefined,
    excerpt,
  };
}

export const FIXTURE_SESSIONS: SessionListing = {
  source: "indexed",
  indexedTotal: 12,
  harnessCounts: [
    { harness: "Claude Code", provider: "claude", count: 5 },
    { harness: "Codex", provider: "codex", count: 4 },
    { harness: "Gemini CLI", provider: "gemini", count: 2 },
    { harness: "Cursor", provider: "cursor", count: 1 },
  ],
  rows: [
    row(1, "claude", "Claude Code", "claude-opus-5", "The harness filter chips on the Usage Stats page scroll off the right edge at the default window width", "vibe-bar", "Done. Harnesses with zero requests in the selected range are filtered out of the chip row (they stay in the menu).", H, 20),
    row(2, "codex", "Codex", "gpt-5.6-sol", "Backfill the events table from the archived parquet files for July, in 1-day batches", "ingest-pipeline", "Backfill the events table from the archived parquet files for July, in 1-day batches, and stop on the first failure.", 3 * H, 12),
    row(3, "cursor", "Cursor", "grok-4.6", "Checkout step indicator loses state on refresh", "storefront-web", "Persisted the checkout step in the URL hash and restored it on load; added a Playwright test for the reload path.", 4 * H, 5),
    row(4, "claude", "Claude Code", "claude-sonnet-5", "Reconcile the on-call rotation in rotation.yaml with the calendar export in exports/", "ops-runbook", "Two weeks disagree: - Aug 10–16: `rotation.yaml` names the platform team, the calendar has the data team.", 7 * H, 6),
    row(5, "codex", "Codex", "gpt-5.6-sol", "Rename the `brand-*` color tokens to `accent-*` across the token JSON, the Tailwind config, and the docs", "design-tokens", "Rename the `brand-*` color tokens to `accent-*` across the token JSON, the Tailwind config, and the docs.", 8 * H, 9),
    row(6, "codex", "Codex", "gpt-5.6-sol", "Audit the dependency manifest for packages that have not been updated upstream in over a year", "mobile-client", "Audit the dependency manifest for packages that have not been updated upstream in over a year and list replacements.", 11 * H, 7),
    row(7, "claude", "Claude Code", "claude-opus-5", "Turn the incident notes from last Tuesday into a postmortem using the template in docs/", "ops-runbook", "Drafted `incidents/2026-08-11-postmortem.md`. Timeline, impact and detection are filled from the notes.", 26 * H, 6),
    row(8, "claude", "Claude Code", "claude-opus-5", "Cart totals are off by a cent when a percentage discount applies to three or more items", "storefront-web", "Fixed and covered with a property test over 1–12 items and every discount from 1–50%. Totals now round once.", 30 * H, 14),
    row(9, "gemini", "Gemini CLI", "gemini-2.5-pro", "Summarise the last 200 support tickets by root cause", "support-tools", "Grouped 200 tickets into 9 root causes; the top three cover 71%.", 2 * 24 * H, 4),
    row(10, "codex", "Codex", "gpt-5.6-sol", "Add a retry with jitter to the webhook sender", "ingest-pipeline", "Added exponential backoff with full jitter, capped at 5 attempts; wrote a table-driven test for the schedule.", 3 * 24 * H, 8),
    row(11, "gemini", "Gemini CLI", "gemini-2.5-pro", "Draft release notes for 1.6.0 from the merged PR titles", "vibe-bar", "Release notes drafted with sections for Workbench, popover, and data.", 4 * 24 * H, 3),
    row(12, "claude", "Claude Code", "claude-sonnet-5", "Explain the failing flaky test in CI and propose a fix", "mobile-client", "The test depends on wall-clock time crossing a minute boundary; froze the clock in the fixture.", 6 * 24 * H, 11),
  ],
};

const stamp = (offsetSeconds: number) => new Date((FIXTURE_NOW - H + offsetSeconds) * 1000).toISOString();

export const FIXTURE_TRANSCRIPT: TranscriptPage = {
  offset: 0,
  totalMessages: 20,
  truncated: false,
  messages: [
    { role: "user", timestamp: stamp(0), text: "The harness filter chips on the Usage Stats page scroll off the right edge at the default window width. Can you make the row wrap instead, and keep the selected chip visible?" },
    { role: "assistant", timestamp: stamp(46), text: "I'll look at how the chip row is laid out first — it is an HStack inside a horizontal ScrollView in `UsageFiltersBar.swift` — and then decide between wrapping and a trailing overflow menu." },
    { role: "assistant", timestamp: stamp(72), text: '[Tool: Read]\n{"file_path":"Sources/VibeBarApp/Views/Workbench/UsageFiltersBar.swift"}' },
    { role: "tool", timestamp: stamp(74), text: "(Read completed)" },
    { role: "assistant", timestamp: stamp(146), text: '[Tool: Grep]\n{"pattern":"harnessChips( Sources/VibeBarApp"}' },
    { role: "tool", timestamp: stamp(148), text: "(Grep completed)" },
    { role: "assistant", timestamp: stamp(180), text: "Wrapping is the better fit: the chips are a filter, not a tab bar, so two rows read fine and nothing hides. I'll switch the container to a flow layout and keep the All chip pinned first." },
    { role: "user", timestamp: stamp(400), text: "Sounds good. Also hide harnesses that have zero requests in the selected range — they add noise." },
    { role: "assistant", timestamp: stamp(430), text: "Done. Harnesses with zero requests in the selected range are filtered out of the chip row (they stay in the menu so a hidden harness can still be re-enabled)." },
    { role: "system", timestamp: stamp(431), text: "Auto Review merged: no findings." },
    ...Array.from({ length: 10 }, (_, i) => ({
      role: (i % 2 === 0 ? "user" : "assistant") as "user" | "assistant",
      timestamp: stamp(500 + i * 40),
      text: i % 2 === 0 ? `Follow-up ${i / 2 + 1}: check the ${["compact", "regular", "spacious"][i % 3]} density too.` : "Checked — the row wraps at every density and the selected chip stays in view.",
    })),
  ],
};
