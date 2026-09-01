#!/usr/bin/env python3
"""Build `latest-main.json` and `latest-dev.json` from published releases.

Rebuilt from what is published, never appended to: a feed that accumulates can
end up describing a release that was deleted or never finished. The native
client's appcast workflow reconstructs both heads for the same reason.

Each document names exactly one version, and which one is the part that is
easy to get backwards:

    latest-main.json   considers Main releases only
    latest-dev.json    considers Main *and* Dev releases

Dev is the channel that sees more. The obvious alternative — Main takes the
newest stable, Dev the newest prerelease — strands every Dev user the moment a
release ships, because a stable version is not a prerelease and would never be
offered to them.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# The manifest the bundler uploads next to the bundles, holding a signature and
# a URL per target. Reading it is the whole platform rule: this file used to
# rebuild the same table out of asset names, and got three of six suffixes
# wrong — `.nsis.zip` and `.AppImage.tar.gz` for artifacts this Tauri actually
# calls `-setup.exe` and `.AppImage`. The result was a release that published
# green and would have offered updates to macOS only.
#
# Those names are the bundler's to change, and this is the bundler telling us
# what it chose. Deriving them again is the same fact in two places, with the
# copy that is wrong staying silent.
MANIFEST_ASSET = "latest.json"

# The targets a complete release covers. Not used to *find* anything — only to
# say what is missing, since the manifest cannot report a build that never ran.
EXPECTED_PLATFORMS = frozenset(
    {
        "darwin-aarch64",
        "darwin-x86_64",
        "windows-x86_64",
        "windows-aarch64",
        "linux-x86_64",
        "linux-aarch64",
    }
)

SEMVER = re.compile(
    r"^(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(?:-dev\.(?P<dev>\d+))?$"
)


def parse_version(tag: str):
    """A sortable key, or None for a tag this scheme does not produce.

    A release without `-dev.N` outranks every prerelease of the same version,
    which is what semver says and what makes the Dev document work: a Dev user
    on 0.2.0-dev.8 is offered 0.2.0 because 0.2.0 sorts above it.
    """
    match = SEMVER.match(tag.removeprefix("v"))
    if not match:
        return None
    dev = match.group("dev")
    return (
        int(match.group("major")),
        int(match.group("minor")),
        int(match.group("patch")),
        0 if dev is not None else 1,
        int(dev) if dev is not None else 0,
    )


def is_dev(tag: str) -> bool:
    return "-dev." in tag


def platforms_for(release: dict) -> dict:
    """The updater entries, as the bundler wrote them.

    The manifest's contents are fetched alongside the release and stashed on
    the asset as `_contents`, the same way signatures used to be. An entry
    without both a signature and a URL is dropped rather than guessed at: an
    entry the client cannot use is worse than a channel that stays quiet.
    """
    manifest = next(
        (
            asset
            for asset in release.get("assets", [])
            if asset["name"] == MANIFEST_ASSET and asset.get("_contents")
        ),
        None,
    )
    if manifest is None:
        return {}
    try:
        platforms = json.loads(manifest["_contents"])["platforms"]
    except (json.JSONDecodeError, KeyError, TypeError):
        return {}
    return {
        name: {"signature": entry["signature"], "url": entry["url"]}
        for name, entry in sorted(platforms.items())
        if isinstance(entry, dict) and entry.get("signature") and entry.get("url")
    }


def missing_platforms(platforms: dict) -> list[str]:
    """Targets a full release would carry that this one does not.

    An alias like `darwin-aarch64-app` sits beside the plain key, so presence
    is judged on the plain ones only.
    """
    return sorted(EXPECTED_PLATFORMS - set(platforms))


def document(release: dict) -> dict:
    return {
        "version": release["tag_name"].removeprefix("v"),
        "notes": (release.get("body") or "").strip(),
        "pub_date": release["published_at"],
        "platforms": platforms_for(release),
    }


def head(releases: list[dict], include_dev: bool):
    eligible = [
        release
        for release in releases
        if not release.get("draft")
        and parse_version(release["tag_name"]) is not None
        and (include_dev or not is_dev(release["tag_name"]))
    ]
    if not eligible:
        return None
    return max(eligible, key=lambda r: parse_version(r["tag_name"]))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("releases", type=Path, help="`gh release list --json ...` output")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    releases = json.loads(args.releases.read_text())
    args.out.mkdir(parents=True, exist_ok=True)

    for name, include_dev in (("latest-main.json", False), ("latest-dev.json", True)):
        chosen = head(releases, include_dev)
        if chosen is None:
            print(f"{name}: nothing published yet", file=sys.stderr)
            continue
        body = document(chosen)
        if not body["platforms"]:
            print(f"{name}: {chosen['tag_name']} has no signed artifacts", file=sys.stderr)
            continue
        (args.out / name).write_text(json.dumps(body, indent=2) + "\n")
        print(f"{name}: {chosen['tag_name']} ({len(body['platforms'])} platforms)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
