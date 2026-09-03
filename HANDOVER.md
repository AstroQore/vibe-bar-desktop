# Handover: where Desktop is, and what is still open

This is the authoritative restart note for `AstroQore/vibe-bar-desktop` as of
2026-09-03. Read [AGENTS.md](AGENTS.md),
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md) before changing anything.

Do not infer full native parity from the amount of code that landed. The
[README's parity table](README.md#feature-parity) is the list of what differs;
this note says where the work stands and which lanes are open.

## 1. Where things stand

| Item | State |
| --- | --- |
| Desktop version | `0.1.0-dev.5` on `main`; pre-parity `0.x` versioning is intentional |
| Release channels | Main (`vX.Y.Z`) and Dev (`vX.Y.Z-dev.N`) tags build through `release.yml`; the updater feed lives on the `updates` branch. See [docs/RELEASE.md](docs/RELEASE.md) |
| Native baseline | `AstroQore/vibe-bar` `1.6.2`, Dev channel at `v1.6.2-dev.63` |
| Session kit | Desktop pins `agent-session-core` to the release tag `0.8.0`, which added the Rust `deletion` module both clients' rules now come from |
| Open feature PRs | None |

## 2. What Desktop `main` does now

| Area | Current implementation |
| --- | --- |
| Quota: live adapters | Thirteen. Codex, Claude and Grok from their CLI credential files; Cursor from the session Cursor.app keeps in its own state store; AntiGravity from the language server running on this machine; Alibaba, Copilot, Z.ai, MiniMax, Kilo, Kiro, OpenRouter and Warp from an explicit environment credential or the provider's own CLI. Credentials are read, never rewritten |
| Quota: the rest | Shared-cache read only, labelled as shared data. The browser-cookie providers wait on a cookie reader |
| Forecast | Per-bucket run-out and surplus from recorded observations, with a verdict that says when there is not enough evidence yet |
| Presentation | Shared display mode, provider visibility and order, plan labels, menu-bar fields, native app icons |
| Settings | The keys Desktop's own Settings presents — the whitelist is `shared::settings_writer::WRITABLE_KEYS`, and it is the safety boundary, so read it rather than a count written down here — are written back to the shared `settings.json` through the documented contract; a change the native app later makes is noticed and shown |
| Tray and lifecycle | Single instance, close-to-tray, explicit Quit, ready-gated first show with a watchdog, later tray startup, resume handling, launch at login |
| Updates | A daily check against the channel's feed, offered from the tray and installed on request |
| Mini | All seven native layouts, tray toggle, private geometry and visibility |
| Popover | A page per company with that company's quota cards and their forecasts, plus Misc providers and Machines. Reset history and service status live on the Overview, not on the company pages |
| Status | Public OpenAI, Claude, Google AI and Cursor status; Desktop's own last-good snapshot stays private |
| Usage and cost | Bounded local Codex, Claude and Gemini CLI scan into a priced aggregate that stays Desktop-private; honours the shared Cost Data privacy switch |
| Resets | Refill horizon, cycle cards with forecasts, the reset calendar and a run-out risk list |
| Skills | Install from a folder, adopt, toggle projections, uninstall with a snapshot, restore — through `SkillsService` and the native sync engine's rules. Repository installs, Discover and harness activation patches stay native |
| Sessions | Shared index when compatible: full-text search over transcript bodies. Without it, bounded local discovery matching title, session id and project directory only — a body search needs the index, which another client writes. Paged transcripts with find, resume command, and deletion through the kit's fenced deleter |
| First run | The native setup assistant, step for step, marking completion in the shared settings |
| MCP | Six read-only tools over stdio: `quota.get`, `sessions.list`, `sessions.search`, `status.get`, `pricing.effective`, `cost.snapshot`. Quota, status and cost answer from what the last run recorded; `pricing.effective` returns this build's compiled table, which needs no prior run and is as current as the binary; the session tools read the shared index, or discover locally at request time when it is absent |
| Writes | Six authorized write domains and nothing else — shared settings and the quota cache, the Control Center allow-list, whole-session deletion under a harness's own directory, the skill library with its managed app directories, and the OS login-item registration behind launch at login. Installing an update is the seventh and the loudest: at the person's explicit yes, the updater replaces the application itself. See [AGENTS.md](AGENTS.md) rule 1 |
| Platforms | The core crate is tested on macOS, Linux and Windows on every pull request; the GUI has had its end-to-end pass on macOS only |

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
- PR [#15](https://github.com/AstroQore/agent-session-kit/pull/15) merged and
  released as `0.7.0`: Swift and Rust became peer implementation lanes under
  `implementations/`, with `contracts/` holding the facts both must honour.
  Desktop pinned that tag at the time; the pin is `0.8.0` now, which
  added the Rust `deletion` module.

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
supersession note.

### Since that checkpoint

The pause ended. In order: the popover's company pages and Overview, the
remaining Mini layouts, Usage Stats, the reset calendar, the shared settings
writer and its contract, the release pipeline and updater feed, launch at
login, the daily update check, session deletion (with kit `0.8.0`, whose Rust
`deletion` module both clients now share), the first-run assistant, the
porcelain design language and the pages migrated onto it, the Skills manager,
the README gallery and its capture script, and the Cursor, Grok and
AntiGravity adapters.

## 4. How this is verified

Every pull request runs the core crate's tests and clippy on macOS, Linux and
Windows, plus the workspace tests, the frontend typecheck and build, and the
Tauri build on macOS. Locally the same four commands are the gate; see
[AGENTS.md](AGENTS.md).

The original integration head `391d926...` passed:

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

`examples/inspect.rs` had the same leak and no longer does: it resolves its
scan root from the shape of the path it was given, so a synthetic
`<home>/.vibebar` scans that synthetic home while an explicit real data root
scans the user's actual home.

Hosted CI was blocked at the time of the pause: every job ended in about two
seconds because the account's Actions spending had to be raised. The
repository is public now, so CI runs, and the first real cross-platform run
immediately found three things the macOS-only evidence had hidden — a Windows
fail-closed bug in the cost scanner, two assertions that only held on Unix,
and a float round-trip in a test fixture. All are fixed on `main`.

For any future data-layer change, also repeat the real-root immutability check
from [AGENTS.md](AGENTS.md). The only tolerated read-only side effect remains
the SQLite `session_index.sqlite3-shm` mtime.

## 5. Invariants that must not be weakened

1. Write outside `<data root>/client/desktop/` only in the authorized
   domains. Five of them — shared settings, the quota cache, the Control
   Center allow-list, session deletion and the skill library — go through a
   documented writer on the terms [AGENTS.md](AGENTS.md) rule 1 sets out, and
   most are not under the Vibe Bar root at all. The other two have no such
   writer and cannot: autostart and the updater hand off to the platform at
   an explicit yes, one registering a login item and one replacing the
   application itself.
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
   particular local-data exposure or writer role. The stdio server never
   refreshes a provider, scans usage, or writes configuration. It has exactly
   one storage side effect, and it is the privacy switch doing its job:
   constructing the cost engine with Cost Data privacy on deletes this
   client's own saved snapshot, before it is read rather than after, because
   the setting is a statement about holding that data at all.
10. On a demo root, quota, sessions, cost, status, geometry, and all scans must
    stay inside that demo root. Both the app runtime and
    `examples/inspect.rs` comply.

## 6. Deliberately not done

These items were not forgotten. They either require explicit authority or a
larger product/architecture decision.

### Requires explicit user authority

- Reading browser cookies, and the WebView login windows that go with them.
  This is the gate in front of Gemini, the cookie-slot plans, and the web
  routes of providers that already have a CLI route here.
- Volc Agent AK/SK and other provider credential stores.
- Anything that installs a release over the user's current app.

### Still requires engineering

- A safe cross-client writer architecture for the stores that still have no
  writer: role leases, fail-closed migrations, and removal of destructive
  native schema recovery.
- The remaining provider and credential matrix on macOS, Windows, and Linux.
- All-harness usage scanning, the per-request ledger, multi-source pricing,
  cost history, fill/forecast timelines, and subscription-cycle inference.
- Multiple Mini windows, the layout editor, the arrangeable Overview, the
  menu-bar field editor's style scopes, and the provider credential panes.
- Skills: repository installs, Discover, and the harness activation patches.
- Native's broader 12-tool MCP surface and socket ownership — most of those
  tools need a scan, a refresh, or a store Desktop does not keep, so each one
  is a decision about what the stdio contract may do, not a port.
- Remote probe sync.
- Windows and Linux GUI validation and launch integration.

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
  QA can leak real session metadata into the UI. App state supplies both, and
  `examples/inspect.rs` derives the scan home from the path shape rather than
  from the write-suppression flag, so an explicit real data root still scans
  the user's actual home.
- A disappearing MCP peer must not terminate the app through SIGPIPE. Native
  dev.53+ and the session kit include that socket hardening; preserve it in any
  future transport work.

## 8. Local cleanup and recovery notes

Task worktrees live under `.agents/worktrees/` and are removed once their pull
request is settled; `git worktree list` should show the main worktree and
whatever is genuinely in flight, nothing more. The Kit worktrees
`feat/mcp-request-context` and `docs/used-by` belong to other work and are
left alone.

From the original checkpoint: three dirty old Desktop worktrees contained task-time rustfmt residue; one also
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
2. Confirm the actual current state; do not trust this note after branches
   have moved:

   ```sh
   git status --short --branch
   git log -1 --oneline
   git worktree list
   gh pr list --state open
   ```

3. Re-read the native checkout and note any drift from the baseline in
   section 1.
4. Rerun CI before treating Windows/Linux or storage-contract parity as
   current evidence; the macOS-only local run has hidden real bugs before.
5. Ask for or confirm authority before choosing any item in section 6.
6. Work in a fresh `feat/`, `fix/`, `docs/`, or `test/` worktree from
   current `main`; never edit the user's main worktree directly.
7. Run the full Rust, clippy, TypeScript, and build verification set. For data
   changes, run the shared-root before/after check. For UI changes, verify the
   installed or built app with Computer rather than relying on unit tests.
8. Merge normally through a PR, and do not tag or release without explicit
   instruction. Two process traps, both learned the hard way: never point an
   automatic merge at a pull request whose base branch you are still
   rebasing, and never merge with `--delete-branch` while another pull
   request uses that branch as its base — GitHub closes the child rather than
   retargeting it. Remove the task worktree after the pull request settles.

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
