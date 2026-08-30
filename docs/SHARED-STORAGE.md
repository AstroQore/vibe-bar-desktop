# Shared storage contract

Vibe Bar is one product with two client implementations. On a Mac that runs
both, they share one data root — `~/.vibebar` — because that directory holds
the *user's* Vibe Bar data, not any one client's private state.

This document states what Desktop may do with it today, why the limit is
where it is, and what has to be built before it moves.

## The rule today

**Desktop reads the shared root and writes only `<root>/client/desktop/`.**

The boundary is enforced in code, not by convention:
`ClientStore::write_json` refuses any path outside the client namespace
(`crates/vibebar-desktop-core/src/client_store.rs`), and there is a test that
a write to each shared store is rejected *and* creates nothing.
The writer accepts only fixed client destinations, rejects `..` path
components, and refuses symlinks in the private directory chain so a lexical
`client/desktop/` prefix cannot escape back into shared state. It anchors the
shared root once and performs creation, temporary-file allocation, and rename
through capability directory handles, so a concurrent pathname replacement
cannot redirect the final write.

| Path | Desktop |
| --- | --- |
| `settings.json` | read |
| `quotas/` | read |
| `quota_field_registry.json` | read |
| `service_status.json` | read |
| `session_index.sqlite3` | read (see below) |
| `mcp.sock` | stat only, never connected |
| everything else in the root | untouched, including files Desktop does not recognize |
| `client/desktop/` | read and write |

## Why Desktop does not write shared state yet

Not caution for its own sake — the shared stores currently have properties
that make a second writer unsafe:

1. **No cross-process locking anywhere.** No `flock`, no lockfile, no
   coordination protocol. Every JSON store is a whole-file read-modify-write
   with last-writer-wins.
2. **Write coalescing hides the race.** The native app defers writes by
   250 ms (settings) up to 30 s (cost and subscription history). Two processes
   editing inside that window silently lose each other's changes.
3. **Schema mismatch is destructive.** `session_index.sqlite3` drops and
   rebuilds every table when `user_version` is not what it expects.
   `usage_events.sqlite3` resets on an unknown `schema_version`.
   `scan_cache/` and `cost_snapshots/` delete the file. A second
   implementation that "helpfully" repairs one of these destroys work the
   other client did.
4. **Nothing prunes.** The shared cache keeps an entry for every credential
   route ever tried, including accounts the user signed out of. One real root
   held five entries for a single Claude subscription — two identical, three
   empty — and one entry stamped five months in the future.

So Desktop treats every shared store as read-only, and every reader is
tolerant: a store it cannot parse degrades to "not available" instead of
failing the app or being rewritten.

### What a writer would need

Before either client may write shared state:

- An OS-level advisory lock and a single-writer lease per store, implemented
  identically in both languages, with no dependency on either app running.
- Schema negotiation before any write, and fail-closed on an unknown version
  — never a destructive rebuild of durable data.
- Lossless patch semantics for `settings.json`: re-read, preserve unknown
  fields, and carry a revision.
- Migrations under an exclusive lock, with a backup and a recoverable marker.
- `flushPendingWrites` on exit for every coalesced store (the native app
  currently flushes only settings).

## Settings v1 patch foundation (not a product writer)

`shared::settings_document` now supplies a pure, product-disabled v1 document
parser and top-level three-way patch engine for the native expected
`settings.json` location. It has no file-write or lease acquisition API. The
manifest remains `json_unversioned` and `legacy_unsafe`; production
`SharedStoreLeaseBatch::acquire_writer` continues to reject Settings.

The diagnostic-only transaction foundation fingerprints the exact settings
source it read (existence, length, SHA-256) and re-reads it through the same
directory capability immediately before rename. A cooperative writer that
changes the file in that window receives `SourceChangedBeforeCommit`; Desktop
cleans its temp sibling and never reports success. No user-space check can
close a hostile, non-cooperating replacement between that final check and the
OS rename; the future enabled protocol therefore still requires the shared
lease and joint interop testing before this path becomes product authority.

The prospective v1 envelope is `schemaVersion: 1` plus an unsigned `revision`.
Legacy objects with neither key read as v0 / revision 0. A patch preserves raw
unknown values, changes only the documented Desktop first-batch whitelist, and
fails without partial changes on an unknown version or a per-key conflict. See
[settings-document-v1.md](contracts/settings-document-v1.md) for the synthetic
fixture, conflict table, and enablement boundary.

## Reading the session index safely

The index is opened `SQLITE_OPEN_READONLY`, and any `user_version` other than
the one this build knows is refused outright — Desktop shows locally scanned
sessions and says why, rather than touching a database another client owns.

One caveat that looks like a write and is not: opening a WAL database
read-only mmaps its `-shm` sibling, which updates that file's mtime. The
database and the `-wal` are untouched — verified byte-for-byte against a live
12,852-session index. An audit of "did Desktop write anything?" should expect
exactly that one mtime bump and nothing else.

Opening with `immutable=1` would avoid even that, at the cost of not seeing
rows the writer has committed to the WAL but not yet checkpointed. A stale
read is the worse trade.

## Cross-client identity

Quota cache files are named `quota-v1-<sha256(accountId)>.json`, so an account
id cannot be recovered from a filename — only guessed and hashed. Desktop
therefore claims what it can:

- The stable ids the native `AccountStore` mints (`oauth-claude`, `cli-codex`,
  `misc-<instance>`, …).
- Every account id Desktop itself has seen, because a provider that reports a
  real account UUID (Codex does) writes its shared entry under that UUID.

Anything still unmatched is shown under its opaque cache key rather than
dropped — an account this build cannot name is still the user's account.

## Timestamps are not trusted blindly

Observations carry a Unix timestamp, and the shared root has held one from
the future. An observation cannot come from the future, so one that claims to
does not win a recency comparison
(`AccountQuota::has_plausible_timestamp`, 5-minute clock-skew tolerance).
Without that rule, a single broken entry outranks every real reading forever.

## Windows and Linux

Same layout, same schemas, same stable ids, at the platform's own app-data
location (`%APPDATA%\VibeBar` on Windows). There is no native app to share
with there; the structure is identical so that one contract, one set of
fixtures, and one migration story cover all three platforms.
