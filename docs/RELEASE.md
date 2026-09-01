# Releases and in-app updates — the plan

**Nothing here is implemented yet.** Desktop has `ci.yml` and nothing else: no
release workflow, no tags, no updater configuration, and no signing key. This
is the design to build, written down first because the pieces constrain each
other and half of them are one-way doors once a version has shipped to
someone.

The native client's pipeline is the reference. Where this differs, it is
because Sparkle and the Tauri updater differ, and the difference is called out.

## What the native client does

Read from the repository rather than from memory:

| | |
| --- | --- |
| Version SSOT | `Resources/Info.plist` — `CFBundleShortVersionString` (1.5.0) and `CFBundleVersion` (58) |
| Tags | `v<version>` for Main, `v<version>-dev.<CFBundleVersion>` for Dev |
| Gate | `Scripts/release_app.sh` refuses a tag that disagrees with the plist, and refuses a bundle carrying an `app-sandbox` entitlement |
| Feed | one `appcast.xml` on the `updates` branch holding **both** channel heads |
| Channel marking | Main entries carry no `<sparkle:channel>`; Dev entries carry `<sparkle:channel>dev</sparkle:channel>` |
| Ordering | `sparkle:version` — the build number, monotonic across both channels |
| Publishing | `publish-update-feed.yml`, serialized by `concurrency`, and it **rebuilds both heads from the published releases** rather than appending to what is there |
| User preference | `updateChannel` in the shared `settings.json` |

Two of these are load-bearing and easy to get wrong:

- **The feed is rebuilt, not appended.** Every run reconstructs both heads from
  what is actually published, so a lost or reordered run cannot leave the feed
  describing a release that does not exist.
- **A Dev subscriber sees both channels.** The channel tag filters what a
  client is *allowed* to see; the build number decides what it *takes*. That is
  why Dev users get a Main release the moment its build number is higher —
  today Main is 1.5.0 (58) while Dev is 1.4.1 (57).

## Version SSOT: one file, not three

Desktop carries `0.1.0` in `apps/desktop/src-tauri/tauri.conf.json`,
`apps/desktop/package.json`, and the workspace `Cargo.toml`. Three copies of
one fact, and nothing checks that they agree.

`tauri.conf.json` is the one that matters: it is what Tauri bakes into the
bundle and into the updater artifacts, so it is the truth. The other two are
checked against it in CI and the release script refuses to build when they
disagree. Not generated — a generated `Cargo.toml` version is a worse trade
than a check that fails loudly.

## Channels and version numbers

Sparkle separates the marketing version from a monotonic build number. Tauri's
updater compares **semver** and has no second number, so the build counter
moves into the prerelease field:

| Channel | Version | Tag |
| --- | --- | --- |
| Main | `X.Y.Z` | `vX.Y.Z` |
| Dev | `X.Y.Z-dev.N` | `vX.Y.Z-dev.N` |

Semver already orders these the way the pipeline needs:
`0.2.0-dev.7 < 0.2.0-dev.8 < 0.2.0`. Dev builds are previews *of the next
release*, which is exactly how the native client uses them — `v1.4.1-dev.50`
through `v1.4.1-dev.56`, then `v1.4.1`.

**This differs from native in one visible way, and it is worth stating.**
Native's Dev head can sit behind Main, because one monotonic build number
spans both channels. Under semver it cannot: a Dev user on `1.5.0-dev.3` is
offered `1.5.0` when it ships, and after that the next thing they see is
`1.6.0-dev.1`. The result for the user is the same — always the newest thing
their channel permits — but the mechanism is ordering rather than filtering,
so there is no equivalent of "Dev is currently older than Main".

## The feed

Tauri's updater fetches JSON per platform and has no channel concept, so the
channel becomes the endpoint:

```
https://raw.githubusercontent.com/AstroQore/vibe-bar-desktop/updates/latest-main.json
https://raw.githubusercontent.com/AstroQore/vibe-bar-desktop/updates/latest-dev.json
```

Written out because this section named them as one of the three things that
cannot be changed later, and "an `updates` branch and two filenames" is not a
URL — it leaves the next person to pick between raw branch content, Pages, or
a domain, and two independently built releases could then embed endpoints that
never see each other's updates.

Raw branch content, the same host and shape the native client's
`SUFeedURL` uses:
`raw.githubusercontent.com/AstroQore/vibe-bar/updates/appcast.xml`. It needs no
Pages build and no domain that can lapse.

