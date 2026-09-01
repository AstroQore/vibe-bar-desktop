#!/usr/bin/env bash
# The version lives in three files. This is the one that checks they agree.
#
# `tauri.conf.json` is the source of truth: it is what Tauri bakes into the
# bundle and into the updater artifacts, so it is what an installed copy
# compares against. The other two are checked rather than generated — a
# generated Cargo.toml version is a worse trade than a check that fails loudly.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONF="$ROOT/apps/desktop/src-tauri/tauri.conf.json"
PKG="$ROOT/apps/desktop/package.json"
CARGO="$ROOT/Cargo.toml"

read_json_version() {
    python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$1"
}

truth="$(read_json_version "$CONF")"
package="$(read_json_version "$PKG")"
cargo="$(python3 - "$CARGO" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
workspace = re.search(r"\[workspace\.package\](.*?)(?=\n\[|\Z)", text, re.S)
section = workspace.group(1) if workspace else text
match = re.search(r'^\s*version\s*=\s*"([^"]+)"', section, re.M)
print(match.group(1) if match else "")
PY
)"

status=0
for pair in "package.json:$package" "Cargo.toml:$cargo"; do
    name="${pair%%:*}"
    value="${pair#*:}"
    if [[ "$value" != "$truth" ]]; then
        echo "version mismatch: tauri.conf.json says $truth, $name says ${value:-<none>}" >&2
        status=1
    fi
done

if [[ $status -ne 0 ]]; then
    echo >&2
    echo "All three carry the same version. tauri.conf.json is the one to edit;" >&2
    echo "the others follow it." >&2
    exit 1
fi

echo "version $truth, in all three files"
