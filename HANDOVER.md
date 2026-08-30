# Handover: from this preview to feature parity

Written for the agent taking `vibe-bar-desktop` from its 0.1 preview to full
parity with the macOS native app. Baseline: `AstroQore/vibe-bar` `main` at
`fbd5371` (dev channel `v1.4.1-dev.50`).

Read [AGENTS.md](AGENTS.md) first — the rules there prevent data loss.
This document is the map: what exists, what the native app does that Desktop
does not, in what order to close the gap, and which parts of the earlier plan
turned out to be wrong.

---

## 1. What this preview actually does

| Area | State |
| --- | --- |
| Quota — Codex, Claude | Live fetch from the provider's own API using CLI credentials |
| Quota — the other 23 providers | Read from the shared cache, labeled `shared data` |
| Tray | One line, fields and labels from the shared settings, remaining/used honoured |
| Sessions | Shared index when present (all harnesses, FTS); otherwise Codex + Claude log scan |
| Transcripts | Codex and Claude Code JSONL, paged, tolerant of unknown lines |
| Resume | Command built by the kit's shared builder, copied to the clipboard |
| Windows / Linux | Core crate builds and passes on both in CI; no GUI verification yet |
| Writes | Only `<root>/client/desktop/`, enforced in code and tested |

Verified against a real data root: 44 accounts, 25 providers, a 12,852-session
index, and parse output matching the native app's cache field for field.

## 2. What the native app does that Desktop does not

Ordered roughly by how much of the product they represent. The full inventory
is in the native repo's `AGENTS.md` and source; this is the gap list.

**Providers (23 adapters).** Gemini Web (learned batchexecute recipe +
WebView calibration), AntiGravity (local language-server probe via `ps` and
`lsof`), Grok (protobuf byte scan), Cursor (app state DB → cookie), Copilot
(GitHub device flow), and 18 misc providers spanning cookie jars, API keys,
AK/SK signing, and one that shells out to a CLI. Each has its own credential
route and wire shape.

**Credentials.** Browser cookie extraction (six browsers, macOS Keychain
"Safe Storage" decryption), WebView login windows, a single-Keychain-item
credential vault, access gating and cooldowns so a background refresh never
raises a prompt storm.

**Cost and usage.** The session-log scanner (six harnesses, each with its own
token accounting), the per-request SQLite ledger, five-source pricing merge,
cost history with max-merge semantics, fill and forecast timelines, and
subscription cycle inference.

**Surfaces.** The Workbench (Usage Stats, Resets, Skills), mini windows
(seven layouts), the layout editor, the full settings tree, service status
polling, the MCP server (12 tools), remote probe sync.

**Platform.** Sparkle updates, launch at login, the Control Center menu-bar
watchdog, AppleScript terminal handoff, Liquid Glass.

## 3. Order of work, and why

### P1 — Storage contract, then the native retrofit

Everything downstream needs Desktop to be able to *write* shared state, and
nothing may write it safely today. Do this first, and do it in the native
repo as much as here:

1. Write the contract down in `vibe-bar/docs/contracts/`: per store, its
   schema, its version mechanism, its mismatch behaviour, and its locking.
   The current per-store reality — including which ones destroy data on a
   version mismatch — is in [docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md).
2. In the native app: an OS-level advisory lock and single-writer lease under
   `~/.vibebar/run/`, elected by role (collector, migrator, pruner), not one
   lease for all writes — the SQLite stores are WAL and safe for concurrent
   readers already.
3. Replace destructive schema handling with fail-closed everywhere:
   `session_index` (drop + rebuild), `usage_events` (reset), `scan_cache` and
   `cost_snapshots` (delete file), `settings.json` (overwrite with defaults on
   a parse failure).
4. Move the shared mutable JSON stores (`cost_history`, `subscription_history`,
   `service_status`) to SQLite. This continues the direction the native app
   already took with the timeline stores in PR #243, and it is far cheaper
   than making cross-language byte-identical JSON CAS work.
5. `settings.json` stays JSON but gains a revision and lossless patch
   semantics: re-read, preserve unknown fields, write back.
6. Flush every coalesced store on exit, not only settings.

Only after this may Desktop take a lease.

### P2 — Desktop becomes a writer

Usage scanning (Codex and Claude first, then the rest), writing to the shared
quota cache, field registry, and usage ledger. Carry the native app's
invariants verbatim: O(n) JSONL scanning with a moving cursor, three
non-overlapping token columns, integer micro-USD, max-merge for local scans
versus replace for authoritative sources, and the forecast timeline's rule
that a prediction is recorded as it was made, never recomputed with hindsight.

### P3 — Provider matrix

The 23 remaining adapters, and the credential machinery they need. Two
platform-shaped decisions to make here:

