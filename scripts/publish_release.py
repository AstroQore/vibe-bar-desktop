#!/usr/bin/env python3
"""Publish a draft release, after re-checking what publishing will mean.

`check_release_tag.py` runs when the tag is built, against the feed as it
stood then. That is early enough to catch a typo and too early to be the
last word: two tags on the same channel can both build while both drafts sit
unpublished, and both pass against the same head. Publish the newer draft and
then the older one, and the older is a published release the feed's semantic
max will never serve.

So the ordering is checked again here, against the documents the channel is
serving at this moment, and the draft is flipped in the same breath. Publishing
is the irreversible step; this is where the check belongs.

    scripts/publish_release.py v0.1.0-dev.1 [--dry-run]
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

import build_update_feed as feed
from check_release_tag import DOCUMENTS


def gh(*arguments: str) -> str:
    result = subprocess.run(["gh", *arguments], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"gh {' '.join(arguments)} failed: {result.stderr.strip()}")
    return result.stdout


def served(repository: str, document: str) -> str | None:
    """What that channel offers right now, or None if it offers nothing.

    A 404 is a channel with no document yet. Any other failure is not
    permission to publish: it would read as "no head" and let an older
    version through.
    """
    result = subprocess.run(
        [
            "gh", "api",
            "-H", "Accept: application/vnd.github.raw",
            f"repos/{repository}/contents/{document}?ref=updates",
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        if "404" in result.stderr or "Not Found" in result.stderr:
            return None
        raise SystemExit(f"could not read {document}: {result.stderr.strip()}")
    return json.loads(result.stdout)["version"]


def effective_head(document: str | None, releases: list[dict], include_dev: bool) -> str | None:
    """The highest version this channel is committed to, however recently.

    The served document lags publication by one workflow run, so it is not the
    whole answer: publish two drafts in a minute and both read the same stale
    document. The published releases are what the next run will read, so the
    head is the higher of the two.
    """
    candidates = [document] if document else []
    published = feed.head(releases, include_dev)
    if published is not None:
        candidates.append(published["tag_name"].removeprefix("v"))
    if not candidates:
        return None
    return max(candidates, key=feed.parse_version)


def check_publishable(tag: str, release: dict, head: str | None) -> str | None:
    """The reason not to publish this draft, or None to publish it."""
    version = tag.removeprefix("v")
    if feed.parse_version(version) is None:
        return f"{tag} is not a tag the update feed reads"
    if not release.get("isDraft"):
        return f"{tag} is already published"

    assets = {asset["name"]: asset for asset in release.get("assets", [])}
    targets = [p for p in feed.PLATFORMS if feed.signed_asset(assets, p) is not None]
    if not targets:
        return (
            f"{tag} has no signed updater artifacts, so the feed would skip it "
            f"however it is published"
        )

    if head is not None and feed.parse_version(head) >= feed.parse_version(version):
        return (
            f"{tag} cannot outrank {head}, which its channel serves now. "
            f"Publishing it would leave every subscriber on {head}."
        )
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("tag")
    parser.add_argument("--repo", default="AstroQore/vibe-bar-desktop")
    parser.add_argument("--dry-run", action="store_true")
    arguments = parser.parse_args()

    release = json.loads(
        gh("release", "view", arguments.tag, "--repo", arguments.repo,
           "--json", "isDraft,assets,tagName")
    )
    include_dev = feed.is_dev(arguments.tag.removeprefix("v"))
    listed = json.loads(
        gh("release", "list", "--repo", arguments.repo, "--limit", "200",
           "--json", "tagName,isDraft")
    )
    releases = [{"tag_name": r["tagName"], "draft": r["isDraft"]} for r in listed]
    head = effective_head(
        served(arguments.repo, DOCUMENTS[include_dev]), releases, include_dev
    )
    refusal = check_publishable(arguments.tag, release, head)
    if refusal is not None:
        print(refusal, file=sys.stderr)
        return 1

    assets = {asset["name"]: asset for asset in release.get("assets", [])}
    targets = [p for p in feed.PLATFORMS if feed.signed_asset(assets, p) is not None]
    print(f"{arguments.tag}: {len(targets)} signed targets — {', '.join(targets)}")
    if arguments.dry_run:
        print("dry run; not publishing")
        return 0

    gh("release", "edit", arguments.tag, "--repo", arguments.repo, "--draft=false")
    print(f"published {arguments.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
