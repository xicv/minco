#!/usr/bin/env python3
"""Create or verify a deterministic SHA-256 manifest for the source tree."""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OUTPUT_RELATIVE = Path("verification/source-manifest.json")
EXCLUDED_PARTS = {".git", ".jj", ".venv", "target", "node_modules", "__pycache__"}
EXCLUDED_NAMES = {".env"}
EXCLUDED_RELATIVE = {
    OUTPUT_RELATIVE,
    # This report contains the source-tree digest and is excluded to avoid self-reference.
    Path("verification/adoption-measurements.json"),
}
EXCLUDED_SUFFIXES = {".pyc", ".zip", ".db", ".sqlite"}


def included(root: Path, path: Path) -> bool:
    relative = path.relative_to(root)
    if relative in EXCLUDED_RELATIVE:
        return False
    if any(part in EXCLUDED_PARTS for part in relative.parts):
        return False
    if path.name in EXCLUDED_NAMES:
        return False
    return path.suffix not in EXCLUDED_SUFFIXES


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def aggregate_digest(files: list[dict[str, Any]]) -> str:
    canonical = json.dumps(files, separators=(",", ":"), sort_keys=True).encode()
    return hashlib.sha256(canonical).hexdigest()


def build_report(root: Path) -> dict[str, Any]:
    files = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or not included(root, path):
            continue
        files.append(
            {
                "path": str(path.relative_to(root)),
                "size_bytes": path.stat().st_size,
                "sha256": digest(path),
            }
        )
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"][
        "version"
    ]
    return {
        "schema_version": 2,
        "artifact": "minco-cargo-ready-source",
        "version": version,
        "source_tree_sha256": aggregate_digest(files),
        "source_tree_exclusions": sorted(str(path) for path in EXCLUDED_RELATIVE),
        "file_count": len(files),
        "total_size_bytes": sum(item["size_bytes"] for item in files),
        "files": files,
    }


def render(report: dict[str, Any]) -> str:
    return json.dumps(report, indent=2, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail without writing if verification/source-manifest.json is stale",
    )
    args = parser.parse_args()
    output = ROOT / OUTPUT_RELATIVE
    expected = render(build_report(ROOT))
    if args.check:
        if not output.is_file() or output.read_text() != expected:
            print(
                f"{OUTPUT_RELATIVE} is stale; run scripts/source_manifest.py",
                file=sys.stderr,
            )
            return 1
        print(f"Verified {OUTPUT_RELATIVE}.")
        return 0
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(expected)
    report = json.loads(expected)
    print(
        f"Wrote {OUTPUT_RELATIVE} for {report['file_count']} files "
        f"({report['source_tree_sha256']})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