- **Browser cookies on Windows.** Chrome's App-Bound Encryption blocks
  third-party decryption. [Win-CodexBar](https://github.com/nesszer/Win-CodexBar)
  (Tauri 2 + Rust, MIT) solved the same problem the way this project should:
  automatic user-scoped DPAPI decryption where it works, explicit "paste the
  Cookie header from DevTools" import where it does not, and DPAPI-protected
  storage for what the user supplies. It is worth reading before starting.
- **The credential vault.** The native app keeps everything in one Keychain
  item (`com.astroqore.VibeBar.credential-vault`) because ad-hoc signing makes
  per-item prompts unbearable. Desktop reading that item is possible but
  prompts once, and two processes doing read-modify-write on it needs the same
  lease discipline as the file stores. Its payload schema has to enter the
  contract. On Windows, a DPAPI-protected file is the equivalent.

### P4 — Analysis surfaces

Usage Stats, the cost pipeline, Resets. Large but mostly mechanical once P2
lands.

### P5 — Advanced surfaces

Mini windows, layout editor, Skills (carry the write allowlist and the
never-follow-a-symlink-while-deleting rule exactly), the MCP server (same 12
tools, `additionalProperties: false` on every schema; decide socket ownership
by lease), remote sync via `vibebar-protocol`, service status polling.

This is also the point to do the `agent-session-kit` symmetric re-layout
(`implementations/swift/` and `implementations/rust/` as peers under a shared
`contracts/`). It is a pure move, and doing it before the Rust lane has
grown would be churn for no benefit.

### P6 — Release and the version switch

Desktop's own signed update feed, then the parity checklist. When it passes,
Desktop takes the shared `MAJOR.MINOR.PATCH` and both repositories' CI starts
validating a common release manifest, failing closed when a feature tag is
present on only one side.

## 4. Corrections to the earlier plan

The plan this preview started from was written before the code was read. What
changed, and why:

1. **Sequencing was inverted.** It required the native storage retrofit
   *before* any cross-platform work. Starting read-only meant a working client
   with zero native changes, and it surfaced four real bugs (§5) that a
   contract-first approach would have found much later.
2. **The lease was scoped too broadly.** Treating every write as needing a
   lease ignores that all the SQLite stores are WAL with a busy timeout —
   concurrent readers and row writes are already safe. What actually needs
   arbitration is *roles* (who scans, who prunes, who migrates) and the
   whole-file JSON rewrites.
3. **Cross-language JSON CAS was underestimated.** Two encoders with different
   formatting, a Swift `Dictionary` that encodes as an alternating array, and
   a pricing revision hash computed over `JSONSerialization` output that is
   written into the usage ledger. Migrating those stores to SQLite is cheaper
   and less brittle than byte-matching Foundation.
4. **Sessions-first was the wrong first slice.** Quota is Vibe Bar's product
   identity; a client that cannot show quota is not a Vibe Bar client. Sessions
   also happened to be the one area where reading another client's index is
   most dangerous.
5. **Two planned parity items do not exist in the native app.** It has no user
   notification system and no single-instance guard. Do not invent them as
   "parity"; Desktop added single-instance because a second tray polling the
   same root is a real defect.
6. **Version alignment needed a pre-parity clause.** Shipping `1.5.0` with a
   fraction of the features would misdescribe the product. Hence `0.x` until
   parity, then the shared train.

## 5. Bugs this preview found in its own implementation

Kept because each is a trap the next adapter or reader can fall into. All
four were found by running against a real data root, not by tests.

1. **`"secondary_window": null` became a phantom bucket.** Swift's
   `as? [String: Any]` rejects `null`; `serde_json`'s `.get()` returns
   `Some(Value::Null)`. Guarded by `null_windows_are_not_buckets`.
2. **A future-dated cache entry outranked every real reading.** The shared root
   held a Claude entry stamped five months ahead. Any "newest wins" rule
   picks it forever. Guarded by `has_plausible_timestamp`.
3. **One account rendered as two cards.** The shared cache is keyed by
   `sha256(accountId)`, so an account whose id Desktop could not guess got an
   opaque key — including the account Desktop had itself fetched under its
   real UUID. Fixed by claiming ids from its own store.
4. **Five cached entries for one subscription.** Two identical, three empty.
   Fixed by consolidating each provider to one card of newest-believable
   windows, which also merges routes that each report a different window.

## 6. Known differences from the native app

- **Gemini weekly reads 99% where the native menu bar shows 96%.** Both are
  real cached observations from different accounts; Desktop takes the newest
  believable one, the native app renders the account its `AccountStore`
  currently considers active. Resolving this means modelling account activeness
  rather than just recency — worth doing in P2, when Desktop has its own
  account store.
- **Tray rendering is a single line.** `NSStatusItem` takes an attributed
  string with per-field colour and an optional two-row layout; a Tauri tray
  takes a plain title. Colour and multi-row need a rendered image.
- **No Liquid Glass, no Control Center watchdog, no Sparkle.** Platform
  features with no cross-platform equivalent; document them as macOS-native
  advantages rather than gaps.
- **The window is shown on every launch.** The native app is `LSUIElement` and
  starts silently in the menu bar. Desktop shows its window so a new user can
  tell it started at all, which is right for a preview and probably wrong once
  it is familiar: show on first run, stay in the tray afterwards. That needs a
  persisted first-run flag, so it is a deliberate open decision rather than an
  oversight.
- **The app icon is a placeholder.** `apps/desktop/src-tauri/icons/icon.png`
  is a flat 32×32 square. Real icons (including the `.icns` and `.ico` sets a
  full bundle wants) are outstanding.

## 7. Ecosystem notes

- `agent-session-core` lives in `AstroQore/agent-session-kit` alongside the
  Swift implementation, added by PR #12. Desktop depends on it by git
  reference; once that PR merges, switch the dependency to a tag.
- `vibebar-protocol` holds the remote-sync contracts and is dormant since
  2026-08-03. Its `ingest-p256-v1` conformance vector is verified by exactly
  one implementation today (the native app, via a vendored copy). Before P5
  remote work, that vector should be verified by every implementation, and
  the vendored copies should be kept in sync automatically.
- `vibebar-probe`, `vibebar-relay`, `vibe-bar-web`, `vibe-bar-web-backend`
  are the remote-platform stack and unchanged by this work.
