"""The gate that keeps an unreachable release from being tagged.

Run with `python3 -m pytest scripts/tests` or plain
`python3 scripts/tests/test_release_tag.py`.
"""

import importlib.util
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# The checker imports the feed builder by plain name, which works because
# Python puts a script's own directory first on the path. Loading it by file
# here has to reproduce that.
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("gate", ROOT / "scripts/check_release_tag.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


def feed_dir(**documents):
    """A checkout of the updates branch serving the given versions."""
    directory = Path(tempfile.mkdtemp())
    for name, version in documents.items():
        (directory / f"latest-{name}.json").write_text(json.dumps({"version": version}))
    return directory


def refused(tag, configured=None, **documents):
    return gate.check(tag, configured or tag.removeprefix("v"), feed_dir(**documents))


def test_a_tag_the_feed_cannot_read_is_refused():
    # Valid SemVer, and both documents would skip it forever.
    assert refused("v0.2.0-rc.1") is not None
    assert refused("v0.2.0-beta1") is not None
    assert refused("v0.2") is not None
    assert refused("v0.2.0-dev") is not None
    assert refused("v0.2.0.1") is not None


def test_the_grammar_is_the_feed_builders():
    # Not a restatement of it: the same call the builder makes when it decides
    # whether a release exists. If these two ever disagree the gate is useless.
    for tag in ("0.2.0", "0.2.0-dev.1", "12.0.34-dev.567"):
        assert gate.check(f"v{tag}", tag, None) is None
        assert gate.feed.parse_version(tag) is not None
    for tag in ("0.2.0-rc.1", "0.2", "0.2.0-dev"):
        assert gate.check(f"v{tag}", tag, None) is not None
        assert gate.feed.parse_version(tag) is None


def test_a_tag_that_disagrees_with_the_config_is_refused():
    assert refused("v0.2.0", configured="0.2.1") is not None


def test_a_dev_tag_below_the_shipped_main_is_refused():
    # The one that looks fine: -dev.9 is a higher counter than any earlier Dev
    # tag, and still orders below the 0.2.0 the Dev document already serves.
    assert refused("v0.2.0-dev.9", dev="0.2.0", main="0.2.0") is not None


def test_a_dev_tag_above_the_dev_head_is_built():
    assert refused("v0.2.1-dev.1", dev="0.2.0", main="0.2.0") is None


def test_retagging_the_served_version_is_refused():
    assert refused("v0.2.0", main="0.2.0") is not None


def test_a_main_tag_need_not_outrank_the_dev_head():
    # Dev users are ahead on purpose. A patch on the Main line is a real
    # release for Main subscribers even though the Dev document keeps serving
    # the newer preview.
    assert refused("v0.2.1", main="0.2.0", dev="0.3.0-dev.1") is None


def test_a_dev_tag_is_measured_against_dev_not_main():
    assert refused("v0.2.1-dev.1", main="0.1.0", dev="0.3.0") is not None


def test_the_first_release_into_a_channel_has_no_head_to_beat():
    assert refused("v0.1.0") is None
    assert refused("v0.1.0-dev.1", main="0.9.0") is None


def test_an_unreadable_document_stops_the_release():
    # Our own workflow wrote it. Reading it as "no head" would publish.
    directory = feed_dir()
    (directory / "latest-main.json").write_text("{ truncated")
    for body in ("{ truncated", "{}", '{"version": "not-a-version"}'):
        (directory / "latest-main.json").write_text(body)
        try:
            gate.check("v0.2.0", "0.2.0", directory)
        except SystemExit:
            continue
        raise AssertionError(f"{body!r} was accepted as a feed document")


if __name__ == "__main__":
    import run_tests

    raise SystemExit(run_tests.run_one(__file__))
