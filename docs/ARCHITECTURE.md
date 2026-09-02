# Architecture

## Shape

```
┌─────────────────────────────────────────────────────────┐
│ apps/desktop/src            React + TypeScript UI        │
│   App · Overview · Sessions · About                      │
└───────────────▲─────────────────────────┬───────────────┘
                │ invoke()                │ event
┌───────────────┴─────────────────────────▼───────────────┐
│ apps/desktop/src-tauri      Tauri 2 shell                │
│   commands · tray · state · native_app detection         │
└───────────────────────────▲─────────────────────────────┘
                            │
┌───────────────────────────┴─────────────────────────────┐
│ crates/vibebar-desktop-core  Platform-independent core   │
│   paths · model · shared (read + settings) · client_store│
│   credentials · providers · refresh · sessions           │
└───────────┬─────────────────────────────┬───────────────┘
            │                             │
   ┌────────▼─────────┐        ┌──────────▼──────────────┐
   │ agent-session-   │        │ ~/.vibebar              │
   │ core (kit crate) │        │ shared: read; settings  │
   │ index · sessions │        │ client/desktop: r/w     │
   └──────────────────┘        └─────────────────────────┘
```

The split is deliberate: everything that is not about windows, trays, or IPC
lives in the core crate, which has no Tauri dependency and is tested on
macOS, Windows, and Linux without a GUI. The shell is thin enough to read in
one sitting.

## Releasing

Not yet possible: there is no release workflow, no tag, and no signing key.
[RELEASE.md](RELEASE.md) is the design, including the three things that cannot
be changed once a version has reached someone.

## Looking at the mini layouts

`pnpm -C apps/desktop dev`, then `/preview.html`. It renders every ported
mini-window layout side by side against synthetic data, and is not part of the
build — `index.html` is the only entry.

It exists because the layouts are where the tests cannot see the bug. Labels
wrapping onto a second line, bars that stop lining up, a forecast marker
invisible against the fill it sits on: all found there, none by a unit test.

## Where decisions live

| Concern | Module |
| --- | --- |
| Which data root, and the write boundary | `paths.rs` |
| The quota vocabulary and the two naming axes | `model.rs` |
| Reading another client's stores, tolerantly | `shared/` |
| Anything this client persists | `client_store.rs` |
| Finding credentials the CLIs wrote | `credentials/` |
| One provider's endpoint and wire shape | `providers/<name>.rs` |
| Merging live and cached into what the UI shows | `refresh.rs` |
| Presentation preferences | `shared/settings.rs` reads them; `shared/settings_writer.rs` is the only thing here that writes a shared store |
| Public OpenAI-wide/Claude/Google AI/Cursor service status | `status.rs` → in-memory cache → status IPC |
| Local Codex/Claude usage and priced portion | `cost.rs` → in-memory cache → cost IPC |
| Indexed vs scanned sessions | `sessions.rs` |

## Data flow for a refresh

1. `QuotaEngine::refresh` fetches every provider with a live adapter
   (Codex, Claude, Alibaba, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter,
   and Warp today), in sequence, each with a 30-second ceiling.
2. Each success is persisted to `client/desktop/quotas/` and kept in hand —
   a failed write must not lose an observation already obtained.
3. The shared cache is read, with account ids claimed from both the native
   app's stable naming and everything this client has seen.
4. `merge` keeps the newest observation per account id; a fresher cache entry
   beats a stale live one, because recency is the question being asked.
5. `consolidate` collapses each provider to one card whose every window
   carries its newest *believable* reading. This is what turns five cached
   Claude entries — two duplicate, three empty, one from the future — into
   the one card describing the subscription.
6. The view is pushed to the window as an event and rendered into the tray
   title.

## Two sources, one list, honestly labeled

Desktop fetches ten providers and reads twenty-five. A number it fetched and
a number the native app left in the cache are different claims about
freshness, so `QuotaOrigin` travels with every account and the UI marks cached
ones `shared data`. The alternative — showing them identically — would make
Desktop look more capable than it is and hide staleness.

## Sessions: indexed or scanned

If the shared `session_index.sqlite3` exists at a schema this build reads,
Desktop queries it: every harness the index covers, trigram full-text search.
Otherwise it scans Codex and Claude Code logs directly and says so. The
fallback is deliberately narrower rather than a second, competing indexer —
one index, one writer, and Desktop is not that writer yet.

Session paths stay inside the Rust process. List and search results expose an
opaque, CSPRNG-backed `sessionRef` that expires after 15 minutes; overlapping
list/search requests retain each other's unexpired references. Transcript IPC
resolves that capability back to the currently
authorized index/discovery result and rejects stale,
unknown, symlinked, or out-of-root files. A webview-supplied path is never an
authorization decision. The parser receives an already-open, no-follow file
handle rather than reopening a checked pathname.

