"""The feed's selection rule, which is the part that is easy to get backwards.

Run with `python3 -m pytest scripts/tests` or plain `python3 scripts/tests/test_update_feed.py`.
"""

import importlib.util
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("feed", ROOT / "scripts/build_update_feed.py")
feed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feed)


MANIFEST = {
    "version": "0.0.0",
    "platforms": {
        name: {"signature": f"sig-{name}", "url": f"https://example.invalid/{name}"}
        for name in (
            "darwin-aarch64", "darwin-x86_64",
            "windows-x86_64", "windows-aarch64",
            "linux-x86_64", "linux-aarch64",
        )
    },
}


def manifest_asset(platforms=None):
    """The asset the bundler uploads, in the shape the workflow hands over."""
    import json as _json

    body = dict(MANIFEST)
    if platforms is not None:
        body = {**MANIFEST, "platforms": platforms}
    return {"name": "latest.json", "_contents": _json.dumps(body)}


def release(tag, *, draft=False, assets=None):
    return {
        "tag_name": tag,
        "draft": draft,
        "published_at": "2026-09-01T00:00:00Z",
        "body": "",
        "assets": [manifest_asset()] if assets is None else list(assets),
    }


def test_main_ignores_prereleases():
    releases = [release("v0.1.0"), release("v0.2.0-dev.8")]
    assert feed.head(releases, include_dev=False)["tag_name"] == "v0.1.0"


def test_dev_takes_a_stable_release_once_it_ships():
    """The failure this rule exists to prevent.

    A builder that gave Dev the newest *prerelease* would leave someone on
    0.2.0-dev.8 there for good: 0.2.0 is not a prerelease, so they would never
    be offered it.
    """
    releases = [release("v0.2.0-dev.8"), release("v0.2.0")]
    assert feed.head(releases, include_dev=True)["tag_name"] == "v0.2.0"


def test_dev_takes_the_next_preview_after_that():
    releases = [release("v0.2.0"), release("v0.3.0-dev.1")]
    assert feed.head(releases, include_dev=True)["tag_name"] == "v0.3.0-dev.1"
    assert feed.head(releases, include_dev=False)["tag_name"] == "v0.2.0"


def test_a_preview_orders_below_its_own_release():
    assert feed.parse_version("v0.2.0-dev.9") < feed.parse_version("v0.2.0")
    assert feed.parse_version("v0.2.0-dev.8") < feed.parse_version("v0.2.0-dev.9")


def test_drafts_and_foreign_tags_are_not_candidates():
    releases = [release("v0.2.0", draft=True), release("nightly-2026-09-01")]
    assert feed.head(releases, include_dev=True) is None


def test_an_entry_the_client_cannot_use_is_left_out():
    """An entry without both halves is worse than no entry: the client
    refuses it, and the channel has promised an update it cannot deliver."""
    platforms = feed.platforms_for(
        release(
            "v0.2.0",
            assets=[
                manifest_asset(
                    {
                        "darwin-aarch64": {"signature": "s", "url": "u"},
                        "linux-x86_64": {"signature": "", "url": "u"},
                        "windows-x86_64": {"signature": "s"},
                        "linux-aarch64": "not-an-object",
                    }
                )
            ],
        )
    )
    assert list(platforms) == ["darwin-aarch64"]


def test_the_platform_names_are_the_bundlers_own():
    """This used to rebuild them from asset names and got three of six wrong —
    `.nsis.zip` and `.AppImage.tar.gz` for artifacts the bundler calls
    `-setup.exe` and `.AppImage`. The release published green and would have
    offered updates to macOS only. The manifest is the bundler saying what it
    actually wrote, so there is nothing left to get wrong."""
    platforms = feed.platforms_for(release("v0.2.0"))
    assert set(platforms) == set(MANIFEST["platforms"])
    assert platforms["windows-x86_64"]["url"].endswith("windows-x86_64")
    assert feed.missing_platforms(platforms) == []


def test_a_release_without_the_manifest_offers_nothing():
    for assets in ([], [{"name": "latest.json"}], [{"name": "latest.json", "_contents": "{ truncated"}]):
        assert feed.platforms_for(release("v0.2.0", assets=assets)) == {}


def test_a_partial_release_says_which_targets_are_absent():
    platforms = feed.platforms_for(
        release("v0.2.0", assets=[manifest_asset({"darwin-aarch64": {"signature": "s", "url": "u"}})])
    )
    assert feed.missing_platforms(platforms) == [
        "darwin-x86_64",
        "linux-aarch64",
        "linux-x86_64",
        "windows-aarch64",
        "windows-x86_64",
    ]


def test_an_alias_beside_a_target_is_not_a_target():
    """The bundler writes `darwin-aarch64-app` next to `darwin-aarch64`.
    Carrying it through is right — a newer client may match on it — but it
    must not make a missing target look present."""
    platforms = feed.platforms_for(
        release(
            "v0.2.0",
            assets=[
                manifest_asset(
                    {
                        "darwin-aarch64": {"signature": "s", "url": "u"},
                        "darwin-aarch64-app": {"signature": "s", "url": "u"},
                    }
                )
            ],
        )
    )
    assert "darwin-aarch64-app" in platforms
    assert "darwin-x86_64" in feed.missing_platforms(platforms)


def test_writes_both_documents():
    releases = [release("v0.2.0")]
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / "releases.json"
        source.write_text(json.dumps(releases))
        out = Path(directory) / "feed"
        sys.argv = ["feed", str(source), "--out", str(out)]
        feed.main()
        for name in ("latest-main.json", "latest-dev.json"):
            body = json.loads((out / name).read_text())
            assert body["version"] == "0.2.0"
            assert body["platforms"]["darwin-aarch64"]["signature"] == "sig-darwin-aarch64"


if __name__ == "__main__":
    import run_tests

    raise SystemExit(run_tests.run_one(__file__))
