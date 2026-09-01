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


def all_releases(repository: str) -> list[dict]:
    """Every release, drafts included, as one list.

    `--slurp` returns an array *of pages*; flattening it is the whole
    difference from feeding `--paginate` straight to a parser, and it does not
    depend on how a given `gh` chooses to join them.

    The by-tag endpoint would be one request instead of a page's worth, and it
    404s on a draft — which is the only kind of release this is ever asked
    about.
    """
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{repository}/releases"))
    return [release for page in pages for release in page]


def with_manifest(repository: str, release: dict) -> dict:
    """The release, with the bundler's manifest read in.

    The API names assets but does not inline their contents, and the contents
    are the whole point.
    """
    for asset in release.get("assets", []):
        if asset["name"] == feed.MANIFEST_ASSET:
            asset["_contents"] = gh(
                "api", "-H", "Accept: application/octet-stream",
                f"repos/{repository}/releases/assets/{asset['id']}",
            )
    return release


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


def check_publishable(
    tag: str, release: dict, head: str | None, allow_missing: bool = False
) -> str | None:
    """The reason not to publish this draft, or None to publish it."""
    version = tag.removeprefix("v")
    if feed.parse_version(version) is None:
        return f"{tag} is not a tag the update feed reads"
    if not release.get("draft"):
        return f"{tag} is already published"

    platforms = feed.platforms_for(release)
    if not platforms:
        return (
            f"{tag} carries no usable {feed.MANIFEST_ASSET}, so the feed would "
            f"skip it however it is published"
        )
    missing = feed.missing_platforms(platforms)
    if missing and not allow_missing:
        return (
            f"{tag} is missing {', '.join(missing)}. Those users would be "
            f"offered nothing. Pass --allow-missing to publish anyway."
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
    parser.add_argument(
        "--allow-missing",
        action="store_true",
        help="publish even though some targets have no updater entry",
    )
    arguments = parser.parse_args()

    # One listing, used twice: to find the draft — the by-tag endpoint does not
    # return drafts — and to see what the channel has already committed to.
    releases = all_releases(arguments.repo)
    release = next(
        (r for r in releases if r["tag_name"] == arguments.tag),
        None,
    )
    if release is None:
        raise SystemExit(f"{arguments.tag} has no release")
    release = with_manifest(arguments.repo, release)
    include_dev = feed.is_dev(arguments.tag.removeprefix("v"))
    head = effective_head(
        served(arguments.repo, DOCUMENTS[include_dev]), releases, include_dev
    )
    refusal = check_publishable(
        arguments.tag, release, head, allow_missing=arguments.allow_missing
    )
    if refusal is not None:
        print(refusal, file=sys.stderr)
        return 1

    platforms = feed.platforms_for(release)
    named = sorted(p for p in platforms if p in feed.EXPECTED_PLATFORMS)
    print(f"{arguments.tag}: {len(named)} targets — {', '.join(named)}")
    for absent in feed.missing_platforms(platforms):
        print(f"  missing: {absent}")
    if arguments.dry_run:
        print("dry run; not publishing")
        return 0

    gh("release", "edit", arguments.tag, "--repo", arguments.repo, "--draft=false")
    print(f"published {arguments.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
