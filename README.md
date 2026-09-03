<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128@2x.png" alt="Vibe Bar Desktop" width="128">
</p>

<h1 align="center">Vibe Bar Desktop</h1>

<p align="center">
  <strong>The same Vibe Bar, on Windows, Linux and macOS.</strong><br>
  <sub>AI subscription quotas, agent sessions, spend and skills — in a tray icon, a small window and a Workbench, on one data root shared with the native app.</sub>
</p>

<p align="center">
  <a href="https://github.com/AstroQore/vibe-bar-desktop/actions/workflows/ci.yml"><img src="https://github.com/AstroQore/vibe-bar-desktop/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/AstroQore/vibe-bar-desktop/releases"><img src="https://img.shields.io/github/v/release/AstroQore/vibe-bar-desktop?display_name=tag&sort=semver&include_prereleases" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-000000?logo=tauri&logoColor=white" alt="macOS, Windows, Linux">
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Rust-stable-000000?logo=rust" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--only-blue" alt="AGPL-3.0-only"></a>
</p>

<p align="center">
  <a href="https://github.com/AstroQore/vibe-bar-desktop/releases"><strong>Download</strong></a>
  · <a href="#what-it-does">What it does</a>
  · <a href="#the-workbench">The Workbench</a>
  · <a href="#feature-parity">Feature parity</a>
  · <a href="#build-from-source">Build from source</a>
  · <a href="#agents-mcp">Agents (MCP)</a>
  · <a href="README.zh-CN.md">中文</a>
</p>

