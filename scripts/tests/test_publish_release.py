"""The check that runs at publication, which is the irreversible step."""

import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("publish", ROOT / "scripts/publish_release.py")
publish = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publish)

# One signed target, in the shape the releases API returns.
SIGNED = [
    {"name": "Vibe.Bar.Desktop_aarch64.app.tar.gz"},
    {"name": "Vibe.Bar.Desktop_aarch64.app.tar.gz.sig"},
]


def draft(assets=None, is_draft=True):
    return {"isDraft": is_draft, "assets": list(SIGNED if assets is None else assets)}


def test_a_draft_that_outranks_its_channel_publishes():
    assert publish.check_publishable("v0.2.1", draft(), "0.2.0") is None


def test_the_second_draft_built_against_the_same_head_is_refused():
    # Both tags built while both drafts sat unpublished, so both passed the
    # build-time gate against 0.2.0. Publishing 0.2.2 first makes 0.2.1 a
    # release the feed will never serve, and this is where that is caught.
    assert publish.check_publishable("v0.2.1", draft(), "0.2.2") is not None


def test_republishing_the_served_version_is_refused():
    assert publish.check_publishable("v0.2.0", draft(), "0.2.0") is not None


def test_a_first_release_has_no_head_to_beat():
    assert publish.check_publishable("v0.1.0", draft(), None) is None


def test_a_draft_without_signed_artifacts_is_refused():
    # The feed would skip it however it is published.
    assert publish.check_publishable("v0.2.1", draft(assets=[]), "0.2.0") is not None
    unsigned = [{"name": "Vibe.Bar.Desktop_aarch64.app.tar.gz"}]
    assert publish.check_publishable("v0.2.1", draft(assets=unsigned), "0.2.0") is not None


def test_an_already_published_release_is_refused():
    assert publish.check_publishable("v0.2.1", draft(is_draft=False), "0.2.0") is not None


def test_a_tag_the_feed_cannot_read_is_refused():
    assert publish.check_publishable("v0.2.1-rc.1", draft(), "0.2.0") is not None


def test_the_asset_rule_is_the_feed_builders():
    # If these two disagree, a release passes here and vanishes from the feed.
    assets = {asset["name"]: asset for asset in SIGNED}
    assert any(
        publish.feed.signed_asset(assets, platform) is not None
        for platform in publish.feed.PLATFORMS
    )


def released(tag, is_draft=False):
    return {"tag_name": tag, "draft": is_draft}


def test_a_release_published_a_minute_ago_counts_as_the_head():
    # The feed workflow has not run yet, so the document still says 0.2.0.
    # Publishing 0.2.1 after 0.2.2 would leave 0.2.1 permanently unserved.
    head = publish.effective_head("0.2.0", [released("v0.2.2")], include_dev=False)
    assert head == "0.2.2"
    assert publish.check_publishable("v0.2.1", draft(), head) is not None


def test_drafts_are_not_a_head():
    # A draft is not a commitment; it may never be published.
    assert publish.effective_head("0.2.0", [released("v0.9.0", is_draft=True)],
                                  include_dev=False) == "0.2.0"


def test_the_document_still_counts_when_it_is_the_higher_one():
    assert publish.effective_head("0.3.0", [released("v0.2.0")], include_dev=False) == "0.3.0"


def test_a_main_publication_is_a_head_for_dev_too():
    # The Dev document serves the newer of both channels.
    assert publish.effective_head(None, [released("v0.4.0")], include_dev=True) == "0.4.0"
    assert publish.effective_head(None, [released("v0.4.0")], include_dev=False) == "0.4.0"


def test_a_dev_publication_is_not_a_head_for_main():
    assert publish.effective_head(None, [released("v0.4.0-dev.1")], include_dev=False) is None


def test_nothing_published_and_no_document_is_the_first_release():
    assert publish.effective_head(None, [], include_dev=True) is None


if __name__ == "__main__":
    import run_tests

    raise SystemExit(run_tests.run_one(__file__))