## What is deliberately absent

- **No writes to shared state.** See [SHARED-STORAGE.md](SHARED-STORAGE.md).
- **No dependency on the native app.** Its presence is detected only to offer
  a link, and never changes what Desktop can do.
- **No credential writing or OAuth refresh.** Three processes share
  `~/.codex/auth.json` and none of them take a lock.
- **No notifications, no cost pipeline, no MCP server, no remote sync.** These
  are parity work, tracked in `HANDOVER.md`, not omissions to be patched in
  ad hoc.

## The popover

Native's primary surface is the menu-bar popover: a transient `NSPopover`
with a tabbed shell — Overview, one page per core company, Misc, Machines —
whose Overview is a two-column waterfall of cards. Desktop draws the same
shell in a second window, `popover`, that the tray's left click toggles.

| Piece | Where | Native counterpart |
| --- | --- | --- |
| Density profiles, widths, heights | `src/popover/theme.ts` | `Theme.Density`, `overviewDensity` — copied number for number, tested |
| Placement | `src/popover/masonry.ts` | `OverviewMasonryPlanner` — summary row, four-quota exhaustive order, cost exhaustive columns, auxiliary shortest-first; `reflow` keeps columns when a card grows |
| Wording | `src/popover/format.ts` | `ResetCountdownFormatter`, `QuotaFreshnessLabel`, `QuotaForecastRow`, `UsagePace`, `ProviderPlanDisplay` |
| Cards | `src/popover/cards.tsx` | `OverviewCostSummaryCard`, `OverviewStatusSummaryCard`, `ProviderQuotaCard` (+ the Google AI and SpaceXAI combined cards), `ProviderBucketRow`, `ForecastQuotaBar`, `PaceMarkerCapsule`, `UpcomingResetsCard` with `ResetLaneView`, `OverviewUsageMixCard` |
| Shell | `src/popover/PopoverRoot.tsx` | `PopoverRoot`, `HeaderView`, `OverviewPageSwitch` |
| Window | `src-tauri/src/popover.rs` | `NSPopover` — decorationless, always on top, placed under the tray icon, hidden on blur, sized to its content, drawn on `NSVisualEffectMaterial.Popover` |

The window is transparent and the page paints nothing behind its cards, so
the system material shows through; that needs `macOSPrivateApi` for WKWebView
to stop painting its own background. The popover's arrow is the one piece of
native chrome this cannot draw.

`VIBEBAR_DEMO_SURFACE=popover` presents it without a tray click, as native's
switch does, for the screenshot scripts. `apps/desktop/preview.html` renders
the same shell on `src/popover/fixture.ts`, the data behind the native
README's Overview screenshot.

### What the shell cannot feed yet

The popover draws what the core serves. These native cards wait on core work:

- **Forecast rows** on shared-cache buckets — native computes them from its
  fill/forecast timelines; the core attaches a forecast only where it has its
  own observations, so cached buckets get the pace row instead.
- **Cost**, and with it the **cost history**, **model ranking** and the two
  **heatmaps** — the core scans JSONL itself and keeps no ledger, so a machine
  whose usage lives in native's ledger reads as no usage here.
- **Uptime percentages** on the status tiles — the core's status model has
  no aggregate uptime.
- **Projects** and **Token Flow** in the usage mix — both need per-request
  attribution the core does not keep.
- **Quota history** — the chart with its brush, which needs the timelines.

## The Workbench

The main window is the native Workbench: a 206 pt sidebar of pages
(Usage Stats, Sessions, Resets, Skills, Settings) and a page header with
the page's status line, the appearance toggle, and refresh.
`src/workbench/porcelain.css` carries the porcelain tokens the native
`WorkbenchPorcelainStyle` defines — window, sidebar, toolbar, field, and
card fills, the 0.5 pt hairline, the `#4E5FE0` accent — and every page
composes from `.wb-card`, `.wb-toolbar`, `.wb-pill`, and `.wb-table`
rather than restating them.

### Usage Stats

The page is the native `UsageStatsPage` composition — filters toolbar,
hero card, trend chart with a brush navigator, five distribution donuts,
breakdown tables — fed by one core query, `usage_stats`, instead of the
webview holding the ledger. `CostEngine` keeps the deduplicated events
after a scan and `usage_stats` scans lazily when nothing is retained yet,
so the page never shows an empty ledger a refresh would have filled.
`crates/vibebar-desktop-core/src/usage_stats.rs` owns the rules that
matter for parity:

- Buckets are local calendar hours, days, and weeks (Monday start). Auto
  picks hour for ranges up to 24 h, day up to 45 d, week beyond, and any
  choice coarsens until the range fits 1,200 buckets — the native
  interactive budget.
