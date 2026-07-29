#!/usr/bin/env python3
"""Regression tests for deterministic native Lambda ZIP packaging."""
from __future__ import annotations

import hashlib
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMMON = ROOT / "scripts/aws/lib/common.sh"


def write_archive(
    path: Path,
    timestamp: tuple[int, int, int, int, int, int],
    *,
    include_ca: bool = False,
    unexpected_entry: bool = False,
) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        bootstrap = zipfile.ZipInfo("bootstrap", timestamp)
        bootstrap.external_attr = 0o100755 << 16
        archive.writestr(bootstrap, b"same-native-binary")
        if include_ca:
            certificate = zipfile.ZipInfo("rds-ca-bundle.pem", timestamp)
            certificate.external_attr = 0o100644 << 16
            archive.writestr(certificate, b"same-ca-bundle")
        if unexpected_entry:
            archive.writestr("unexpected", b"must fail closed")


def normalize(path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "bash",
            "-c",
            f'source "{COMMON}"; normalize_lambda_zip "$1"',
            "normalize-lambda-zip",
            str(path),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )


class LambdaArtifactReproducibilityTests(unittest.TestCase):
    def test_build_scripts_keep_lockfile_and_normalize_both_artifacts(self) -> None:
        for relative in [
            "scripts/aws/build-lambda.sh",
            "scripts/aws/build-worker-lambda.sh",
        ]:
            source = (ROOT / relative).read_text()
            self.assertIn("--locked", source)
            self.assertIn('normalize_lambda_zip "$artifact"', source)

    def test_timestamp_variance_normalizes_to_identical_archives(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-lambda-zip-") as raw:
            root = Path(raw)
            first = root / "first.zip"
            second = root / "second.zip"
            write_archive(first, (2026, 7, 28, 1, 2, 4), include_ca=True)
            write_archive(second, (2026, 7, 28, 5, 6, 8), include_ca=True)

            for path in [first, second]:
                result = normalize(path)
                self.assertEqual(result.returncode, 0, result.stderr)

            self.assertEqual(
                hashlib.sha256(first.read_bytes()).digest(),
                hashlib.sha256(second.read_bytes()).digest(),
            )
            with zipfile.ZipFile(first) as archive:
                self.assertEqual(
                    archive.namelist(),
                    ["bootstrap", "rds-ca-bundle.pem"],
                )
                for entry, mode in [
                    ("bootstrap", 0o755),
                    ("rds-ca-bundle.pem", 0o644),
                ]:
                    info = archive.getinfo(entry)
                    self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                    self.assertEqual((info.external_attr >> 16) & 0o777, mode)

    def test_unexpected_entries_fail_without_replacing_the_archive(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-lambda-zip-") as raw:
            path = Path(raw) / "unexpected.zip"
            write_archive(
                path,
                (2026, 7, 28, 1, 2, 4),
                unexpected_entry=True,
            )
            before = path.read_bytes()
            result = normalize(path)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
