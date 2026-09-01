"""Run every suite under this directory.

Discovery rather than a list, so adding a suite does not also mean remembering
a line in two workflows. And a count per module, because the obvious shape —
a shell loop over `test_*.py` — passes a file that forgot its runner block:
importing it defines the tests and exits 0, and the suite silently never runs.
"""

import importlib.util
import sys
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
# The suites load the scripts they test by plain name.
sys.path.insert(0, str(HERE.parent))


def run(path: Path) -> tuple[int, int]:
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    cases = sorted(
        (name, case)
        for name, case in vars(module).items()
        if name.startswith("test_") and callable(case)
    )
    if not cases:
        print(f"FAIL {path.name}: defines no tests", file=sys.stderr)
        return 0, 1
    failures = 0
    for name, case in cases:
        try:
            case()
        except Exception:
            failures += 1
            print(f"FAIL {path.name}::{name}", file=sys.stderr)
            traceback.print_exc()
        else:
            print(f"ok   {path.name}::{name}")
    return len(cases), failures


def run_one(path) -> int:
    """One suite, for a `__main__` block that wants to run just itself."""
    sys.path.insert(0, str(HERE))
    ran, failures = run(Path(path).resolve())
    print(f"{ran} tests, {failures} failed")
    return 1 if failures else 0


def main() -> int:
    suites = sorted(HERE.glob("test_*.py"))
    if not suites:
        print("no suites found — discovery is broken", file=sys.stderr)
        return 1
    total = failures = 0
    for suite in suites:
        ran, failed = run(suite)
        total += ran
        failures += failed
    print(f"{total} tests, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
