#!/usr/bin/env python3
"""Regression test for deep-review source discovery."""
from __future__ import annotations

import importlib.util
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("deep_review", ROOT / "scripts/deep_review.py")
assert SPEC is not None and SPEC.loader is not None
DEEP_REVIEW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEEP_REVIEW)
STATIC_SPEC = importlib.util.spec_from_file_location(
    "validate_static", ROOT / "scripts/validate_static.py"
)
assert STATIC_SPEC is not None and STATIC_SPEC.loader is not None
VALIDATE_STATIC = importlib.util.module_from_spec(STATIC_SPEC)
sys.modules[STATIC_SPEC.name] = VALIDATE_STATIC
STATIC_SPEC.loader.exec_module(VALIDATE_STATIC)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="minco-deep-review-") as temporary:
        root = Path(temporary)
        expected = root / "migrations" / "001_feedback.sql"
        excluded = [
            root / "target" / "package" / "001_packaged.sql",
            root / "node_modules" / "fixture" / "001_dependency.sql",
            root / ".jj" / "repo" / "001_metadata.sql",
        ]
        for path in [expected, *excluded]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("SELECT 1;\n")

        assert DEEP_REVIEW.source_files("*.sql", root) == [expected]

    assert VALIDATE_STATIC.report_root(ROOT, ROOT) == "."
    assert VALIDATE_STATIC.report_root(ROOT / "nested", ROOT) == str(ROOT / "nested")

    print("deep-review source exclusions: passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
