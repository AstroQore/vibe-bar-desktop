# Handover: paused parity checkpoint

This is the authoritative restart note for `AstroQore/vibe-bar-desktop` as of
2026-08-31. Feature development is intentionally paused. Read
[AGENTS.md](AGENTS.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and
[docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md) before changing anything.

Do not infer full native parity from the amount of code that landed. The safe,
read-only and Desktop-private slices are on `main`; several credential,
shared-writer, analysis, surface, platform, and release lanes remain open.

## 1. Exact stop point

| Item | Checkpoint state |
| --- | --- |
| Desktop version | `0.1.0` (pre-parity versioning remains intentional) |
| Desktop feature stop | `42d34e90c475cc5c2550ec14a49c3c2274db44de`, clean and equal to `origin/main` before this docs-only checkpoint |
| Desktop integration | PR [#26](https://github.com/AstroQore/vibe-bar-desktop/pull/26) merged normally; merge tree is byte-identical to reviewed head `391d92663d576ee3b62a364f6373e21b84e97f4b` |
| Desktop feature PR queue | No open feature PRs; the earlier stacked PRs were verified as included and closed as superseded |
| Native baseline | `AstroQore/vibe-bar` `v1.4.1-dev.56`, commit `25b5b6959827c27cad20217ed9b3ba6af72b99b6` |
| Session kit | `AstroQore/agent-session-kit` `main` at `0419098d549bebdc67c63dc93f6be5d67929bdc4` |
| Desktop kit pin | `agent-session-core` pinned to reviewed commit `1959fbea750337e2c1ebf9ad6065d71d834861f2`; no new tag was created |
| Release state | No Desktop tag, package, installer release, signing, or update feed was produced |

The native baseline moved from dev.52 to dev.56 during this work. Those native
releases added SIGPIPE hardening, adopted session-kit 0.6.3, and refined
menu-bar logo metrics and Mini layout tiering. Desktop has not completed a new
visual parity pass against every dev.56 Mini variant.

## 2. What Desktop `main` does now

| Area | Current implementation |
| --- | --- |
| Quota: Codex and Claude | Live provider fetch using the official CLI credential files; credentials are read but never rewritten |
| Quota: Alibaba, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter, Warp | Live fetch using explicit environment credentials or the provider's official CLI route |
| Remaining providers | Shared-cache read only, labelled as shared data; no browser/session credential import |
| Presentation | Shared display mode, provider visibility/order, plan labels, native app icons, and a Desktop settings view |
| Tray and lifecycle | Single-instance app, close-to-tray, explicit Quit, first-run visibility, later tray startup, and resume handling |
| Mini | One Desktop-owned quota layout, tray toggle, private geometry and visibility state |
| Status | Public OpenAI, Claude, Google AI, and Cursor status with last-good state stored only under Desktop's private namespace |
| Usage and cost | Read-only local Codex, Claude, and Gemini CLI scan; aggregate snapshot stays Desktop-private |
| Resets | Provider-declared upcoming reset times from current quota observations; no forecasting or historical inference |
| Skills | Read-only inventory from fixed local SSOT and harness roots; no install, delete, or sync |
| Sessions | Shared index when compatible; otherwise bounded local Codex and Claude discovery |
| Search and transcripts | Indexed FTS or bounded metadata fallback, paged transcript reads, opaque expiring capabilities, and page-local find |
| Resume | Shared kit command builder with platform-safe shell handling; copied to the clipboard |
| MCP | Exactly five read-only tools: `quota.get`, `sessions.list`, `sessions.search`, `status.get`, `pricing.effective` |
| Writes | Enforced boundary: only `<data root>/client/desktop/` |
| Platforms | Cross-platform Rust/Tauri code exists; the final head has not had end-to-end Windows or Linux GUI QA |

Desktop does not depend on the native process, native binaries, or the native
MCP socket. It must remain usable on a machine where the native app has never
been installed.

## 3. What was merged

### agent-session-kit

- PR [#12](https://github.com/AstroQore/agent-session-kit/pull/12) merged as
  `958f867f7167bbb26ea9b6f3ba3f6e67b09ef20c`: the reusable Rust
  `agent-session-core` lane.
- PR [#14](https://github.com/AstroQore/agent-session-kit/pull/14) merged as
  `0419098d549bebdc67c63dc93f6be5d67929bdc4`: indexed transcript cursor
  compatibility and slash-command title follow-ups.
- Desktop deliberately pins `1959fbea...`, the reviewed PR #14 head. Tagging
  and switching the dependency to a release tag remain separate release work.

### vibe-bar-desktop

PR [#26](https://github.com/AstroQore/vibe-bar-desktop/pull/26) integrated the
safe feature stack in one normal merge. It includes:

- the shared storage contract reader, lease/record conformance code, lossless
  settings document and Desktop-private transaction path;
- live explicit-credential quota adapters and strict fail-closed parsing;
- presentation settings, public status, local usage/cost, Gemini scanning,
  upcoming resets, Skills, transcript search, Mini, tray/lifecycle, icons;
- Desktop-private status and cost persistence;
- the five-tool read-only MCP server and its filter/paging limits;
- isolated app-runtime demo roots so the demo UI's Sessions and Cost scans do
  not use the real home.

PRs #1 and #14 had already merged independently. After #26 landed, the heads
of the remaining stacked PRs were verified as ancestors of `main` (PR #16's
last patch was patch-equivalent) and those redundant PRs were closed with a
supersession note. No feature PR was left open.

## 4. Verification evidence at pause

The final integration head `391d926...` passed:

- `cargo test --workspace --all-targets`: 6 Tauri tests plus 236 core tests,
  242 total;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cd apps/desktop && pnpm typecheck && pnpm build`;
- `git diff --check`.

The merge commit `42d34e9...` has the same tree as that tested integration
head.

A Computer-driven macOS smoke pass used an isolated synthetic root and checked
Quota, public Status, Codex/Claude/Gemini cost, Resets, Sessions, transcript
paging/find, Skills, Settings, About, main-window close-to-tray, Mini show/hide,
and Mini geometry persistence. That pass found and fixed a demo-isolation bug:
demo Sessions had been scanning the real home. The final app uses
`SessionsService::with_home` with the demo scan root. The synthetic shared
root stayed unchanged; only Desktop-private Mini state changed.

This fix is not yet universal: `examples/inspect.rs` still constructs
`SessionsService::new(root)`. Running that diagnostic with a demo root can
therefore scan real-home session metadata. Do not use the example as
demo-isolation proof until that call site is fixed.

GitHub Actions on the final PR did not execute a single step. Every job ended
in about two seconds with the GitHub annotation that the job was not started
because account payments failed or the spending limit must be increased.
There was no admin bypass. Before relying on hosted cross-platform evidence,
fix the account billing/spending state and rerun the final workflows.

For any future data-layer change, also repeat the real-root immutability check
from [AGENTS.md](AGENTS.md). The only tolerated read-only side effect remains
the SQLite `session_index.sqlite3-shm` mtime.

## 5. Invariants that must not be weakened

1. Never write outside `<data root>/client/desktop/`.
2. Never repair, migrate, prune, rebuild, or downgrade another client's shared
   store. Unknown or unreadable versions degrade to unavailable with a reason.
3. Keep quota hierarchy and usage-harness grouping as separate naming axes.
4. Keep storage keys, raw provider ids, account ids, bucket ids, and quota
   filenames byte-compatible with native Swift.
5. Treat shared timestamps as hostile input; future observations cannot win a
   recency comparison.
6. Read CLI credentials but never refresh or rewrite them.
7. Do not place real credentials, account ids, emails, hostnames, or personal
   filesystem paths in source, fixtures, tests, or logs.
8. Keep Desktop on `0.x` until the parity checklist actually passes.
9. Keep the MCP surface read-only unless the user explicitly authorizes a
   particular local-data exposure or writer role.
10. On a demo root, quota, sessions, cost, status, geometry, and all scans must
    stay inside that demo root. The app runtime complies; the
    `examples/inspect.rs` Sessions call is a documented outstanding violation.

## 6. Deliberately not done

These items were not forgotten. They either require explicit authority or a
larger product/architecture decision.

### Requires explicit user authority

- Exposing `cost.snapshot` over MCP. The local aggregate includes usage,
  token, request, and spend information.
- AntiGravity discovery from process arguments/ports and its localhost API.
- Grok and the remaining browser/session-backed providers, including cookie
  extraction or import.
- Volc Agent AK/SK and other provider credential stores.
- Launch-at-login, which mutates OS login-item state outside the
  `client/desktop` filesystem boundary.
- Creating a tag, signing, packaging, publishing a release, or installing it
  over the user's current app.

### Still requires engineering

- A safe cross-client writer architecture for shared stores: role leases,
  fail-closed migrations, and removal of destructive native schema recovery.
- The remaining provider and credential matrix on macOS, Windows, and Linux.
- All-harness usage scanning, per-request ledger, multi-source pricing,
  history, fill/forecast timelines, and subscription-cycle inference.
- The other native Mini layouts, layout editor, full settings tree, writable
  Skills management, full Workbench, and deeper Resets surfaces.
- Native's broader 12-tool MCP surface, socket ownership, and remote probe
  sync.
- Windows/Linux GUI validation, launch integration, signed updates, and the
  final shared-version release checklist.

Do not start several of these lanes together. Pick one explicit product slice,
create one topic worktree, and keep its authority and verification boundary
narrow.

## 7. Known traps and fixes already present

- A JSON `null` secondary window once became a phantom 100%-left bucket.
  Non-object windows are now rejected or omitted according to the wire
  contract.
- A future-dated cache entry once outranked every real observation.
  `has_plausible_timestamp` prevents it.
- One subscription could render as several cards because shared cache filenames
  hide account ids. Desktop now claims its own ids and consolidates believable
  windows without mixing providers.
- Malformed Warp bonus-grant containers now fail the snapshot closed.
- Unknown Z.ai units are ignored before validation and sorting; malformed
  recognized units still fail closed.
- Scanned session fallback is intentionally bounded to the newest 400 sessions
  per provider. Every scanned list/search response discloses that older or
  filtered matches may be omitted.
- Fractional RFC3339 `since` boundaries round upward so sub-second sessions
  just before the boundary are not included.
- Bare slash commands are not used as session titles.
- A demo root must supply both the data root and scan home; otherwise synthetic
  QA can leak real session metadata into the UI. App state now supplies both,
  but `examples/inspect.rs` still needs the same correction.
- A disappearing MCP peer must not terminate the app through SIGPIPE. Native
  dev.53+ and the session kit include that socket hardening; preserve it in any
  future transport work.

## 8. Local cleanup and recovery notes

At pause:

- After this docs-only checkpoint worktree is merged and removed, Desktop has
  only its main worktree at `<desktop-repo>`.
- Twenty-eight obsolete Desktop worktrees and the task's temporary
  `agent-session-kit` worktree were removed.
- Twenty-two `/private/tmp/vbd-*` build, review, demo, and inspection paths
  were deleted, reclaiming about 36 GiB (38 GB).
- The two pre-existing Kit worktrees
  `feat/mcp-request-context` and `feat/rust-session-core` were intentionally
  left untouched.

Three dirty old Desktop worktrees contained task-time rustfmt residue; one also
contained an older copy of the already-merged platform-safe resume helper.
Direct forced deletion was rejected, so the changes were preserved before the
worktrees were removed:

| Stash commit | Original worktree |
| --- | --- |
| `2f2abf1ccc1e341c18fa86620f4e9a2cff77c4df` | `fix-main-close-hide` |
| `603be8dbb9ac32785ff3f33745b1bbf3e1c80147` | `feat-status-snapshot-mcp` |
| `c3a49835d847b4b188c7ef5f149d6551aa086392` | `feat-first-run-tray` |

They are recovery-only and are not needed by `main`. Inspect with
`git stash show --stat <commit>`; do not apply or drop them by default.

## 9. Restart checklist

1. Read this file and the three documents linked at the top.
2. Confirm actual current state; do not trust this checkpoint after branches
   have moved:

   ```sh
   git status --short --branch
   git log -1 --oneline
   git worktree list
   gh pr list --state open
   ```

3. Re-read the native checkout and note any drift from
   `v1.4.1-dev.56`/`25b5b69`.
4. Check whether GitHub-hosted jobs can start again. Rerun CI before treating
   Windows/Linux or storage-contract parity as current evidence.
5. Ask for or confirm authority before choosing any item in section 6.
6. Work in a fresh `feat/`, `fix/`, `docs/`, or `test/` worktree from
   current `main`; never edit the user's main worktree directly.
7. Run the full Rust, clippy, TypeScript, and build verification set. For data
   changes, run the shared-root before/after check. For UI changes, verify the
   installed or built app with Computer rather than relying on unit tests.
8. Merge normally through a PR. Do not use an admin bypass for billing-failed
   checks, do not tag or release without explicit instruction, and remove the
   task worktree after the PR is settled.

## 10. Ecosystem references

- Native app: [AstroQore/vibe-bar](https://github.com/AstroQore/vibe-bar)
- Desktop app:
  [AstroQore/vibe-bar-desktop](https://github.com/AstroQore/vibe-bar-desktop)
- Session kit:
  [AstroQore/agent-session-kit](https://github.com/AstroQore/agent-session-kit)
- Remote protocol:
  [AstroQore/vibebar-protocol](https://github.com/AstroQore/vibebar-protocol)
- Remote stack: `vibebar-probe`, `vibebar-relay`,
  `vibe-bar-web`, `vibe-bar-web-backend`

The remote stack was not changed in this work.
