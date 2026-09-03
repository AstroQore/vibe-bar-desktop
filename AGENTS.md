# AGENTS.md

Operating manual for AI agents working in `AstroQore/vibe-bar-desktop`.
Read this before touching code. The companion documents are
[README.md](README.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md); the native app's own
`AGENTS.md` in `AstroQore/vibe-bar` remains authoritative for anything about
the macOS client.

## 1. What this is

The cross-platform client of Vibe Bar: Tauri 2 shell, React + TypeScript UI,
and a platform-independent Rust core. Bundle id
`com.astroqore.VibeBarDesktop`. AGPL-3.0-only. Not sandboxed on macOS, the
same as the native app.

One product, two client implementations. Desktop depends on **no** part of
the native app — not its process, not its MCP socket, not its binaries — and
must work fully on a machine that has never had it installed.

## 2. Rules that cause silent damage if ignored

1. **Write a shared store only through its documented writer.** The shared
   root has no cross-process locking for most stores, coalesces writes for
   up to 30 seconds, and responds to schema mismatches by dropping data, so
   Desktop's own state lives under `<data root>/client/desktop/`
   (`ClientStore::write_json` enforces that boundary and there are tests;
   do not add a bypass). Both clients write the shared stores that have a
   writer whose rules are written down — and nothing else:

   - `settings.json`, through `shared::settings_writer::SettingsWriter`:
     an advisory `flock(2)` both clients take, a merge that puts back only
     the top-level keys this process changed and preserves every key this
     build does not know, and the whitelist of what Desktop's own Settings
     presents. The rule is
     [docs/contracts/settings-write-v1.md](docs/contracts/settings-write-v1.md),
     shared with the native app — a change to it is a change in both
     repositories.
   - `quotas/quota-v1-<sha256(accountId)>.json`, through
     `shared::quota_cache::save`: the native `QuotaCacheStore` schema (tool,
     buckets, plan, `queriedAt` in Apple reference seconds), pretty with
     sorted keys, one account per file, written to a temporary file and
     renamed; only a read that succeeded, never from demo mode. The last
     atomic write for an account wins, which is what the native app's own
     multiple routes already do.
   - The Control Center menu-bar allow-list, a macOS system preference and
     not a Vibe Bar store, rewritten by the menu bar health repair only on
     the user's click or the shared `menuBarAutoRepairEnabled`, through the
     bundled `fix_menu_bar_allowlist.py` (backup, only stale references to
     this bundle id, atomic write, Control Center restart). See
     [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

   - Whole-session deletion — a session's own log files under `~/.codex/`
     or `~/.claude/`, never the shared index — through the kit's fenced
     deleter (`agent_session_core::deletion`, the Rust counterpart of the
     native `SessionDeleter`), only from the Sessions page after the person
     armed and confirmed the delete: every target must resolve strictly
     below one of the provider's roots under this home with no symlinked
     component, and the file is re-parsed for its session id right before
     removal so a stale row cannot delete a different session. Only Codex
     and Claude Code sessions are candidates. A deleted session leaves
     listings because its file is gone; the index row is the native app's
     to drop. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

   - The skill library — `~/.agents/skills/` (the SSOT), the allow-listed
     app skills directories (`~/.codex/skills`, `~/.claude/skills`,
     `~/.gemini/skills`, `~/.gemini/config/skills`, `~/.grok/skills`,
     `~/.cursor/skills`; Hermes and OpenCode roots only so old registries
     decode), the registry `~/.vibebar/skills.json`, and snapshots under
     `~/.vibebar/skill_backups/` — through `skills::service::SkillsService`
     and nothing else, at the person's explicit request (a toggle, Install,
     Import, Uninstall, Restore): the native `SkillSyncEngine` rules
     verbatim — a name is one safe path segment, every path mutated sits
     under those roots, a sync needs a `SKILL.md`, ancestors are created
     one component at a time, deletion never follows a symlink and removes
     only links that resolve back into the SSOT or copies whose recorded
     SHA-256 still matches. The registry is re-read immediately before each
     whole-file atomic write; `~/.agents/.skill-lock.json` is read for
     provenance and never written. Harness config patches (native
     activation) and repository installs stay with the native app for now.

   Everything else under the shared root — the session index, the usage
   ledger, cost history — is read here and written by the native app until
   it too has a writer with rules like the above. Full reasoning in
   [docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md).