The app reads `updateChannel` from the shared `settings.json` — the same key
the native client uses, so choosing Dev in one window applies to both.

**Desktop has to be able to set it too, not only read it.** On a machine with
no native client there is otherwise no way into the Dev channel at all: the key
is not in `settings_writer::WRITABLE_KEYS` and Desktop's Settings presents
three controls, none of them this one. So the updater work includes a channel
control in Settings and `updateChannel` in the whitelist — which is the rule
that list already states, that a key may be writable only when a control
submits it.

Each file is the Tauri updater's shape:

```json
{
  "version": "0.2.0",
  "notes": "…",
  "pub_date": "2026-09-01T00:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "…", "url": "https://github.com/…/Vibe.Bar.Desktop_0.2.0_aarch64.app.tar.gz" }
  }
}
```

Rebuilt from the published releases on every run, serialized with
`concurrency`, for the same reason native rebuilds its appcast: a feed that is
appended to can end up describing a release that was deleted or never
finished.

**Which release goes in which document**, since each one can only name a
single version:

| Document | Considers | Takes |
| --- | --- | --- |
| `latest-main.json` | Main releases only | the greatest semver among them |
| `latest-dev.json` | Main **and** Dev releases | the greatest semver among them |

The second row is the whole point and is easy to get backwards. The obvious
builder — Main takes the newest stable, Dev takes the newest prerelease —
strands every Dev user the moment a release ships: someone on `0.2.0-dev.8`
would never be offered `0.2.0`, because it is not a prerelease. Dev is *the
channel that sees more*, exactly as a Dev subscriber to native's appcast sees
both channel tags, and semver ordering then does the rest.

## Signing

`tauri signer generate`. The public key goes in `tauri.conf.json`; the private
key and its password are repository secrets. This is separate from code
signing — Desktop is ad-hoc signed like the native client, and an unsigned
update that the updater accepts is still a binary the OS will question.

**Generate the key before the first release, not after.** The public key is
baked into every build, so a build that shipped without it can never be
updated in place.

## Platforms

Six targets from the first release: macOS, Windows and Linux, each on x86_64
and arm64.

| Target | Runner | Bundle |
| --- | --- | --- |
| `darwin-aarch64` | `macos-latest`, `--target aarch64-apple-darwin` | `.app` + `.dmg`, updater artifact `.app.tar.gz` |
| `darwin-x86_64` | `macos-latest`, `--target x86_64-apple-darwin` | same |
| `windows-x86_64` | `windows-latest` | NSIS `.exe`, updater artifact `.nsis.zip` |
| `windows-aarch64` | `windows-latest`, `--target aarch64-pc-windows-msvc` | same |
| `linux-x86_64` | `ubuntu-22.04` | `.AppImage`, updater artifact `.AppImage.tar.gz` |
| `linux-aarch64` | `ubuntu-22.04-arm` | same |

Linux arm64 builds on an arm runner rather than cross-compiling: the webkit2gtk
and appindicator dependencies make a cross build far more work than renting the
right machine.

**22.04 on both architectures**, because the glibc a binary is built against is
the floor of what can run it — and an AppImage does not change that, since it
bundles the application and not the C library. Building arm64 on a newer runner
would hand every arm64 user still on 22.04 a binary that will not start.

Every target passes an explicit `--target`, including the arm64 macOS one.
`macos-latest` is an alias: the day it points at an Intel runner, a row relying
on the host architecture silently produces a second x86_64 bundle and the
arm64 updater artifact goes missing.

### What each platform still needs

- **Linux system packages** on the runner: `libwebkit2gtk-4.1-dev`,
  `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`.
- **AppImage is the only updatable Linux bundle.** Tauri's updater cannot
  replace a `.deb` or `.rpm` in place. Either those are not shipped, or they
  ship as one-time downloads that will never update themselves — which is a
  product decision, not a build one.
- **Windows code signing.** Unsigned installers meet SmartScreen. A
  certificate is a cost and a decision; without one the first-run experience
  is a warning dialog.
- **macOS stays ad-hoc signed**, as the native client is. Downloads carry the
  quarantine flag and need the usual first-open dance.

### The honest risk

