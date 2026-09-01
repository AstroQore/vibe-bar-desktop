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
| Main | `X.Y.Z` | `v X.Y.Z` |
| Dev | `X.Y.Z-dev.N` | `v X.Y.Z-dev.N` |

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
updates branch
  latest-main.json
  latest-dev.json
```

The app reads `updateChannel` from the shared `settings.json` — the same key
the native client uses, so choosing Dev in either window sets it for both. It
is a read; `settings_writer`'s whitelist does not need to grow.

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

## Signing

`tauri signer generate`. The public key goes in `tauri.conf.json`; the private
key and its password are repository secrets. This is separate from code
signing — Desktop is ad-hoc signed like the native client, and an unsigned
update that the updater accepts is still a binary the OS will question.

**Generate the key before the first release, not after.** The public key is
baked into every build, so a build that shipped without it can never be
updated in place.

## Platforms

macOS only to begin with, `aarch64` and `x86_64`. The core crate is tested on
Windows and Linux, but the GUI has only had a macOS pass, and shipping an
updater to a platform nobody has run is how you find out that the tray does
not work on a machine you cannot reach.

## The pipeline

1. **Bump** `tauri.conf.json` on a `release/<version>` branch of its own.
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
- the Dev tag's `-dev.N` is present and higher than the last Dev tag for that
  version;
- the updater artifact exists and its signature verifies against the public key
  in the config.

## What is deliberately not in the first version

- **Windows and Linux updates.** They follow the GUI pass, not before.
- **Delta updates.** Sparkle does not use them here either.
- **A "check now" button.** Native's daily ask-first check is the behaviour to
  match, and it needs the settings pane before it needs a button.
- **Rollback.** The channel switch covers it: a Dev user who hits a bad build
  moves to Main and stays there until the next Main release. Automatic
  rollback needs a signal that a build is bad, and there is nothing that
  produces one.

## The one-way doors

Worth naming, because these cannot be undone after a version reaches someone:

- **The signing key.** Baked into every build.
- **The version scheme.** An updater compares against what shipped; changing
  the scheme later means some installs can never see another update.
- **The endpoint URLs.** Same reason.

Everything else — how the feed is built, which platforms, what the release
notes say — can change freely.
