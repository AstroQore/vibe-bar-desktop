#!/usr/bin/env python3
"""The lockfile records the workspace crates' own versions.

A bump that leaves it behind produces a tagged tree where `--locked` fails and
an unlocked build rewrites the lockfile inside the runner instead — so the
tagged source and what was built from it disagree.
"""

import re
import sys
from pathlib import Path

WORKSPACE_CRATES = {"vibe-bar-desktop", "vibebar-desktop-core"}


def main() -> int:
    lockfile, wanted = Path(sys.argv[1]), sys.argv[2]
    text = lockfile.read_text()
    stale = [
        f"{name} {version}"
        for name, version in re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', text)
        if name in WORKSPACE_CRATES and version != wanted
    ]
    if stale:
        print(f"Cargo.lock still has {', '.join(stale)}, expected {wanted}", file=sys.stderr)
        print("Run `cargo update --workspace` and commit Cargo.lock.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
