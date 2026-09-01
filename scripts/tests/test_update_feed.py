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


def release(tag, *, draft=False, assets=()):
    return {
        "tag_name": tag,
        "draft": draft,
        "published_at": "2026-09-01T00:00:00Z",
        "body": "",
        "assets": list(assets),
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


def test_a_target_without_a_signature_is_left_out():
    """An entry the client would refuse is worse than no entry."""
    signed = {"name": "app_0.2.0_aarch64.app.tar.gz", "browser_download_url": "u1"}
    signature = {"name": "app_0.2.0_aarch64.app.tar.gz.sig", "_contents": "sig", "browser_download_url": "u2"}
    unsigned = {"name": "app_0.2.0_amd64.AppImage.tar.gz", "browser_download_url": "u3"}
    platforms = feed.platforms_for(release("v0.2.0", assets=[signed, signature, unsigned]))
    assert "darwin-aarch64" in platforms
    assert "linux-x86_64" not in platforms


def test_one_macos_build_does_not_answer_for_both_architectures():
    """`.app.tar.gz` is what arm64 and x64 are each called, so the suffix alone
    made one build appear as two platforms — and half of those users would
    have been handed the wrong binary."""
    signed = {"name": "app_0.2.0_aarch64.app.tar.gz", "browser_download_url": "u1"}
    signature = {"name": "app_0.2.0_aarch64.app.tar.gz.sig", "_contents": "sig", "browser_download_url": "u2"}
    platforms = feed.platforms_for(release("v0.2.0", assets=[signed, signature]))
    assert list(platforms) == ["darwin-aarch64"]


def test_each_target_takes_its_own_artifact():
    def pair(name):
        return [
            {"name": name, "browser_download_url": f"url-{name}"},
            {"name": f"{name}.sig", "_contents": f"sig-{name}", "browser_download_url": "s"},
        ]

    assets = [
        *pair("app_0.2.0_aarch64.app.tar.gz"),
        *pair("app_0.2.0_x64.app.tar.gz"),
        *pair("app_0.2.0_x64-setup.nsis.zip"),
        *pair("app_0.2.0_arm64-setup.nsis.zip"),
        *pair("app_0.2.0_amd64.AppImage.tar.gz"),
        *pair("app_0.2.0_aarch64.AppImage.tar.gz"),
    ]
    platforms = feed.platforms_for(release("v0.2.0", assets=assets))
    assert set(platforms) == set(feed.PLATFORMS)
    assert platforms["darwin-aarch64"]["url"] == "url-app_0.2.0_aarch64.app.tar.gz"
    assert platforms["linux-aarch64"]["url"] == "url-app_0.2.0_aarch64.AppImage.tar.gz"
    assert platforms["windows-x86_64"]["url"] == "url-app_0.2.0_x64-setup.nsis.zip"


def test_writes_both_documents():
    signed = {"name": "app_0.2.0_aarch64.app.tar.gz", "browser_download_url": "u1"}
    signature = {"name": "app_0.2.0_aarch64.app.tar.gz.sig", "_contents": "sig", "browser_download_url": "u2"}
    releases = [release("v0.2.0", assets=[signed, signature])]
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / "releases.json"
        source.write_text(json.dumps(releases))
        out = Path(directory) / "feed"
        sys.argv = ["feed", str(source), "--out", str(out)]
        feed.main()
        for name in ("latest-main.json", "latest-dev.json"):
            body = json.loads((out / name).read_text())
            assert body["version"] == "0.2.0"
            assert body["platforms"]["darwin-aarch64"]["signature"] == "sig"


if __name__ == "__main__":
    import run_tests

    raise SystemExit(run_tests.run_one(__file__))
