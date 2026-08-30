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

- **Quota.** Fetches Codex and Claude usage directly from their own APIs,
  using the credentials the Codex and Claude CLIs already wrote. Every other
  provider Vibe Bar tracks is read from the shared data root and labeled
  `shared data`, so the UI never overstates how fresh a number is.
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
- **Local usage and cost.** Scans bounded Codex and Claude session JSONL files
  into an in-memory priced-usage view. Unknown models stay visibly unpriced;
  Desktop does not write a cost cache or ledger yet.

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
docs/                          Architecture and the shared-storage contract
```

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
`sessions.list`, `sessions.search`, and `status.get` over JSON-RPC stdin/stdout.
This mode never refreshes providers, scans usage, writes configuration, or
connects to the native app. `status.get` returns only Desktop's fresh private
last-good snapshot, never a network refresh or native shared-status write.

Verification:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm build
```

Point the app at a synthetic data root with `VIBEBAR_DEMO_HOME=<dir>` — the
same environment variable the native app's demo mode uses. In demo mode
Desktop makes no network requests and reads no credentials.

## License

AGPL-3.0-only, same as the native app.
