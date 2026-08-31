# Vibe Bar Desktop

The cross-platform client of [Vibe Bar](https://github.com/AstroQore/vibe-bar) —
AI subscription quota and local agent sessions, in a tray icon and a small
window, on macOS, Windows, and Linux.

> **Preview (0.x).** This is one product with two client implementations. The
> macOS native app is the complete one today; Desktop is being built up to it.
> Until it reaches feature parity, Desktop carries its own `0.x` version and
> does **not** share the native app's release train. At parity it adopts the
> shared `MAJOR.MINOR.PATCH` and every feature release ships from both
> repositories together.

## What it does today

- **Quota.** Fetches Codex and Claude with their CLI credentials, plus Alibaba
  Coding Plan, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter, and Warp with
  explicit credentials or their official CLI. The other 15 providers are read
  from the shared data root and labeled `shared data`, so the UI never
  overstates how fresh a number is.
- **Upcoming resets.** Sorts provider-declared reset times from the current
  quota view, while keeping expired or future-dated observations visibly
  separate, and carries each one's forecast. Nothing is manufactured: a
  forecast appears only where enough observations have been recorded to
  support one, and says so when they have not.
- **Tray.** One line of the fields you picked, with your labels and your
  remaining-vs-used preference — read from the same settings the macOS menu
  bar uses.
- **Sessions.** Search and read local agent sessions. Uses the shared session
  index when one exists (every harness it covers, full-text search), and falls
  back to scanning Codex and Claude Code logs directly when it does not.
- **Presentation settings.** Applies the shared used/remaining mode, provider
  visibility/order, plan labels, and menu-bar field labels. The Settings page
  shows the effective values, and the three it presents — percent shown,
  refresh interval, menu-bar colour basis — are editable and save to the shared
  `settings.json` under a lock, keeping every key this build does not know.
  When the native app replaces a choice made here, the page says so.
- **Service status.** Reads cached native status immediately, then refreshes
  OpenAI-wide, Claude, Google AI, and Cursor status from public feeds without credentials.
  A fresh Desktop last-good snapshot is private to `client/desktop/`; shared
  `service_status.json` remains read-only.
- **Local usage and cost.** Scans bounded Codex, Claude, and Gemini CLI session JSONL files
  into a priced-usage view, broken down by harness, by model, and by the company
  that bills for it. Honours the shared Cost Data privacy setting: with it on,
  nothing is scanned, any snapshot this client saved is deleted, and the window
  says the data is hidden rather than showing zeroes. A completed aggregate
  snapshot persists only under
  `client/desktop/`; unknown models stay visibly unpriced and no shared ledger
  or history is written.
- **Skills inventory.** Lists skills from the fixed local SSOT and harness
  roots, with verified projections and health warnings. It never installs,
  deletes, syncs, or executes skill content.

API-key adapters read credentials from the process environment and never
persist them: `DASHSCOPE_API_KEY` (or `ALIBABA_API_KEY`), `Z_AI_API_KEY`,
`COPILOT_TOKEN`, `MINIMAX_CODING_API_KEY` (or `MINIMAX_API_KEY`),
`KILO_API_KEY`, `OPENROUTER_API_KEY`, and `WARP_API_KEY` (or `WARP_TOKEN`).
Kilo can also read its CLI login file; Kiro runs `kiro-cli` non-interactively.
Endpoint/region overrides retain each provider's standard env names; see the
corresponding module in `providers/`.

## One product, two clients

Desktop **depends on no part of the native app**: not its process, not its
MCP socket, not its binaries. On a Mac that has never seen the native app it
discovers its own data, and works.

But Vibe Bar data belongs to the *user*, not to one client. On the same Mac,
both clients read the same `~/.vibebar`. Desktop shows the providers the
native app tracks even though it has no adapter for them yet, and honours the
menu-bar fields and labels you configured once.

In this preview that sharing is read-only, with one exception:

| | Shared data root | `client/desktop/` |
| --- | --- | --- |
| Read | yes | yes |
| Write | **only `settings.json`** | yes |

Every other shared store stays read-only, and for the original reason: writing
one needs a cross-process storage contract — a lease, schema negotiation,
fail-closed migrations — that exists on neither side, and several of them
respond to a schema mismatch by dropping data.

`settings.json` is the exception because it now has that contract, in
[docs/contracts/settings-write-v1.md](docs/contracts/settings-write-v1.md): an
advisory `flock(2)` both clients take, a merge that puts back only the keys the
writer changed and preserves every key it does not know, and a whitelist of the
settings Desktop's own Settings presents. See
[docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md).

[HANDOVER.md](HANDOVER.md) is the map from here to parity: what the native
app does that Desktop does not, in what order to close the gap, and the bugs
this preview found in itself along the way.

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
parity — the quota hierarchy, tray percentages with your own fields and
labels, session search and transcripts, mini-window geometry, and so on. A new
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
| Live provider fetch | ● 25 | ◐ 10 | Desktop reads the rest from the shared cache, labelled as such |
| Browser-cookie providers | ● | ○ | Windows blocks third-party cookie reads; explicit import there |
| Observation and forecast history | ● | ◐ | Desktop records observations and draws the reset-history strip; the quota history chart with its brush is not ported |
| Service status sources | ● 5 | ● 4 | |
| **Menu bar / tray** |
| Rich-text and two-row title | ● | — | Windows and Linux trays have no title at all, only an icon |
| Field editor with style scopes | ● | ○ | |
| Control Center allow-list watchdog | ● | — | macOS 26 platform behaviour |
| **Main window** |
| Provider detail pages | ● 4 | ◐ | Desktop has a page per company with its quota and status; native's also carry that provider's cost cards and history charts |
| Arrangeable module waterfall | ● 11 | ○ | |
| Layout editor with presets | ● | ○ | |
| **Mini window** |
| Layouts | ● 7 | ◐ 4 | regular, compact, ledger, tile ported; strip, focus, rail not. Follows the shared `miniWindow.displayMode`, falling back to regular for the rest |
| Multiple independent windows | ● | ○ | |
| Translucent surface | ● Liquid Glass | ○ | Planned as a platform blur, deliberately not a copy. The window is currently opaque and undecorated |
| **Workbench** |
| Usage charts, donuts, breakdown tables | ● | ○ | Desktop draws the reset-history strip but no usage charts |
| Session deletion | ● | ○ | |
| Resets: risk view | ● | ◐ | Desktop lists resets with each one's forecast; the calendar and the risk grouping are not ported |
| Skills: install, import, discover, backups | ● | ◐ | Desktop is a read-only inventory |
| **Cost and usage** |
| Local usage scan | ● 7 harnesses | ◐ 3 | Codex, Claude Code, Gemini CLI. Counts harnesses with a local scanner: Cursor's usage comes from dashboard events and Grok Bot has no usage source at all, so neither is a local scan on either side |
| Per-request ledger, multi-source pricing, history | ● | ○ | Desktop keeps an in-memory aggregate |
| Spend by billing company | ● | ● | Both group by company rather than by harness: two harnesses can bill one company |
| **Settings** |
| Writable | ● | ◐ 3 | The cross-client write contract is in place (`docs/contracts/settings-write-v1.md`): a lock, a merge that keeps every key the writer does not know, and a notice when the other client replaces a choice made here. Desktop writes the three settings its own Settings presents |
| Provider credential panes | ● 25 | ○ | |
| **Platform** |
| MCP tools | ● 12 | ◐ 6 | Read-only subset. `cost.snapshot` reports what the last local scan found, in the shape native's does |
| Remote probe sync | ● | ○ | |
| Launch at login | ● | ○ | |
| In-app updates | ● Sparkle | ○ | Planned on the Tauri updater |
| App Sandbox | ○ by design | ○ for now | Neither ships sandboxed. Native **cannot**: reading browser cookies, probing AntiGravity with `ps`/`lsof`, and driving Terminal by Apple events are all blocked inside it, and the release script refuses a sandboxed bundle. Desktop needs none of that while it stays read-only, so it is the one that *could* — an option that closes as soon as it grows cookie providers |
| Windows and Linux | — | ◐ | Core is tested on all three; the GUI has only had a macOS pass |

## Layout

```
crates/vibebar-desktop-core/   Platform-independent core: shared-data readers,
                               provider adapters, refresh orchestration.
                               No Tauri, no GUI, tested on all three platforms.
apps/desktop/                  React + TypeScript UI
apps/desktop/src-tauri/        Tauri shell: window, tray, IPC
docs/                          Architecture, the shared-storage rules, and
                               the design record for cross-client writes
```

The `core` crate is tested on macOS, Linux, and Windows on every pull request.
The app job — workspace tests, frontend build, and the Tauri build — runs on
macOS only, and the GUI has had its end-to-end pass there only.

Session reading comes from [`agent-session-core`](https://github.com/AstroQore/agent-session-kit),
the Rust lane of `agent-session-kit` — the same kit the native app's Swift
implementation uses, so both clients read sessions by one set of rules.

## Build

Requires Rust (stable), Node 22.13+, pnpm, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your
platform.

```sh
cd apps/desktop && pnpm install
pnpm tauri dev      # run
pnpm tauri build    # package
```

### Read-only MCP stdio

Run the Desktop binary with `--mcp-stdio` to serve cached `quota.get`,
`sessions.list`, `sessions.search`, `status.get`, and `pricing.effective` over
JSON-RPC stdin/stdout. This mode never refreshes providers, scans usage, writes
configuration, or connects to the native app. `status.get` returns only
Desktop's fresh private last-good snapshot; `pricing.effective` returns the
static Codex, Claude, and Gemini table used by the local cost scanner.
Session calls accept the native provider and harness filters; listing also
supports RFC3339 `since`, `offset`, and bounded `limit` pagination.

Verification:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm build
```

Point the app at a synthetic data root with `VIBEBAR_DEMO_HOME=<dir>` — the
same environment variable the native app's demo mode uses. In demo mode
Desktop makes no network requests and reads no credentials.

There is also a read-only diagnostic that prints what Desktop can see from a
data root without starting the GUI:

```sh
cargo run -p vibebar-desktop-core --example inspect -- <data-root>
```

## License

AGPL-3.0-only, same as the native app.