- Harness labels are `ToolType::hierarchy().tool` ("Codex", "Claude
  Code", "Gemini Web"); chip groups fold them under their billing
  company in `ToolType` order and list every harness ever ingested, not
  only those in range.
- Model choices cascade from the harness pick; an unset harness list
  means every harness, an empty one means none, matching the native All
  chip that is a switch rather than a shortcut.
- Cost is unset, not zero, when nothing in range carried a price, so the
  hero shows "—" rather than a $0 that reads as free.

`src/workbench/usage/model.ts` holds the page's own rules — presets,
formatting that matches `UsageFormatting`, the chart window floor of two
buckets — and `fixture.ts` renders the page at `/preview.html?surface=usage`
without Tauri. Project attribution is not available in this client, so
the Project Mix card says so instead of drawing an empty ring.

### Sessions

The page is the native `SessionManagerPage`: a filters toolbar (search,
scope, folders, index status, the All chip, company/harness/when/sort/
options menus, delete controls), a resizable split of the session list
(300–620 pt) and the transcript (≥ 420 pt), 13 pt row cards, and message
cards with the native role bar, timestamp, copy, and the 3,000-character
collapse. One core call, `session_listing`, takes the search text,
provider and harness filters, a time bound, and an offset page, and
returns the listing with per-harness counts for the menu. Folder
filters, sort order, and project grouping apply to the loaded rows in
`src/workbench/sessions/model.ts`, which also owns the 80-message
transcript window, find and highlight, and the prompt outline.

Three things this client says plainly instead of pretending:

- Search scope. The shared index searches every scope it holds; the
  Scope menu keeps the native vocabulary and says that per-scope
  narrowing waits on a role-aware index.
- Deleting session logs. The store is the native app's to write
  (`AGENTS.md` § storage boundary), so Select/Delete are present but
  refuse with the reason rather than reaching into `~/.codex` or
  `~/.claude`.
- Opening a terminal. `open_in_terminal` runs only a line shaped like a
  resume command — one known CLI, plain arguments, no shell operators —
  through `osascript` on macOS; elsewhere it says to copy the command.

### Resets

The page is the native `ResetsPage`: the seven-day Refill Horizon lane,
one card per SubProvider cycle on an adaptive 270 pt grid, the reset
calendar with its sub-daily lane, and the 320 pt Run-out Risk column.
`src/workbench/resets/model.ts` carries the rules — buckets grouped by
SubProvider and group title in first-seen order and headlined by the
longest window; risk rows for buckets whose forecast is uneasy or that
sit at 15% or less, soonest run-out first, badged OUT / RISK / WATCH /
LOW by the verdict rather than the raw remaining; calendar entries from
the completed cycles the forecast store keeps (`quota_cycles`, one call
per bucket, bounded) plus every scheduled reset in the month. `now` is
rounded down to five minutes the way the native page does, so the cards
do not re-lay out every second.

The native card also draws the remaining-percent curve across the
current cycle from the fill timeline's observations; this client's
forecast store keeps cycle summaries, not samples, so the card omits the
curve rather than sketching one.

### Skills

The page is the native `SkillsManagerPage`: the toolbar card with the
filter field and the five actions, the per-harness count capsules with
the wiring explainer, and one row per skill — name, source badge, health
badge, description, and an activation circle per managed harness. The
data is `skills_inventory`, the read-only scan of `~/.agents/skills` and
the six projection roots. `src/workbench/skills/model.ts` derives what a
circle can honestly say from that: a link means projected, a harness
that scans the shared root sees every skill without a link, and a
harness with its own per-skill switch is shown as projected with the
switch unread — this client does not parse `config.toml` or
`settings.json` to learn it.

Install, update, import, backups, and discovery run in the native app;
the controls are present and say so. `reveal_path` opens a skill in the
file manager and accepts only a path inside `~/.agents/skills`.

### Settings

The page is the native `SettingsView`: a searchable 236 pt sidebar of
grouped rows — Settings, Core Providers, Misc Providers — and titled
section cards for the selected one, in the native order and with the
native copy. What each control can do here follows the storage
contract: the four keys `settings-write-v1.md` lets this client write
(percent shown, refresh cadence, percent colour basis, update channel)
are live; everything else the shared file holds — menu bar layout and
fields, popover density, mini windows, provider visibility, cost data
retention and privacy mode, misc provider instances — is shown read-only
with the note that it is set in the native app. `PresentationSettings`
now carries the menu bar item and the misc provider instances for that.

Controls that are this client's own: launch at login through
`tauri-plugin-autostart`, the update check and install, rescanning cost
logs, the effective price table (`pricing_effective`), and connection
health per core provider, which reports what a quota read can show —
CLI, OAuth, and credential-file routes — and says cookie routes are not
used here. Remote probes, the MCP socket, the menu bar health monitor,
WebView login, and cookie import remain the native app's; each section
says so rather than presenting a control that would do nothing.
