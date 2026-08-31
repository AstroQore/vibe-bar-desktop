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
  separate. It does not manufacture history or a forecast.
- **Tray.** One line of the fields you picked, with your labels and your
  remaining-vs-used preference — read from the same settings the macOS menu
  bar uses.
- **Sessions.** Search and read local agent sessions. Uses the shared session
  index when one exists (every harness it covers, full-text search), and falls
  back to scanning Codex and Claude Code logs directly when it does not.
- **Presentation settings.** Applies the shared used/remaining mode, provider
  visibility/order, plan labels, and menu-bar field labels. A read-only
  Settings page shows the effective values; Desktop does not save them yet.
- **Service status.** Reads cached native status immediately, then refreshes
  OpenAI-wide, Claude, Google AI, and Cursor status from public feeds without credentials.
  A fresh Desktop last-good snapshot is private to `client/desktop/`; shared
  `service_status.json` remains read-only.
- **Local usage and cost.** Scans bounded Codex, Claude, and Gemini CLI session JSONL files
  into a priced-usage view. A completed aggregate snapshot persists only under
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

In this preview that sharing is strictly one-way:

| | Shared data root | `client/desktop/` |
| --- | --- | --- |
| Read | yes | yes |
| Write | **never** | yes |

Desktop becoming a writer of shared state needs a cross-process storage
contract (single-writer lease, schema negotiation, fail-closed migrations)
that does not exist on either side yet. Until it does, writing would risk the
user's history for no gain. See [docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md).

[HANDOVER.md](HANDOVER.md) is the map from here to parity: what the native
app does that Desktop does not, in what order to close the gap, and the bugs
this preview found in itself along the way.

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