The GUI has only ever run on macOS. The core crate is tested on all three
platforms and the credential and scan paths are written portably — but the
tray, the undecorated always-on-top mini window, `data-tauri-drag-region`, and
window placement have never been exercised on Windows or Linux. Some Linux
desktops have no tray at all, and this is a tray application.

Shipping there means shipping something no one has run. That is a reason to
start on the Dev channel and to say so in the release notes, not a reason to
hold the other four targets back.

### What does work off macOS

Worth stating precisely, because "macOS-only features" sounds worse than it
is. `credentials/keychain.rs` returns `None` off macOS, so anything that
*only* lives in the macOS Keychain is unavailable. But Codex and Claude
credentials are also read from files — `~/.codex/auth.json`,
`~/.claude/.credentials.json`, `~/.config/claude/.credentials.json` — and
`home_directory()` falls back to `USERPROFILE`. Those providers, and the local
usage scan that reads the same session files, work on all three platforms.

## The pipeline

1. **Bump all three version files** — `tauri.conf.json`,
   `apps/desktop/package.json`, the workspace `Cargo.toml` — **and regenerate
   `Cargo.lock`**, on a `release/<version>` branch of its own. The lockfile
   records the workspace crates' own versions, so a bump without it leaves a
   tagged tree where `--locked` fails and an unlocked build silently rewrites
   the lockfile inside the runner instead. All three, because the gate below
   refuses a build where they disagree, and the SSOT section chose checking
   over generating: a release prepared by bumping only the source of truth
   would be rejected by its own pipeline.
   Never in a feature PR: tags are first-come, and two agents bumping in
   parallel means one of them loses the tag it just wrote.
2. **Merge**, then **tag** — `vX.Y.Z` or `vX.Y.Z-dev.N`.
3. **`release.yml`** builds the bundles, signs the updater artifacts, and opens
   a **draft** release. Draft, so the assets can be checked before anyone's
   updater sees them.
4. **Verify before publishing**: the SHA-256 of each asset independently, the
   version inside the bundle, and that the bundle launches.
5. **Publish.** Main releases are marked latest; Dev releases are prereleases.
6. **`publish-update-feed.yml`** rebuilds both JSON files from the published
   releases.

## The gate

`Scripts/release_app.sh`, mirroring `release_app.sh` on the native side:

- the tag matches `tauri.conf.json`;
- the three version files agree;
- the Dev tag's `-dev.N` is present;
- **the candidate outranks the head its channel currently serves**, compared
  as a whole semver against the same set that document considers — Main
  against Main, Dev against Main and Dev together.

  Counting `-dev.N` upwards is not enough on its own. After `0.2.0` ships,
  `0.2.0-dev.9` has a higher counter than every earlier Dev tag and still
  orders *below* `0.2.0`: publishing it would put a version in the Dev feed
  that no current subscriber can install. An older Main version passes every
  other check here for the same reason.
- the updater artifact exists and its signature verifies against the public key
  in the config.

## What the first version includes beyond the pipeline

- A **channel control** in Desktop's Settings, and `updateChannel` in
  `settings_writer::WRITABLE_KEYS`. Without it a standalone install cannot
  reach the Dev channel at all.
- The **version check** in CI, so the three copies cannot drift apart between
  releases rather than only at one.

## What is deliberately not in the first version

- **A Windows signing certificate.** Until there is one, Windows installers
  meet SmartScreen. That is a purchase and a decision, not a build step.
- **Delta updates.** Sparkle does not use them here either.
- **A "check now" button.** Native's daily ask-first check is the behaviour to
  match, and it needs the settings pane before it needs a button.
- **Rollback.** There is none, and the channel switch is not one — which is
  worth stating because it is the obvious thing to assume. The updater only
  offers versions *newer* than the installed one, so someone on a bad
  `0.3.0-dev.5` who switches to Main is offered nothing at all until a Main
  release passes that version. They are on the bad build until then.

  The recovery in the first version is to download and reinstall an earlier
  release by hand. Anything better needs a downgrade path in the updater and a
  signal that a build is bad, and neither exists.

## The one-way doors

Worth naming, because these cannot be undone after a version reaches someone:

- **The signing key.** Baked into every build.
- **The version scheme.** An updater compares against what shipped; changing
  the scheme later means some installs can never see another update.
- **The endpoint URLs.** Same reason.

Everything else — how the feed is built, which platforms, what the release
notes say — can change freely.
