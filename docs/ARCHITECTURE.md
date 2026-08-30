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
│   paths · model · shared (read-only) · client_store      │
│   credentials · providers · refresh · sessions           │
└───────────┬─────────────────────────────┬───────────────┘
            │                             │
   ┌────────▼─────────┐        ┌──────────▼──────────────┐
   │ agent-session-   │        │ ~/.vibebar              │
   │ core (kit crate) │        │ shared: read-only       │
   │ index · sessions │        │ client/desktop: r/w     │
   └──────────────────┘        └─────────────────────────┘
```

The split is deliberate: everything that is not about windows, trays, or IPC
lives in the core crate, which has no Tauri dependency and is tested on
macOS, Windows, and Linux without a GUI. The shell is thin enough to read in
one sitting.

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
| Indexed vs scanned sessions | `sessions.rs` |

## Data flow for a refresh

1. `QuotaEngine::refresh` fetches every provider with a live adapter
   (Codex, Claude today), in sequence, each with a 30-second ceiling.
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

Desktop fetches two providers and reads twenty-five. A number it fetched and
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
opaque, CSPRNG-backed `sessionRef` that expires after 15 minutes or the next
listing; transcript IPC resolves that capability back to the currently
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