Vibe Bar Desktop is the cross-platform client of
[Vibe Bar](https://github.com/AstroQore/vibe-bar), the local capacity control
plane for people who run coding agents all day. It shows how much of each AI
subscription is left and whether it will last until the reset, what the agents
on this machine are spending, which sessions and skills live here, and it
answers the same questions to your agents over MCP.

It is one product with two client implementations. The macOS app is native
AppKit and SwiftUI; Desktop is Tauri 2, Rust and React, and runs on Windows
and Linux as well. On a Mac that has both, they read one `~/.vibebar` — one
set of provider accounts, one set of settings, no second copy to maintain —
and Desktop runs standalone on a machine that has never had the native app.

> **Preview (0.x).** Desktop is being built up to the native app and carries
> its own `0.x` version until it gets there; the [parity table](#feature-parity)
> is the honest map of what is and is not ported. At parity both clients adopt
> one shared `MAJOR.MINOR` and every feature release ships from both
> repositories together.

![The Overview: cost and status at the top, one quota card per provider below, each bar carrying its forecast](docs/screenshots/popover-overview-light.png)

<details>
<summary>The same Overview in the dark appearance</summary>

![The Overview in the dark appearance](docs/screenshots/popover-overview.png)

</details>

## What it does

| The question | Desktop's answer |
| --- | --- |
| **How much is left, and will it last?** | Every quota card carries its reset countdown and a personal forecast built from the observations recorded here: run-out or surplus, with a verdict that admits when it is still learning. Nothing is manufactured — a forecast appears only where enough cycles support one. |
| **Which of these numbers is fresh?** | Codex, Claude and Grok are fetched with the credentials their CLIs already keep, Cursor with the session Cursor.app keeps, AntiGravity from the language server running here, eight more plans with an explicit key or their own CLI, and the remaining providers are read from the shared cache and labelled `shared data`. The UI never overstates how fresh a number is. |
| **Where did the work go?** | The Workbench prices the Codex, Claude Code and Gemini CLI session logs on this machine into a usage view by harness, by model and by the company that bills for it, and indexes every local agent session for full-text search, reading and one-click resume. |
| **Can my agents use this context?** | The same binary, launched with `--mcp-stdio`, answers quota, session, status, pricing and cost questions over JSON-RPC on stdin/stdout — no port, no socket, no credentials. |

## Quota, one page per company

Each company has a page: its plans and windows, the reset history strip, the
forecast explained in words, and that provider's public service status.
Misc providers — the coding and token plans with their own dashboards — and
remote machines have pages of their own.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-openai-light.png" alt="The OpenAI page: ChatGPT Agentic and Codex Spark windows with their reset history and forecast"><br><sub><strong>OpenAI</strong> — ChatGPT Agentic and Spark, each window with its forecast</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-anthropic-light.png" alt="The Anthropic page: 5 Hours, Weekly and Fable windows with their forecasts"><br><sub><strong>Anthropic</strong> — 5 Hours, Weekly and the per-model windows</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-google-light.png" alt="The Google AI page: Gemini Web and AntiGravity quotas"><br><sub><strong>Google AI</strong> — Gemini Web and AntiGravity</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-spacexai-light.png" alt="The SpaceXAI page: Grok, Cursor and Grok Bot quotas"><br><sub><strong>SpaceXAI</strong> — Grok, Cursor and Grok Bot</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-misc-light.png" alt="The Misc providers page: coding and token plans, each with its quota and reset"><br><sub><strong>Misc providers</strong> — coding and token plans with their own dashboards</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-machines-light.png" alt="The Machines page, explaining end-to-end encrypted remote usage"><br><sub><strong>Machines</strong> — remote usage, end-to-end encrypted to this machine</sub></td>
  </tr>
</table>

<details>
<summary>The company pages in the dark appearance</summary>

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-openai.png" alt="The OpenAI page in the dark appearance"></td>
    <td width="50%"><img src="docs/screenshots/popover-anthropic.png" alt="The Anthropic page in the dark appearance"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-google.png" alt="The Google AI page in the dark appearance"></td>
    <td width="50%"><img src="docs/screenshots/popover-spacexai.png" alt="The SpaceXAI page in the dark appearance"></td>
  </tr>
</table>

</details>

## Mini window

A small always-available window with the fields you chose, in any of the
seven layouts the native app has: regular, compact, ledger, tile, focus, rail,
and strip in its roomy, two-line and narrow forms. The window fits what it
draws and follows the shared `miniWindow` settings, so a layout chosen on one
client is the layout on the other.

<table>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-regular-light.png" alt="The regular mini layout: quota rings"><br><sub><strong>Regular</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-compact-light.png" alt="The compact mini layout"><br><sub><strong>Compact</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-ledger-light.png" alt="The ledger mini layout: one row per field"><br><sub><strong>Ledger</strong></sub></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-tile-light.png" alt="The tile mini layout"><br><sub><strong>Tile</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-focus-light.png" alt="The focus mini layout: one field, large"><br><sub><strong>Focus</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-rail-light.png" alt="The rail mini layout: a lane with ticks and markers"><br><sub><strong>Rail</strong></sub></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-strip-roomy-light.png" alt="The strip mini layout, roomy"><br><sub><strong>Strip</strong> — roomy</sub></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-twoLine-light.png" alt="The strip mini layout, two lines"><br><sub><strong>Strip</strong> — two lines</sub></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-narrow-light.png" alt="The strip mini layout, narrow"><br><sub><strong>Strip</strong> — narrow</sub></td>
  </tr>
</table>

<details>
<summary>The mini layouts in the dark appearance</summary>

<table>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-regular.png" alt="Regular, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-compact.png" alt="Compact, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-ledger.png" alt="Ledger, dark"></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-tile.png" alt="Tile, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-focus.png" alt="Focus, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-rail.png" alt="Rail, dark"></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-strip-roomy.png" alt="Strip roomy, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-twoLine.png" alt="Strip two lines, dark"></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-narrow.png" alt="Strip narrow, dark"></td>
  </tr>
</table>

</details>

## The Workbench

A larger window for the questions a glance cannot answer. Five pages, one
[design language](docs/DESIGN.md) — the same porcelain the native Workbench
is built from, with its tokens, type ramp, radii and provider accents carried
over exactly rather than approximated.

### Usage Stats

What the agents on this machine spent, priced locally from their own session
logs: hero cards for the period, a trend chart, distribution donuts by harness,
model and billing company, and a breakdown table. The harness filters are the
shared pill; the period and grouping are one control each.

![Usage Stats: hero cards, trend chart, distribution and breakdown](docs/screenshots/workbench-usage-light.png)

<details>
<summary>Usage Stats in the dark appearance</summary>

![Usage Stats in the dark appearance](docs/screenshots/workbench-usage.png)

</details>

### Sessions

Every local agent session, searchable in full text with a scope of your
choosing, filtered by folder, company, harness and time. A transcript opens beside the list with
tool calls and results folded, find-in-transcript with a pager, and a resume
command for the harness that made it. Sessions can be deleted from here —
sidecars first, never through a symlink, only below the roots the session kit
recognises.

![Sessions: the filter toolbar, the session list and an open transcript](docs/screenshots/workbench-sessions-light.png)

<details>
<summary>Sessions in the dark appearance</summary>

![Sessions in the dark appearance](docs/screenshots/workbench-sessions.png)

</details>

### Resets

When capacity comes back. The refill horizon shows the next seven days as
columns, one card per window carries its forecast, the calendar lays the
cycles out by day, and the run-out risk list ranks the windows the forecast
says will not last.

![Resets: refill horizon, cycle cards, the reset calendar and run-out risk](docs/screenshots/workbench-resets-light.png)

<details>
<summary>Resets in the dark appearance</summary>

![Resets in the dark appearance](docs/screenshots/workbench-resets.png)

</details>

### Skills

One skill library under `~/.agents/skills`, projected into six agent CLIs.
The page shows every skill with a slot per app — linked, copied, foreign or
missing — and lets you toggle projections, install a skill from a folder,
record one that is already there, uninstall with a snapshot first, and
restore from the backups. The native sync engine's rules are applied verbatim:
a name is one safe path segment, every write sits under an allowed root,
nothing is deleted through a symlink.

![Skills: the library with a projection slot per app](docs/screenshots/workbench-skills-light.png)

<details>
<summary>Skills in the dark appearance</summary>

![Skills in the dark appearance](docs/screenshots/workbench-skills.png)

</details>

## Settings that stay shared

Settings are read from, and the ones this client presents written back to,
the same `settings.json` the native app uses — under a lock, putting back only
the keys you changed and keeping every key this build does not know. When the
native app replaces a choice made here, the page says so. On first run a
short assistant walks through subscriptions, browser cookies, other plans,
model pricing and launch at login, and marks itself complete in the shared
settings so the native app will not ask again.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-system-light.png" alt="Settings › System: refresh cadence, update channel, launch at login, what this client writes"><br><sub><strong>System</strong> — refresh, updates, launch at login, and what this client writes where</sub></td>
    <td width="50%"><img src="docs/screenshots/settings-menubar-light.png" alt="Settings › Menu bar: the fields the tray shows and their labels"><br><sub><strong>Menu bar</strong> — the fields the tray shows, in your words</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-costData-light.png" alt="Settings › Cost data: the privacy switch and the scan scope"><br><sub><strong>Cost data</strong> — the privacy switch that stops every scan</sub></td>
    <td width="50%"><img src="docs/screenshots/onboarding-welcome-light.png" alt="The first-run assistant over the Workbench"><br><sub><strong>First run</strong> — seven short steps, shared completion flag</sub></td>
  </tr>
</table>

<details>
<summary>Settings in the dark appearance</summary>

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-system.png" alt="System settings, dark"></td>
    <td width="50%"><img src="docs/screenshots/settings-menuBarHealth.png" alt="Menu Bar Health settings, dark"></td>
  </tr>
</table>

</details>

## Agents (MCP)

Run the Desktop binary with `--mcp-stdio` and it serves `quota.get`,
`sessions.list`, `sessions.search`, `status.get`, `pricing.effective` and
`cost.snapshot` over JSON-RPC on stdin/stdout. This mode never refreshes
providers, scans usage, writes configuration, or connects to the native app;
it answers from what the last run recorded. Session calls accept the native
provider and harness filters, and listing supports RFC 3339 `since`, `offset`
and a bounded `limit`. The Unix socket in your home directory belongs to the
native app.

## What Desktop reads, and the five things it writes

Desktop **depends on no part of the native app**: not its process, not its
MCP socket, not its binaries. Everything it shows is read from files already
on the machine — the CLIs' own credentials and session logs, and the shared
`~/.vibebar` root.

| Surface | Quota and status | Cost and activity |
| --- | --- | --- |
| ChatGPT / Codex | Codex subscription windows, OpenAI status | `~/.codex/sessions/**/*.jsonl` |
| Claude Code | 5 Hours, Weekly, per-model weekly, Anthropic status | `~/.claude/projects/**/*.jsonl` |
| Gemini + AntiGravity | Read from the shared cache | Gemini CLI session logs |
| Grok + Cursor | Read from the shared cache; Cursor status | — |
| Coding and token plans | Alibaba, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter and Warp with an explicit key or their CLI; the rest from the shared cache | — |

Vibe Bar data belongs to the person, not to one client, and a second client
writing into a store the first one owns is how data gets lost. So Desktop
writes exactly five things into the shared root, each through one documented
writer with the native app's rules:

1. **`settings.json`** — under an advisory lock, a merge that puts back only
   the keys this client changed and keeps every key it does not know
   ([the contract](docs/contracts/settings-write-v1.md)).
2. **The quota cache** — its own fresh observations, in the file layout the
   native app reads.
3. **The Control Center allow-list repair** — the same script the native app
   runs when macOS 26 hides a menu-bar icon.
4. **Whole-session deletion** — through the session kit's deleter, only at
   your request, only below the roots it recognises, never through a symlink.
5. **The skill library** — `~/.agents/skills`, the managed app directories,
   the registry and its backups, through the skills service and nothing else.

Everything else — the session index, the usage ledger, cost history — is read
here and written by the native app until it too has a writer with rules like
those. An unreadable or unknown-version store degrades to "not available"
with an explanation; Desktop never repairs, migrates or rebuilds another
client's data. The full reasoning is in
[docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md) and the rules in
[AGENTS.md](AGENTS.md).

## About the screenshots

Every picture on this page is this app's real UI, rendered in a browser from
the same React code the Tauri window loads, over a flat backdrop, in both
appearances. The data is the app's built-in fixtures — the numbers one
maintainer's usage shaped, with every account, path, machine and session
replaced or written for the occasion — so nothing here identifies a person,
and no refresh left the machine.
[`apps/desktop/scripts/capture-screenshots.mjs`](apps/desktop/scripts/capture-screenshots.mjs)
produces the whole set (`pnpm screenshots`), so the gallery is regenerated,
not retouched. The native app's screenshots come from its own demo mode; the
two galleries are the same fixtures seen through the two clients.

## Feature parity

One product, two clients. The binding rule is the **minor** version: the same
`MAJOR.MINOR` means the same features. Patch versions diverge freely — each
client fixes its own bugs at its own pace — and build numbers are always
independent.

Two things are exempt from parity, and only these two:

- **Bug fixes.**
- **Features with no equivalent on another platform at all.** Needing a
  different implementation is not an exemption: Keychain becomes DPAPI or
  libsecret, Sparkle becomes the Tauri updater, `SMAppService` becomes each
  platform's autostart. Those are the same feature, built differently.

**This table lists only where the two differ.** Anything not here is at
parity — the quota hierarchy, tray fields with your own labels, session search
and transcripts, session deletion, the mini-window layouts, the reset
calendar, the first-run assistant, in-app updates, launch at login. A new
feature on either side must appear here until it lands on both.

**Until then.** Desktop is `0.x` and this contract is not yet in force: the
native app ships feature minors freely while Desktop closes the table below.
When Desktop reaches parity with the native minor of the day, both ship the
next minor together as the first joint release — and from that point neither
client ships a feature minor the other cannot.

Legend: ● full · ◐ partial · ○ not yet · — exempt

| Feature | macOS native | Desktop | Note |
| --- | :---: | :---: | --- |
| **Quota** |
| Live provider fetch | ● 25 | ◐ 13 | Codex, Claude, Cursor, Grok, AntiGravity, Alibaba, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter, Warp. The rest is read from the shared cache and labelled as such |
| Browser-cookie providers | ● | ○ | Gemini and the cookie-slot plans need a cookie reader; Windows blocks third-party cookie reads, so it will be an explicit import there |
| Observation and forecast history | ● | ◐ | Desktop records observations and draws the reset-history strip; the quota history chart with its brush is not ported |
| Service status sources | ● 5 | ● 4 | OpenAI, Anthropic, Google, Cursor; xAI's page is scraped and not ported |
| **Menu bar / tray** |
| Rich-text and two-row title | ● | — | Windows and Linux trays have no title at all, only an icon; the macOS tray shows one line |
| Field editor with style scopes | ● | ○ | Fields and labels are read from the shared settings and edited on the Menu bar page without the style scopes |
| Merged group windows | ● | ○ | Native can fold a group's 5-hour and weekly windows into one menu-bar entry — the group named once, the percentages sharing it, each keeping its own colour. The grouping rules live in `VibeBarCore`, so this is a contract to mirror rather than a look to imitate |
| Menu-bar composer | ● | ○ | Native is moving to a freely arranged menu bar: pick a template, then place logos, free text and any available quota as elements, each with its own or a quota-derived colour, with the current fixed layouts kept as the default mode |
| Control Center allow-list watchdog | ● | ◐ | Desktop runs the same repair script; the watchdog that notices the icon is gone is native-only |
| **Main window** |
| Provider detail pages | ● 4 | ◐ | Each company has its page with quota, forecast explanation, reset history and status; native's also carry that provider's cost cards and history charts |
| Arrangeable module waterfall | ● 11 | ◐ | The Overview draws the modules in the shared order; arranging them is native-only |
| Layout editor with presets | ● | ○ | |
| **Mini window** |
| Multiple independent windows | ● | ○ | One mini window, seven layouts |
| Translucent surface | ● Liquid Glass | ◐ | On macOS the main window, popover and mini window carry real `NSVisualEffect` materials — sidebar and popover, deliberately the platform's own rather than a copy of Liquid Glass. Windows and Linux draw them opaque for now |
| **Workbench** |
| Reset-history comparison | ● | ○ | Native's cross-quota card: rows grouped company → SubProvider → bucket with two-level labels, a Cycles / Time axis toggle stored in the shared `resetHistoryCompareAxis`, a 4 / 8 / 12 / All picker whose default follows the card's width, and a bar drawing what was left at each reset |
| Skills: install, import, discover, backups | ● | ◐ | Install from a folder, import, projections, uninstall and backups are here; repository install, discover and the harness activation patches stay native for now |
| Session hand-off to a terminal | ● | ◐ | Desktop copies the resume command; native opens Terminal with it |
| **Cost and usage** |
| Local usage scan | ● 7 harnesses | ◐ 3 | Codex, Claude Code, Gemini CLI. Counts harnesses with a local scanner: Cursor's usage comes from dashboard events and Grok Bot has no usage source at all, so neither is a local scan on either side |
| Per-request ledger, multi-source pricing, history | ● | ○ | Desktop keeps a priced aggregate under `client/desktop/` and a static price table; no shared ledger or history is written |
| **Settings** |
| Writable | ● | ◐ | The keys Desktop's own Settings presents, through the cross-client write contract — the whitelist in `shared::settings_writer::WRITABLE_KEYS` is the boundary. Provider credentials and the layout editor are not among them |
| Provider credential panes | ● 25 | ○ | API-key adapters read the process environment; nothing is persisted |
| **Platform** |
| Localization | ◐ | ○ | Both clients will read one catalog, [`AstroQore/vibe-bar-i18n`](https://github.com/AstroQore/vibe-bar-i18n) — Swift package for native, npm package here, Simplified Chinese first. Neither client consumes it yet |
| MCP tools | ● 12 | ◐ 6 | Read-only subset over stdio; the Unix socket is native's |
| Remote probe sync | ● | ○ | The Machines page explains the model; no relay client yet |
| App Sandbox | ○ by design | ○ for now | Neither ships sandboxed. Native **cannot**: reading browser cookies, probing AntiGravity and driving Terminal are all blocked inside it. Desktop has no reason yet, and would lose the same cookie reads |
| Windows and Linux | — | ◐ | The core crate is tested on all three on every pull request and the credential and scan paths are portable; the GUI has had its end-to-end pass on macOS only. Both are release targets — see [docs/RELEASE.md](docs/RELEASE.md) |

[HANDOVER.md](HANDOVER.md) is the map from here to parity: what the native
app does that Desktop does not, in what order to close the gap, and the bugs
this preview found in itself along the way.

## Design language

The Workbench, the popover and the mini window are drawn from one spec,
[docs/DESIGN.md](docs/DESIGN.md): the native Workbench's porcelain — its
window and sidebar tints, the 0.5px hairline, the accent, the provider colours,
the type ramp, the radii and the single 26px control height — lifted from the
Swift source rather than eyeballed, with the shared pieces (pills, capsules,
segmented controls, code blocks, switches) implemented once in
[`porcelain.css`](apps/desktop/src/workbench/porcelain.css). A page that needs
a new piece extends the spec first.

## Build from source

Requires Rust (stable), Node 22.13+, pnpm, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your
platform.

```sh
cd apps/desktop && pnpm install
pnpm tauri dev      # run
pnpm tauri build    # package
```

Verification, the same four steps CI runs:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm test && pnpm build
```

Point the app at a synthetic data root with `VIBEBAR_DEMO_HOME=<dir>` — the
same environment variable the native app's demo mode uses. In demo mode
Desktop makes no network requests, reads no credentials and refuses every
write that would leave that directory.

Two more things the repository can do without starting the GUI:

```sh
cargo run -p vibebar-desktop-core --example inspect -- <data-root>   # what Desktop can see from a data root
cd apps/desktop && pnpm screenshots                                  # regenerate docs/screenshots/
```

### Updates

Releases are built by [`release.yml`](.github/workflows/release.yml) from a
tag — `vX.Y.Z` on the Main channel, `vX.Y.Z-dev.N` on Dev — and the updater
feed lives on the `updates` branch, one entry per channel. The app checks once
a day, reads the channel from the shared `updateChannel` setting, and offers
the update from the tray. [docs/RELEASE.md](docs/RELEASE.md) has the whole
pipeline.

## Layout

```
crates/vibebar-desktop-core/   Platform-independent core: shared-data readers
                               and writers, provider adapters, refresh
                               orchestration, the usage scanner, the skills
                               service, the MCP server. No Tauri, no GUI,
                               tested on all three platforms.
apps/desktop/                  React + TypeScript UI: popover, mini window,
                               Workbench, settings, the first-run assistant
apps/desktop/src-tauri/        Tauri shell: windows, tray, IPC, the updater
docs/                          Architecture, the shared-storage rules, the
                               design language, the release pipeline, and the
                               contracts for cross-client writes
```

Session reading and deletion come from
[`agent-session-core`](https://github.com/AstroQore/agent-session-kit), the
Rust lane of `agent-session-kit` — the same kit the native app's Swift
implementation uses, so both clients handle sessions by one set of rules.

## Acknowledgements

Desktop is a port: nearly every rule in it — the provider endpoints, the
bucket shapes, the storage layout, the sync engine's fences — was read out of
[Vibe Bar](https://github.com/AstroQore/vibe-bar)'s Swift source and
reimplemented here so both clients behave the same way. Where that source
credits someone, so does this one:

- [CodexBar](https://github.com/steipete/CodexBar) is the technical reference
  behind the menu-bar quota experience the native app built, and several
  behaviours this client ports reach it through that: the Cursor endpoint set
  and the gate on its legacy request-plan fallback, the shape of Grok's
  billing response, and the idea of discovering AntiGravity from the language
  server running on the machine.
- [CC Switch](https://github.com/farion1231/cc-switch) informed the unified
  skills workflow the Skills manager reconciles, and remains the
  interoperability reference for existing cross-agent skill layouts.
- [ccusage](https://github.com/ccusage/ccusage) informed the local
  session-cost parsing and pricing semantics this client's scanner follows.
- [LiteLLM](https://github.com/BerriAI/litellm),
  [models.dev](https://github.com/anomalyco/models.dev) and
  [Portkey Models](https://github.com/Portkey-AI/models) maintain the public
  model-price catalogs the rates in this client's static table trace back to,
  by way of the native app's merged catalog and the small Vibe Bar supplement
  in [vibebar-model-pricing](https://github.com/AstroQore/vibebar-model-pricing).
  Desktop does not refresh or merge those catalogs yet.

Desktop is built on [Tauri 2](https://github.com/tauri-apps/tauri) and its
single-instance, autostart, updater, dialog and opener plugins, with
[window-vibrancy](https://github.com/tauri-apps/window-vibrancy) for the
translucent surfaces, [rusqlite](https://github.com/rusqlite/rusqlite) and
[reqwest](https://github.com/seanmonstar/reqwest) underneath, and
[React](https://github.com/facebook/react) with
[Vite](https://github.com/vitejs/vite) in front. Sessions come from
[agent-session-kit](https://github.com/AstroQore/agent-session-kit), which is
ours and shared with the native app. Every dependency and its version is in
`Cargo.lock` and `apps/desktop/pnpm-lock.yaml`; each carries its own licence
in its own repository.

These projects are independent from Vibe Bar. Acknowledgement does not imply
affiliation or endorsement.

## License

AGPL-3.0-only, same as the native app.

## Star History

<p align="center">
  <a href="https://star-history.com/#AstroQore/vibe-bar-desktop&Date">
    <img src="https://api.star-history.com/svg?repos=AstroQore/vibe-bar-desktop&type=Date" alt="Star History Chart">
  </a>
</p>
