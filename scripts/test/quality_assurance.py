#!/usr/bin/env python3
"""Behavioral tests for the measured quality-assurance contract."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "quality_assurance.py"
SPEC = importlib.util.spec_from_file_location("minco_quality_assurance", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
ASSURANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSURANCE)


class QualityAssuranceTests(unittest.TestCase):
    def valid_receipt(self, policy: dict[str, object]) -> dict[str, object]:
        dimensions = {
            "os": "darwin",
            "os_release": "fixture",
            "architecture": "aarch64",
            "python": "3.14.0",
        }
        return {
            "schema_version": 1,
            "kind": "minco.quality-assurance.v1",
            "status": "PASS",
            "production_slo": False,
            "provider_contact": False,
            "effective_date": "2026-08-12",
            "source": {"version": "1.8.0", "source_tree_sha256": "a" * 64},
            "policy": {"sha256": "b" * 64},
            "runner": {
                "scope": "local",
                "dimensions": dimensions,
                "fingerprint_sha256": hashlib.sha256(
                    json.dumps(
                        dimensions, separators=(",", ":"), sort_keys=True
                    ).encode()
                ).hexdigest(),
            },
            "tools": {
                identifier: {"version": version, "status": "PASS"}
                for identifier, version in policy["tools"].items()
            },
            "gates": {
                "nextest_parity": {
                    "status": "PASS",
                    "nextest_test_count": policy["nextest"]["executable_test_count"],
                    "doctest_count": 1,
                    "cargo_test_count": policy["nextest"]["executable_test_count"] + 1,
                },
                "coverage": {
                    "status": "PASS",
                    "line_percent": 84.91,
                    "function_percent": 80.98,
                },
                "mutation": {
                    "status": "PASS",
                    "total_mutants": 46,
                    "caught": 43,
                    "missed": 0,
                    "timeouts": 0,
                    "unviable": 3,
                },
                "semver": {
                    "status": "PASS",
                    "baseline_tag": policy["semver"]["baseline_tag"],
                    "baseline_commit": policy["semver"]["baseline_commit"],
                    "package_count": policy["semver"]["package_count"],
                    "checked_package_count": policy["semver"][
                        "baseline_package_count"
                    ],
                    "new_packages": policy["semver"]["new_packages"],
                },
                "local_performance": {
                    "status": "PASS",
                    "runner_scope": "local",
                    "production_slo": False,
                    "provider_contact": False,
                },
            },
            "limitations": ["Local evidence is not a production SLO."],
            "commands": [
                {
                    "id": "fixture",
                    "command": ["cargo", "test"],
                    "status": "PASS",
                    "exit_code": 0,
                    "duration_seconds": 1.0,
                    "log": {
                        "path": "target/minco/quality-assurance/logs/fixture.log",
                        "bytes": 1,
                        "sha256": "c" * 64,
                    },
                }
            ],
        }

    def current_receipt_fixture(
        self,
        policy: dict[str, object],
        root: Path,
    ) -> dict[str, object]:
        receipt = self.valid_receipt(policy)
        policy_path = root / ASSURANCE.POLICY_RELATIVE
        policy_path.parent.mkdir(parents=True)
        policy_path.write_text("fixture policy\n")
        performance_path = root / policy["performance"]["local_receipt"]
        performance_path.write_text("fixture performance\n")
        command_path = root / receipt["commands"][0]["log"]["path"]
        command_path.parent.mkdir(parents=True)
        command_path.write_bytes(b"fixture command\n")
        receipt["commands"][0]["log"] = {
            "path": str(command_path.relative_to(root)),
            "bytes": command_path.stat().st_size,
            "sha256": ASSURANCE.digest(command_path),
        }
        coverage_path = root / ASSURANCE.TARGET_RELATIVE / "coverage.json"
        coverage_path.write_bytes(b"fixture coverage\n")
        receipt["gates"]["coverage"]["report"] = {
            "path": str(coverage_path.relative_to(root)),
            "bytes": coverage_path.stat().st_size,
            "sha256": ASSURANCE.digest(coverage_path),
        }
        mutation_scopes = {}
        for name in ("plan", "release"):
            mutation_path = (
                root
                / ASSURANCE.TARGET_RELATIVE
                / f"mutants-{name}/mutants.out/outcomes.json"
            )
            mutation_path.parent.mkdir(parents=True)
            mutation_path.write_bytes(f"fixture {name} mutation\n".encode())
            mutation_scopes[name] = {
                "report": {
                    "path": str(mutation_path.relative_to(root)),
                    "bytes": mutation_path.stat().st_size,
                    "sha256": ASSURANCE.digest(mutation_path),
                }
            }
        receipt["gates"]["mutation"]["scopes"] = mutation_scopes
        receipt["policy"] = {
            "path": str(ASSURANCE.POLICY_RELATIVE),
            "sha256": ASSURANCE.digest(policy_path),
        }
        receipt["gates"]["semver"]["packages"] = []
        receipt["gates"]["local_performance"]["receipt"] = {
            "path": str(policy["performance"]["local_receipt"]),
            "bytes": performance_path.stat().st_size,
            "sha256": ASSURANCE.digest(performance_path),
        }
        return receipt

    def test_current_policy_pins_compatible_exact_tools(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )

        self.assertEqual(
            policy["tools"],
            {
                "cargo-nextest": "0.9.143",
                "cargo-llvm-cov": "0.8.7",
                "cargo-mutants": "27.1.0",
                "cargo-semver-checks": "0.50.0",
            },
        )
        self.assertEqual(policy["semver"]["baseline_tag"], "v1.9.0")
        self.assertEqual(policy["semver"]["package_count"], 37)
        self.assertEqual(policy["semver"]["baseline_package_count"], 34)
        self.assertEqual(
            policy["semver"]["new_packages"],
            ["minco-interaction", "minco-plugin-jobs", "minco-plugin-ticketing"],
        )
        self.assertEqual(policy["nextest"]["baseline_executable_test_count"], 122)
        self.assertEqual(policy["nextest"]["executable_test_count"], 166)
        self.assertFalse(policy["production_slo"])
        self.assertFalse(policy["provider_contact"])

    def test_passing_receipt_requires_every_reviewed_gate(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        del receipt["gates"]["mutation"]

        with self.assertRaisesRegex(
            ValueError,
            "ASSURANCE-RECEIPT-004: receipt requires gate results",
        ):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_passing_receipt_rejects_coverage_below_measured_floor(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["gates"]["coverage"]["line_percent"] = 82.90

        with self.assertRaisesRegex(
            ValueError,
            "ASSURANCE-COVERAGE-001: line coverage 82.9 is below measured floor 82.91",
        ):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_passing_receipt_rejects_a_surviving_reviewed_mutant(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["gates"]["mutation"].update({"caught": 42, "missed": 1})

        with self.assertRaisesRegex(
            ValueError,
            "ASSURANCE-MUTATION-001: mutation result exceeds the measured baseline",
        ):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_output_path_rejects_escape_and_symlink_parent(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-assurance-path-") as temporary:
            root = Path(temporary)
            (root / "verification").mkdir()
            (root / "target/minco").mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, "ASSURANCE-PATH-001"):
                ASSURANCE.safe_output_path(Path("../escape.json"), root)

            outside = root / "outside"
            outside.mkdir()
            (root / "target/minco/quality-assurance").symlink_to(
                outside, target_is_directory=True
            )
            with self.assertRaisesRegex(ValueError, "ASSURANCE-PATH-001"):
                ASSURANCE.safe_output_path(
                    Path("target/minco/quality-assurance/receipt.json"), root
                )

    def test_passing_receipt_requires_nonblank_limitations(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["limitations"] = [" "]

        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-007"):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_effective_date_accepts_the_reviewed_exact_string(self) -> None:
        self.assertEqual(ASSURANCE.effective_date(ROOT), "2026-08-12")

    def test_passing_receipt_requires_a_verified_local_runner_identity(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["runner"] = None

        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-008"):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_passing_receipt_rejects_non_finite_measurements(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["gates"]["coverage"]["line_percent"] = math.nan

        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-009"):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_passing_receipt_requires_every_tool_to_have_run(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["tools"]["cargo-nextest"]["status"] = "NOT RUN"

        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-003"):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_checked_receipt_requires_canonical_unique_json_keys(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-assurance-json-") as temporary:
            path = Path(temporary) / "receipt.json"
            path.write_text('{"status":"PASS","status":"NOT RUN"}\n')
            with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-010"):
                ASSURANCE.load_canonical_receipt(path)

            path.write_text('{"status": "PASS"}\n')
            with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-010"):
                ASSURANCE.load_canonical_receipt(path)

    def test_passing_receipt_rejects_unknown_claims_and_missing_commands(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        receipt["hosted_linux"] = "PASS"
        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-011"):
            ASSURANCE.validate_receipt(receipt, policy)

        receipt = self.valid_receipt(policy)
        receipt["commands"] = []
        with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-012"):
            ASSURANCE.validate_receipt(receipt, policy)

    def test_artifact_identity_requires_matching_confined_regular_file(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-assurance-artifact-") as temporary:
            root = Path(temporary)
            artifact = root / "target/minco/quality-assurance/logs/fixture.log"
            artifact.parent.mkdir(parents=True)
            artifact.write_bytes(b"x")
            identity = {
                "path": "target/minco/quality-assurance/logs/fixture.log",
                "bytes": 1,
                "sha256": hashlib.sha256(b"x").hexdigest(),
            }

            ASSURANCE.validate_artifact_identity(
                identity,
                allowed_prefixes=(ASSURANCE.TARGET_RELATIVE,),
                root=root,
            )
            artifact.unlink()
            with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-013"):
                ASSURANCE.validate_artifact_identity(
                    identity,
                    allowed_prefixes=(ASSURANCE.TARGET_RELATIVE,),
                    root=root,
                )

    def test_current_receipt_rejects_missing_private_command_evidence(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        receipt = self.valid_receipt(policy)
        with tempfile.TemporaryDirectory(prefix="minco-assurance-current-") as temporary:
            root = Path(temporary)
            receipt = self.current_receipt_fixture(policy, root)
            (root / receipt["commands"][0]["log"]["path"]).unlink()

            with (
                mock.patch.object(
                    ASSURANCE,
                    "load_source_manifest",
                    return_value=receipt["source"],
                ),
                mock.patch.object(
                    ASSURANCE,
                    "effective_date",
                    return_value=receipt["effective_date"],
                ),
                mock.patch.object(ASSURANCE, "publishable_packages", return_value=[]),
                self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-013"),
            ):
                ASSURANCE.validate_current_receipt(receipt, policy, root=root)

    def test_current_receipt_rejects_missing_private_coverage_evidence(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        with tempfile.TemporaryDirectory(prefix="minco-assurance-current-") as temporary:
            root = Path(temporary)
            receipt = self.current_receipt_fixture(policy, root)
            (root / receipt["gates"]["coverage"]["report"]["path"]).unlink()

            with (
                mock.patch.object(
                    ASSURANCE,
                    "load_source_manifest",
                    return_value=receipt["source"],
                ),
                mock.patch.object(
                    ASSURANCE,
                    "effective_date",
                    return_value=receipt["effective_date"],
                ),
                mock.patch.object(ASSURANCE, "publishable_packages", return_value=[]),
                self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-013"),
            ):
                ASSURANCE.validate_current_receipt(receipt, policy, root=root)

    def test_current_receipt_rejects_missing_private_mutation_evidence(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        with tempfile.TemporaryDirectory(prefix="minco-assurance-current-") as temporary:
            root = Path(temporary)
            receipt = self.current_receipt_fixture(policy, root)
            report = receipt["gates"]["mutation"]["scopes"]["plan"]["report"]
            (root / report["path"]).unlink()

            with (
                mock.patch.object(
                    ASSURANCE,
                    "load_source_manifest",
                    return_value=receipt["source"],
                ),
                mock.patch.object(
                    ASSURANCE,
                    "effective_date",
                    return_value=receipt["effective_date"],
                ),
                mock.patch.object(ASSURANCE, "publishable_packages", return_value=[]),
                self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-013"),
            ):
                ASSURANCE.validate_current_receipt(receipt, policy, root=root)

    def test_artifact_identity_rejects_symlinked_leaf(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-assurance-symlink-") as temporary:
            root = Path(temporary)
            outside = root / "outside.log"
            outside.write_bytes(b"x")
            artifact = root / "target/minco/quality-assurance/logs/fixture.log"
            artifact.parent.mkdir(parents=True)
            artifact.symlink_to(outside)
            identity = {
                "path": str(artifact.relative_to(root)),
                "bytes": 1,
                "sha256": hashlib.sha256(b"x").hexdigest(),
            }

            with self.assertRaisesRegex(ValueError, "ASSURANCE-RECEIPT-013"):
                ASSURANCE.validate_artifact_identity(
                    identity,
                    allowed_prefixes=(ASSURANCE.TARGET_RELATIVE,),
                    root=root,
                )

    def test_local_performance_gate_honors_private_receipt_path(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        source = {"version": "1.4.0", "source_tree_sha256": "a" * 64}
        private_receipt = (
            ASSURANCE.TARGET_RELATIVE / "release-candidate-load.json"
        )
        with tempfile.TemporaryDirectory(prefix="minco-assurance-performance-") as temporary:
            root = Path(temporary)
            receipt_path = root / private_receipt
            receipt_path.parent.mkdir(parents=True)

            def run_fixture(*_args, **_kwargs):
                receipt_path.write_text(
                    json.dumps(
                        {
                            "schema_version": 2,
                            "kind": "minco.candidate-load-qualification.v2",
                            "status": "PASS",
                            "production_slo": False,
                            "provider_contact": False,
                            "runner": {"scope": "local"},
                            "source": source,
                            "api": {
                                "requests": 80,
                                "latency": {"p95_ms": 1.0, "p99_ms": 2.0},
                                "throughput_requests_per_second": 100.0,
                            },
                            "worker": {
                                "messages": 1000,
                                "throughput_messages_per_second": 200.0,
                            },
                        },
                        sort_keys=True,
                    )
                )
                return "", {"id": "fixture"}

            commands: list[dict[str, object]] = []
            with mock.patch.object(ASSURANCE, "run_command", side_effect=run_fixture):
                gate = ASSURANCE.local_performance_gate(
                    policy,
                    source,
                    commands,
                    root,
                    receipt_relative=private_receipt,
                )

            self.assertEqual(gate["receipt"]["path"], str(private_receipt))
            self.assertFalse((root / policy["performance"]["local_receipt"]).exists())

    def test_current_receipt_accepts_private_performance_evidence(self) -> None:
        policy = ASSURANCE.load_policy(
            ROOT / "verification" / "quality-assurance-policy.toml"
        )
        with tempfile.TemporaryDirectory(prefix="minco-assurance-current-") as temporary:
            root = Path(temporary)
            receipt = self.current_receipt_fixture(policy, root)
            tracked = root / policy["performance"]["local_receipt"]
            private = root / ASSURANCE.TARGET_RELATIVE / "release-candidate-load.json"
            private.write_bytes(tracked.read_bytes())
            tracked.unlink()
            receipt["gates"]["local_performance"]["receipt"] = {
                "path": str(private.relative_to(root)),
                "bytes": private.stat().st_size,
                "sha256": ASSURANCE.digest(private),
            }

            with (
                mock.patch.object(
                    ASSURANCE,
                    "load_source_manifest",
                    return_value=receipt["source"],
                ),
                mock.patch.object(
                    ASSURANCE,
                    "effective_date",
                    return_value=receipt["effective_date"],
                ),
                mock.patch.object(ASSURANCE, "publishable_packages", return_value=[]),
            ):
                ASSURANCE.validate_current_receipt(receipt, policy, root=root)

    def test_semver_gate_excludes_only_reviewed_new_packages(self) -> None:
        policy = {
            "semver": {
                "baseline_tag": "v1.9.0",
                "baseline_commit": "a" * 40,
                "package_count": 2,
                "baseline_package_count": 1,
                "new_packages": ["minco-new"],
            }
        }
        completed = mock.Mock(returncode=0, stdout=("a" * 40) + "\n")
        with (
            mock.patch.object(
                ASSURANCE,
                "publishable_packages",
                return_value=["minco-established", "minco-new"],
            ),
            mock.patch.object(
                ASSURANCE,
                "command_environment",
                return_value={"GIT_DIR": "/fixture/.git"},
            ),
            mock.patch.object(ASSURANCE.subprocess, "run", return_value=completed),
            mock.patch.object(
                ASSURANCE,
                "baseline_package_names",
                return_value={"minco-established"},
            ),
            mock.patch.object(
                ASSURANCE,
                "run_command",
                return_value=("", {"id": "semver"}),
            ) as run_command,
        ):
            gate = ASSURANCE.semver_gate(policy, [], Path("/fixture"))

        arguments = run_command.call_args.args[1]
        self.assertIn("minco-established", arguments)
        self.assertNotIn("minco-new", arguments)
        self.assertEqual(gate["packages"], ["minco-established", "minco-new"])
        self.assertEqual(gate["checked_packages"], ["minco-established"])
        self.assertEqual(gate["new_packages"], ["minco-new"])

    def test_semver_gate_rejects_a_package_that_is_not_new(self) -> None:
        policy = {
            "semver": {
                "baseline_tag": "v1.9.0",
                "baseline_commit": "a" * 40,
                "package_count": 2,
                "baseline_package_count": 1,
                "new_packages": ["minco-new"],
            }
        }
        completed = mock.Mock(returncode=0, stdout=("a" * 40) + "\n")
        with (
            mock.patch.object(
                ASSURANCE,
                "publishable_packages",
                return_value=["minco-established", "minco-new"],
            ),
            mock.patch.object(
                ASSURANCE,
                "command_environment",
                return_value={"GIT_DIR": "/fixture/.git"},
            ),
            mock.patch.object(ASSURANCE.subprocess, "run", return_value=completed),
            mock.patch.object(
                ASSURANCE,
                "baseline_package_names",
                return_value={"minco-established", "minco-new"},
            ),
            self.assertRaisesRegex(RuntimeError, "ASSURANCE-SEMVER-004"),
        ):
            ASSURANCE.semver_gate(policy, [], Path("/fixture"))


if __name__ == "__main__":
    unittest.main()
