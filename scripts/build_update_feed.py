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

# How each target's updater artifact is spelled. Both halves matter: the
# suffix says which bundle kind it is, and the token says which architecture —
# matching on the suffix alone makes one macOS build answer for both, because
# `.app.tar.gz` is what arm64 and x86_64 are each called.
PLATFORMS = {
    "darwin-aarch64": ("_aarch64", ".app.tar.gz"),
    "darwin-x86_64": ("_x64", ".app.tar.gz"),
    "windows-x86_64": ("_x64-setup", ".nsis.zip"),
    "windows-aarch64": ("_arm64-setup", ".nsis.zip"),
    "linux-x86_64": ("_amd64", ".AppImage.tar.gz"),
    "linux-aarch64": ("_aarch64", ".AppImage.tar.gz"),
}

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
    """The updater entry per target, from the assets and their signatures.

    A target whose signature is missing is left out rather than guessed at: an
    entry without one is an update the client will refuse, and a feed that
    promises it is worse than one that stays quiet.
    """
    assets = {asset["name"]: asset for asset in release.get("assets", [])}
    out = {}
    for platform in PLATFORMS:
        name = signed_asset(assets, platform)
        if name is None:
            continue
        out[platform] = {
            "signature": assets[f"{name}.sig"]["_contents"],
            "url": assets[name]["browser_download_url"],
        }
    return out


def signed_asset(assets, platform: str) -> str | None:
    """The updater bundle for a target, if it is there with its signature.

    Shared with the publish gate so that "this release will appear in the
    feed" and "this release may be published" are decided by one rule.
    """
    token, suffix = PLATFORMS[platform]
    for name in assets:
        if name.endswith(suffix) and token in name and f"{name}.sig" in assets:
            return name
    return None


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
