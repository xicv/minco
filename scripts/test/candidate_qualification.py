#!/usr/bin/env python3
"""Behavioral tests for bounded Minco candidate qualification records."""

from __future__ import annotations

import importlib.util
import math
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "release" / "candidate_qualification.py"
SPEC = importlib.util.spec_from_file_location("candidate_qualification", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
CANDIDATE_QUALIFICATION = importlib.util.module_from_spec(SPEC)
sys.modules["candidate_qualification"] = CANDIDATE_QUALIFICATION
SPEC.loader.exec_module(CANDIDATE_QUALIFICATION)
RECOVERY_PATH = ROOT / "scripts" / "release" / "candidate_recovery.py"
RECOVERY_SPEC = importlib.util.spec_from_file_location("candidate_recovery", RECOVERY_PATH)
assert RECOVERY_SPEC is not None and RECOVERY_SPEC.loader is not None
CANDIDATE_RECOVERY = importlib.util.module_from_spec(RECOVERY_SPEC)
RECOVERY_SPEC.loader.exec_module(CANDIDATE_RECOVERY)


class LoadRecordContractTests(unittest.TestCase):
    def passing_record(self) -> dict:
        return {
            "schema_version": 2,
            "kind": "minco.candidate-load-qualification.v2",
            "status": "PASS",
            "production_slo": False,
            "provider_contact": False,
            "source": {"version": "1.2.0", "source_tree_sha256": "a" * 64},
            "topology": {"runtime": "local_native", "ingress": "local_tcp"},
            "runner": {"scope": "local", "source_tree_sha256": "a" * 64},
            "environment": {"fingerprint_sha256": "b" * 64},
            "classification": {"warm": True, "cold_start_measured": False},
            "api": {
                "database_max_connections": 4,
                "requests": 40,
                "failures": 0,
                "latency": {
                    "minimum_ms": 1.0,
                    "p50_ms": 2.0,
                    "p95_ms": 3.0,
                    "p99_ms": 4.0,
                    "maximum_ms": 5.0,
                },
            },
            "worker": {"messages": 100, "failures": 0},
            "queue": {"batch_size": 10, "maximum_concurrency": 2},
            "cost": {"modeled_lambda_invocations": 10, "pricing_claim": "none"},
            "artifacts": {"orders_local_bytes": 1, "worker_crate_bytes": 1},
        }

    def test_pass_requires_connection_queue_cost_and_artifact_measurements(self) -> None:
        record = self.passing_record()
        CANDIDATE_QUALIFICATION.validate_load_record(record)

        del record["queue"]
        with self.assertRaisesRegex(ValueError, "queue"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

    def test_pass_rejects_failures_or_zero_sized_artifacts(self) -> None:
        record = self.passing_record()
        record["api"]["failures"] = 1
        with self.assertRaisesRegex(ValueError, "api failures"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

        record = self.passing_record()
        record["artifacts"]["worker_crate_bytes"] = 0
        with self.assertRaisesRegex(ValueError, "artifact"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

    def test_pass_rejects_non_finite_or_reordered_latency(self) -> None:
        for invalid in (math.nan, math.inf, -math.inf, True):
            with self.subTest(invalid=invalid):
                record = self.passing_record()
                record["api"]["latency"]["p95_ms"] = invalid
                with self.assertRaisesRegex(ValueError, "finite"):
                    CANDIDATE_QUALIFICATION.validate_load_record(record)

        record = self.passing_record()
        record["api"]["latency"]["p95_ms"] = 6.0
        with self.assertRaisesRegex(ValueError, "monotonic"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

    def test_pass_cannot_claim_a_production_slo_or_provider_contact(self) -> None:
        record = self.passing_record()
        record["production_slo"] = True
        with self.assertRaisesRegex(ValueError, "production_slo"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

        record = self.passing_record()
        record["provider_contact"] = True
        with self.assertRaisesRegex(ValueError, "provider contact"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

    def test_runner_provenance_binds_the_verified_source_tree(self) -> None:
        record = self.passing_record()
        record["runner"]["source_tree_sha256"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "runner source tree"):
            CANDIDATE_QUALIFICATION.validate_load_record(record)

    def test_latency_summary_is_deterministic_for_small_samples(self) -> None:
        self.assertEqual(
            CANDIDATE_QUALIFICATION.summarize_latencies([40.0, 10.0, 30.0, 20.0]),
            {
                "minimum_ms": 10.0,
                "p50_ms": 20.0,
                "p95_ms": 40.0,
                "p99_ms": 40.0,
                "maximum_ms": 40.0,
            },
        )
        for invalid in (math.nan, math.inf, -math.inf, True):
            with self.subTest(invalid=invalid):
                with self.assertRaisesRegex(ValueError, "finite"):
                    CANDIDATE_QUALIFICATION.summarize_latencies([1.0, invalid])


class RecoveryRecordContractTests(unittest.TestCase):
    def test_pass_requires_restore_migration_and_rollback_evidence(self) -> None:
        record = {
            "status": "PASS",
            "data_boundary": "temporary synthetic SQLite only",
            "backup": {"integrity_check": "ok", "rows": 1},
            "restore": {"integrity_check": "ok", "rows": 1, "application_read": True},
            "migration": {"first_apply": True, "repeat_apply": True, "verify": True},
            "rollback": {"tests_passed": True, "reverse_sql": False},
        }
        CANDIDATE_QUALIFICATION.validate_recovery_record(record)

        record["restore"]["application_read"] = False
        with self.assertRaisesRegex(ValueError, "application read"):
            CANDIDATE_QUALIFICATION.validate_recovery_record(record)


class ReleaseGateRecordContractTests(unittest.TestCase):
    def test_standalone_appsync_proof_is_a_mandatory_release_gate(self) -> None:
        commands = CANDIDATE_QUALIFICATION.MANDATORY_RELEASE_COMMANDS
        self.assertIn("proofs/realtime-appsync/scripts/test-local.sh", commands)

    def test_pass_requires_every_mandatory_command_to_pass(self) -> None:
        commands = [
            {"command": command, "status": "PASS", "exit_code": 0}
            for command in CANDIDATE_QUALIFICATION.MANDATORY_RELEASE_COMMANDS
        ]
        record = {"status": "PASS", "commands": commands}
        CANDIDATE_QUALIFICATION.validate_release_gate_record(record)

        commands[-1]["status"] = "NOT RUN"
        commands[-1]["exit_code"] = None
        with self.assertRaisesRegex(ValueError, "NOT RUN"):
            CANDIDATE_QUALIFICATION.validate_release_gate_record(record)

    def test_publish_dry_run_precedes_generated_evidence(self) -> None:
        commands = list(CANDIDATE_QUALIFICATION.MANDATORY_RELEASE_COMMANDS)
        publish = commands.index("scripts/release/publish.sh --skip-quality")
        recovery = commands.index(
            "scripts/release/candidate-recovery.sh --output "
            + CANDIDATE_QUALIFICATION.CANDIDATE_RECOVERY_RECORD
        )
        load = commands.index(
            "scripts/release/candidate-load.sh --output "
            + CANDIDATE_QUALIFICATION.CANDIDATE_LOAD_RECORD
        )
        self.assertLess(publish, recovery)
        self.assertLess(publish, load)

    def test_generated_evidence_uses_the_current_release_series(self) -> None:
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        version = workspace["workspace"]["package"]["version"]
        release_series = ".".join(version.split(".")[:2])
        self.assertEqual(CANDIDATE_QUALIFICATION.WORKSPACE_VERSION, version)
        self.assertEqual(CANDIDATE_QUALIFICATION.RELEASE_SERIES, release_series)
        self.assertEqual(
            CANDIDATE_QUALIFICATION.CANDIDATE_RECOVERY_RECORD,
            f"verification/{release_series}-candidate-recovery.json",
        )
        self.assertEqual(
            CANDIDATE_QUALIFICATION.CANDIDATE_LOAD_RECORD,
            f"verification/{release_series}-candidate-load.json",
        )

    def test_every_current_command_catalog_preserves_historical_evidence(self) -> None:
        def strings(value: object) -> list[str]:
            if isinstance(value, str):
                return [value]
            if isinstance(value, list):
                return [item for entry in value for item in strings(entry)]
            if isinstance(value, dict):
                return [item for entry in value.values() for item in strings(entry)]
            return []

        expected_by_runner = {
            "candidate-load.sh": CANDIDATE_QUALIFICATION.CANDIDATE_LOAD_RECORD,
            "candidate-recovery.sh": CANDIDATE_QUALIFICATION.CANDIDATE_RECOVERY_RECORD,
        }
        for relative in (
            "quality.toml",
            "verification/deployment-assurance.toml",
        ):
            catalog = tomllib.loads((ROOT / relative).read_text())
            commands = [
                value
                for value in strings(catalog)
                if " --output verification/" in value
                and any(runner in value for runner in expected_by_runner)
            ]
            self.assertEqual(len(commands), 2, relative)
            for runner, expected_path in expected_by_runner.items():
                command = next(value for value in commands if runner in value)
                self.assertIn(f"--output {expected_path}", command, relative)
                self.assertNotIn("verification/1.0-candidate-", command, relative)


class ExternalConsumerEnvironmentTests(unittest.TestCase):
    def test_worker_consumer_uses_manifest_pinned_rust(self) -> None:
        environment = CANDIDATE_QUALIFICATION.external_cargo_environment(Path("target/test"))
        self.assertEqual(environment["RUSTUP_TOOLCHAIN"], "1.97.1")

    def test_worker_consumer_uses_the_workspace_locked_tokio(self) -> None:
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        locked_versions = {
            package["version"]
            for package in lock["package"]
            if package["name"] == "tokio"
        }
        self.assertEqual(len(locked_versions), 1)

        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            CANDIDATE_QUALIFICATION.worker_benchmark_sources(project)
            manifest = tomllib.loads((project / "Cargo.toml").read_text())

        self.assertEqual(
            manifest["dependencies"]["tokio"]["version"],
            f"={locked_versions.pop()}",
        )

    def test_generated_evidence_cannot_escape_project_boundaries(self) -> None:
        allowed = CANDIDATE_QUALIFICATION.safe_output_path(
            Path("target/minco/candidate-test.json")
        )
        self.assertEqual(allowed, ROOT / "target" / "minco" / "candidate-test.json")
        with self.assertRaisesRegex(ValueError, "target/minco or verification"):
            CANDIDATE_QUALIFICATION.safe_output_path(Path("/tmp/minco-escape.json"))


class MigrationRecoveryBoundaryTests(unittest.TestCase):
    def test_recovery_uses_catalog_owned_migration_lifecycle(self) -> None:
        command = CANDIDATE_RECOVERY.migration_command(
            "reviewed-digest", ROOT / "target" / "recovery-receipt.json"
        )
        self.assertEqual(command[:4], ["cargo", "minco", "db", "migrate"])
        self.assertNotIn("orders-migrate", command)
        self.assertIn("target/recovery-receipt.json", command)
        self.assertNotIn(str(ROOT / "target" / "recovery-receipt.json"), command)

    def test_migration_receipts_stay_inside_ignored_project_target(self) -> None:
        receipt_root = CANDIDATE_RECOVERY.receipt_root()
        self.assertTrue(receipt_root.is_relative_to(ROOT))
        self.assertEqual(
            receipt_root.relative_to(ROOT).parts[:3], ("target", "minco", "candidate-recovery")
        )


class RehearsalCleanupBoundaryTests(unittest.TestCase):
    def test_disposable_git_fixture_cleanup_cannot_prompt_on_read_only_objects(self) -> None:
        source = (ROOT / "scripts" / "aws" / "test-multi-release-rehearsal-plan.sh").read_text()
        cleanup = source.split("cleanup_fixture() {", maxsplit=1)[1].split("}", maxsplit=1)[0]
        self.assertIn('rm -rf -- "$fixture_dir"', cleanup)
        self.assertNotIn('rm -r -- "$fixture_dir"', cleanup)


if __name__ == "__main__":
    unittest.main()