2. **Never repair, migrate, prune, or rebuild a shared store.** Including the
   session index. An unreadable or unknown-version store degrades to "not
   available" with an explanation — it is another client's data.
3. **Keep the two naming axes apart.** Quota is L1 company → L2 SubProvider →
   L3 bucket (`ToolType::hierarchy`). Usage is the harness that produced a
   session. A surface picks one axis and stays on it; never mix levels of the
   two in one list.
4. **Storage keys are a contract with the native app.** `ToolType` raw values,
   bucket ids, account ids, and the `quota-v1-<sha256>` filename scheme must
   match the Swift implementation byte for byte. Changing one is a
   coordinated change across both repositories, never a local rename.
5. **No secrets or personal paths in source.** No real tokens, cookies,
   account UUIDs, organization ids, email addresses, `/Users/<name>` paths, or
   hostnames in code, tests, fixtures, or log strings. Use
   `/Users/example/...` and synthetic values. Credentials are read, used for
   the provider's own endpoint, and never logged or persisted.
6. **Read credentials, do not rewrite them.** `~/.codex/auth.json` is shared
   with the Codex CLI and the native app, none of which take a lock. An
   expired token is reported as `needsLogin` so the user re-runs the CLI's own
   login; silently rewriting it risks corrupting the credential all three
   share.
7. **Timestamps from a shared cache are untrusted input.** An observation
   cannot come from the future; one that claims to must not win a recency
   comparison. See `AccountQuota::has_plausible_timestamp`.

## 3. Versioning

Two layers, and they are not the same thing:

- `productVersion` — the Vibe Bar product. **Pre-parity**, Desktop carries its
  own `0.x` and is not part of the native app's release train: shipping
  `1.5.0` with a fraction of the features would misdescribe the product.
  **At parity**, Desktop adopts the shared `MAJOR.MINOR.PATCH`, and from then
  on every feature release ships from both repositories with the same version.
- Build numbers and `-dev.N` are per client. A client-only bug fix may ship
  alone; the next feature train realigns the base SemVer.

Data schema versions are independent of the app version and are managed by
the storage contract, not by SemVer.

## 4. Verification before claiming a change works

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm build
```

For anything touching the data layer, also verify against a real root that
Desktop wrote nothing into it:

```sh
find ~/.vibebar -maxdepth 2 -type f -exec stat -f '%N %m %z' {} \; | sort > /tmp/before
cargo run -p vibebar-desktop-core --example inspect
find ~/.vibebar -maxdepth 2 -type f -exec stat -f '%N %m %z' {} \; | sort > /tmp/after
diff /tmp/before /tmp/after
```

The only acceptable difference is the `session_index.sqlite3-shm` mtime (a
read-only WAL open mmaps it; the database and `-wal` are untouched). Anything
else is a bug.

`cargo run -p vibebar-desktop-core --example inspect` prints no credentials,
tokens, emails, or account identifiers, and is safe to paste into a report.

## 5. Adding a provider adapter

1. Add the parse function with tests against a **synthetic** payload first —
   wire shapes are unit-tested without a network.
2. Match the native app's bucket ids exactly; a mismatch means the two
   clients disagree about the same subscription.
3. Reuse the shared error taxonomy (`QuotaError`) so both clients classify
   the same failure identically.
4. Reject non-object window slots. A provider sending `"secondary_window":
   null` must not become a phantom 100%-left lane — this was a real bug, and
   `null_windows_are_not_buckets` guards it.

## 6. Branch and PR workflow

Repository is `AstroQore/vibe-bar-desktop`, default branch `main`. Start from
current `main`, work on a topic branch (`feat/`, `fix/`, `chore/`, `docs/`),
and open a PR. Prefer a separate worktree for non-trivial work — several
agents may be in this repository at once. Never force-push `main`.

Commit subjects are imperative, ≤70 characters, with no `feat:` / `fix:`
prefix — those belong on branch names. Include a `Co-Authored-By:` trailer
for every real participant.

## 7. What not to change without explicit instruction

- The license (AGPL-3.0-only) or the bundle id.
- The write boundary in `ClientStore`.
- Storage keys shared with the native app (see §2.4).
- The decision to run unsandboxed on macOS.
