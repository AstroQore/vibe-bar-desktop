#!/usr/bin/env python3
"""Refuse a tag that would produce a release nobody is ever offered.

There are two ways to get one, and neither shows up in a build log.

A tag outside the grammar `build_update_feed.py` parses: `v0.2.0-rc.1` is
valid SemVer, builds green, publishes, and is skipped by both documents
forever.

A tag that cannot outrank the document it would land in: after `0.2.0` ships,
`0.2.0-dev.9` has a higher counter than every earlier Dev tag and still orders
*below* `0.2.0`, so the Dev document keeps serving `0.2.0` and no subscriber
sees the new build. An older Main version fails the same way.

Both end in a release with signed assets that no installed copy can reach, and
a tag is not something to take back. So this runs before the build.

The grammar is imported rather than restated: the whole failure being checked
for is this file and the feed builder disagreeing about what a version is.
"""

import argparse
import json
import sys
from pathlib import Path

import build_update_feed as feed

# Which document a tag lands in — and therefore which head it has to beat.
# Dev is compared against `latest-dev.json`, which already holds the newer of
# the two channels, so a Dev tag is measured against Main and Dev together
# without this file having to know that rule twice.
DOCUMENTS = {True: "latest-dev.json", False: "latest-main.json"}


def served_version(document: Path) -> str | None:
    """The version that document currently offers, or None if there is none.

    A missing document is the first release into that channel. A present one
    that cannot be read is not: our own workflow wrote it, so something is
    wrong, and guessing "no head" would let the release through.
    """
    if not document.exists():
        return None
    try:
        version = json.loads(document.read_text())["version"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise SystemExit(f"{document} is not a feed document this can read: {error}")
    if feed.parse_version(version) is None:
        raise SystemExit(f"{document} serves {version!r}, which is not a version")
    return version


def check(tag: str, configured: str, feed_dir: Path | None) -> str | None:
    """The reason to refuse this tag, or None to build it."""
    version = tag.removeprefix("v")

    if feed.parse_version(version) is None:
        return (
            f"{tag} is not a tag this project releases. The update feed reads "
            f"X.Y.Z and X.Y.Z-dev.N and skips everything else, so this would "
            f"publish assets that no installed copy is ever offered."
        )

    if version != configured:
        return f"{tag} does not match the configured version {configured}"

    if feed_dir is None:
        return None
    document = feed_dir / DOCUMENTS[feed.is_dev(version)]
    head = served_version(document)
    if head is None:
        return None
    if feed.parse_version(head) >= feed.parse_version(version):
        return (
            f"{tag} cannot outrank {head}, which {document.name} serves now. "
            f"Publishing it would leave every subscriber on {head}."
        )
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("tag", help="the tag being built, with or without the v")
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("apps/desktop/src-tauri/tauri.conf.json"),
        help="the version's source of truth",
    )
    parser.add_argument(
        "--feed-dir",
        type=Path,
        help="a checkout of the updates branch; omit to skip the channel comparison",
    )
    args = parser.parse_args()

    configured = json.loads(args.config.read_text())["version"]
    refusal = check(args.tag, configured, args.feed_dir)
    if refusal is not None:
        print(refusal, file=sys.stderr)
        return 1
    print(f"building {args.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
