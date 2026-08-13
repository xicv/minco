#!/usr/bin/env python3
"""Mutation tests for Minco's operational evidence validator."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKSPACE_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
    "package"
]["version"]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


VALIDATOR = load_module(
    "validate_operational_evidence",
    ROOT / "scripts" / "validate_operational_evidence.py",
)
SOURCE_MANIFEST = load_module("operational_source_manifest", ROOT / "scripts/source_manifest.py")


class OperationalEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="minco-operational-evidence-")
        self.root = Path(self.temporary.name)
        (self.root / "verification").mkdir()
        (self.root / "Cargo.toml").write_text(
            f'[workspace]\nresolver = "3"\n\n[workspace.package]\nversion = "{WORKSPACE_VERSION}"\n'
        )
        for name in (
            "repository-truth.toml",
            "performance-policy.toml",
            "provider-evidence.toml",
            "aws-capability-candidates.toml",
            "1.7-performance-baseline.json",
        ):
            shutil.copy2(ROOT / "verification" / name, self.root / "verification" / name)
        (self.root / "VERIFICATION.md").write_text("bounded historical provider evidence\n")
        self.replace_historical_digest()
        self.refresh_manifest_and_baseline()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def replace_historical_digest(self) -> None:
        path = self.root / "verification/provider-evidence.toml"
        digest = hashlib.sha256((self.root / "VERIFICATION.md").read_bytes()).hexdigest()
        repository_digest = hashlib.sha256((ROOT / "VERIFICATION.md").read_bytes()).hexdigest()
        source = path.read_text()
        self.assertIn(repository_digest, source)
        source = source.replace(
            repository_digest,
            digest,
            1,
        )
        path.write_text(source)

    def refresh_manifest_and_baseline(self) -> None:
        report = SOURCE_MANIFEST.build_report(self.root)
        (self.root / "verification/source-manifest.json").write_text(SOURCE_MANIFEST.render(report))
        baseline_path = self.root / "verification/1.7-performance-baseline.json"
        baseline = json.loads(baseline_path.read_text())
        baseline["source_tree_sha256"] = report["source_tree_sha256"]
        baseline_path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        final = SOURCE_MANIFEST.build_report(self.root)
        self.assertEqual(report["source_tree_sha256"], final["source_tree_sha256"])
        (self.root / "verification/source-manifest.json").write_text(SOURCE_MANIFEST.render(final))

    def report(self, *, require_current_provider: bool = False):
        return VALIDATOR.Validator(
            self.root,
            require_current_provider=require_current_provider,
        ).run()

    def configure_current_provider(self, *, write_receipt: bool) -> None:
        artifact = self.root / "verification/live-provider-proof.json"
        artifact.write_text('{"cleanup":"verified_absent","provider":"aws"}\n')
        artifact_digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        ledger = self.root / "verification/provider-evidence.toml"
        source = ledger.read_text()
        replacements = [
            ('evidence_state = "not_run"', 'evidence_state = "current"'),
            ('source_revision = "no-observed-source-no-provider-contact"', 'source_revision = "receipt_bound"'),
            ('observed_at = ""', 'observed_at = "2026-08-07T00:00:00Z"'),
            ('evidence_kind = "none"', 'evidence_kind = "bounded_disposable_rehearsal"'),
            ('provider_contact = false', 'provider_contact = true'),
            ('aws_region = "not_applicable"', 'aws_region = "ap-southeast-2"'),
            ('account_scope = "none"', 'account_scope = "bounded_disposable_nonproduction"'),
            ('dimensions_proven = []', 'dimensions_proven = ["deployment", "cleanup"]'),
            ('cleanup_state = "not_required_no_contact"', 'cleanup_state = "verified_absent"'),
            ('evidence_paths = []', 'evidence_paths = ["verification/live-provider-proof.json"]'),
            ('evidence_sha256 = []', f'evidence_sha256 = ["{artifact_digest}"]'),
            ('qualification_receipt_path = ""', 'qualification_receipt_path = "verification/provider-evidence-receipts/minimal-aws-current-candidate.json"'),
        ]
        for before, after in replacements:
            self.assertIn(before, source)
            source = source.replace(before, after, 1)
        ledger.write_text(source)
        self.refresh_manifest_and_baseline()
        if write_receipt:
            self.write_current_provider_receipt()

    def write_current_provider_receipt(self) -> None:
        ledger = self.root / "verification/provider-evidence.toml"
        profile = tomllib.loads(ledger.read_text())["profile"][0]
        manifest = json.loads((self.root / "verification/source-manifest.json").read_text())
        payload = {
            "schema_version": 1,
            "kind": "minco.provider-evidence-receipt.v1",
            "profile_id": profile["id"],
            "source_tree_sha256": manifest["source_tree_sha256"],
            "observed_at": profile["observed_at"],
            "reviewed_at": profile["reviewed_at"],
            "evidence_kind": profile["evidence_kind"],
            "provider_contact": True,
            "aws_region": profile["aws_region"],
            "account_scope": profile["account_scope"],
            "dimensions_proven": profile["dimensions_proven"],
            "cleanup_state": "verified_absent",
            "retained_resources": [],
            "evidence_paths": profile["evidence_paths"],
            "evidence_sha256": profile["evidence_sha256"],
            "limitations": profile["limitations"],
        }
        receipt = dict(payload)
        receipt["receipt_digest"] = hashlib.sha256(
            json.dumps(payload, allow_nan=False, separators=(",", ":"), sort_keys=True).encode()
        ).hexdigest()
        receipt_dir = self.root / "verification/provider-evidence-receipts"
        receipt_dir.mkdir(exist_ok=True)
        (receipt_dir / "minimal-aws-current-candidate.json").write_text(
            json.dumps(receipt, allow_nan=False, indent=2, sort_keys=True) + "\n"
        )

    @staticmethod
    def codes(report: dict) -> set[str]:
        return {finding["code"] for finding in report["findings"]}

    def passing_measurement(self, fingerprint: str) -> dict:
        return {
            "environment_fingerprint_sha256": fingerprint,
            "api": {
                "requests": 80,
                "failures": 0,
                "error_rate": 0.0,
                "throughput_requests_per_second": 100.0,
                "latency": {
                    "minimum_ms": 1.0,
                    "p50_ms": 2.0,
                    "p95_ms": 3.0,
                    "p99_ms": 4.0,
                    "maximum_ms": 5.0,
                },
            },
            "worker": {
                "messages": 1000,
                "failures": 0,
                "error_rate": 0.0,
                "throughput_messages_per_second": 1000.0,
            },
            "artifacts": {"orders_local_bytes": 1_000_000},
        }

    def passing_baseline(self) -> dict:
        source_digest = json.loads(
            (self.root / "verification/source-manifest.json").read_text()
        )["source_tree_sha256"]
        dimensions = {
            "os": "linux",
            "os_release": "reviewed-hosted-image",
            "architecture": "x86_64",
            "python": "3.14.0",
            "github_actions": True,
        }
        fingerprint = hashlib.sha256(
            json.dumps(dimensions, separators=(",", ":"), sort_keys=True).encode()
        ).hexdigest()
        return {
            "schema_version": 1,
            "kind": "minco.performance-baseline.v1",
            "status": "PASS",
            "candidate_version": WORKSPACE_VERSION,
            "source_tree_sha256": source_digest,
            "source_revision": "a" * 40,
            "production_slo": False,
            "provider_contact": False,
            "topology": {"runtime": "local_native", "ingress": "local_tcp"},
            "runner": {
                "scope": "github_hosted",
                "repository": "xicv/minco",
                "source_sha": "a" * 40,
                "source_tree_sha256": source_digest,
                "run_id": "12345",
                "run_attempt": "1",
                "runner_os": "Linux",
                "runner_arch": "X64",
                "runner_image": "ubuntu-reviewed",
            },
            "environment": {
                "dimensions": dimensions,
                "fingerprint_sha256": fingerprint,
            },
            "classification": {"warm": True, "cold_start_measured": False},
            "baseline": self.passing_measurement(fingerprint),
            "candidate": self.passing_measurement(fingerprint),
            "limitations": ["Synthetic hosted qualification; not a production SLO."],
        }

    def test_not_run_evidence_is_valid_but_never_silent(self) -> None:
        report = self.report()
        self.assertEqual(report["status"], "PASS", report)
        repository_truth = tomllib.loads(
            (self.root / "verification/repository-truth.toml").read_text()
        )
        self.assertEqual(
            report["effective_date"],
            repository_truth["operational_evidence_effective_date"],
        )
        self.assertEqual(report["metrics"]["performance_status"], "NOT RUN")
        self.assertEqual(report["metrics"]["current_provider_profiles"], 0)
        self.assertIn("PERF-BASELINE-007", self.codes(report))
        self.assertIn("EVIDENCE-PROVIDER-021", self.codes(report))
        self.assertEqual(
            report["source_tree_sha256"],
            json.loads((self.root / "verification/source-manifest.json").read_text())["source_tree_sha256"],
        )
        self.assertIn("verification/provider-evidence.toml", report["inputs"])
        receipt_digest = report.pop("receipt_digest")
        self.assertEqual(
            receipt_digest,
            hashlib.sha256(
                json.dumps(
                    report,
                    allow_nan=False,
                    separators=(",", ":"),
                    sort_keys=True,
                ).encode()
            ).hexdigest(),
        )

    def test_report_is_byte_deterministic_and_override_is_explicit(self) -> None:
        first = self.report()
        second = self.report()
        self.assertEqual(
            json.dumps(first, allow_nan=False, sort_keys=True),
            json.dumps(second, allow_nan=False, sort_keys=True),
        )
        overridden = VALIDATOR.Validator(
            self.root,
            effective_date_override=date(2026, 8, 6),
        ).run()
        self.assertEqual(overridden["effective_date"], "2026-08-06")
        self.assertEqual(overridden["effective_date_source"], "cli")

    def test_check_output_fails_closed_on_a_stale_or_forged_receipt(self) -> None:
        receipt = self.root / "verification/operational-evidence-validation.json"
        receipt.write_text(
            json.dumps(self.report(), allow_nan=False, indent=2, sort_keys=True) + "\n"
        )
        command = [
            sys.executable,
            str(ROOT / "scripts/validate_operational_evidence.py"),
            "--root",
            str(self.root),
            "--check-output",
            "verification/operational-evidence-validation.json",
        ]
        self.assertEqual(
            subprocess.run(command, check=False, capture_output=True).returncode,
            0,
        )
        value = json.loads(receipt.read_text())
        value["status"] = "PASS"
        value["metrics"]["current_provider_profiles"] = 99
        receipt.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
        self.assertNotEqual(
            subprocess.run(command, check=False, capture_output=True).returncode,
            0,
        )

    def test_require_current_provider_fails_without_live_exact_source_proof(self) -> None:
        report = self.report(require_current_provider=True)
        self.assertEqual(report["status"], "FAIL")
        finding = next(item for item in report["findings"] if item["code"] == "EVIDENCE-PROVIDER-021")
        self.assertEqual(finding["severity"], "error")

    def test_stale_source_manifest_and_wrong_baseline_source_fail_closed(self) -> None:
        (self.root / "included-source.txt").write_text("mutation")
        report = self.report()
        self.assertEqual(report["status"], "FAIL")
        self.assertIn("EVIDENCE-SOURCE-004", self.codes(report))

        (self.root / "included-source.txt").unlink()
        self.refresh_manifest_and_baseline()
        baseline_path = self.root / "verification/1.7-performance-baseline.json"
        baseline = json.loads(baseline_path.read_text())
        baseline["source_tree_sha256"] = "f" * 64
        baseline_path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        report = self.report()
        self.assertIn("PERF-BASELINE-003", self.codes(report))

    def test_non_finite_json_and_production_slo_claim_fail_closed(self) -> None:
        path = self.root / "verification/1.7-performance-baseline.json"
        path.write_text(
            path.read_text().replace(
                f'"candidate_version": "{WORKSPACE_VERSION}"',
                '"candidate_version": NaN',
            )
        )
        report = self.report()
        self.assertIn("PERF-DATA-001", self.codes(report))

        shutil.copy2(ROOT / "verification/1.7-performance-baseline.json", path)
        self.refresh_manifest_and_baseline()
        policy = self.root / "verification/performance-policy.toml"
        policy.write_text(policy.read_text().replace("production_slo = false", "production_slo = true"))
        self.refresh_manifest_and_baseline()
        report = self.report()
        self.assertIn("PERF-POLICY-002", self.codes(report))

    def test_provider_contact_cleanup_and_freshness_mutations_fail_closed(self) -> None:
        path = self.root / "verification/provider-evidence.toml"
        source = path.read_text().replace("provider_contact = false", "provider_contact = true", 1)
        path.write_text(source)
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-PROVIDER-014", self.codes(self.report()))

        shutil.copy2(ROOT / "verification/provider-evidence.toml", path)
        self.replace_historical_digest()
        path.write_text(path.read_text().replace('evidence_state = "stale"', 'evidence_state = "current"'))
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-PROVIDER-020", self.codes(self.report()))

    def test_current_provider_requires_meaningful_non_circular_receipt(self) -> None:
        self.configure_current_provider(write_receipt=False)
        failed = self.report(require_current_provider=True)
        self.assertEqual(failed["status"], "FAIL")
        self.assertIn("EVIDENCE-PROVIDER-025", self.codes(failed))

        self.write_current_provider_receipt()
        passed = self.report(require_current_provider=True)
        self.assertEqual(passed["status"], "PASS", passed)
        self.assertEqual(passed["metrics"]["current_provider_profiles"], 1)
        self.assertNotIn("EVIDENCE-PROVIDER-021", self.codes(passed))
        before = json.loads((self.root / "verification/source-manifest.json").read_text())["source_tree_sha256"]
        self.refresh_manifest_and_baseline()
        after = json.loads((self.root / "verification/source-manifest.json").read_text())["source_tree_sha256"]
        self.assertEqual(before, after, "excluded receipt must not create a source-digest cycle")

    def test_fabricated_current_provider_without_artifacts_fails_closed(self) -> None:
        path = self.root / "verification/provider-evidence.toml"
        source = path.read_text().replace('evidence_state = "not_run"', 'evidence_state = "current"', 1)
        source = source.replace(
            'source_revision = "no-observed-source-no-provider-contact"',
            'source_revision = "receipt_bound"',
            1,
        )
        source = source.replace('observed_at = ""', 'observed_at = "2026-08-07T00:00:00Z"', 1)
        source = source.replace('provider_contact = false', 'provider_contact = true', 1)
        source = source.replace('cleanup_state = "not_required_no_contact"', 'cleanup_state = "verified_absent"', 1)
        path.write_text(source)
        self.refresh_manifest_and_baseline()
        report = self.report()
        self.assertIn("EVIDENCE-PROVIDER-023", self.codes(report))

    def test_provider_evidence_rejects_symlinked_parent_escape(self) -> None:
        outside = self.root.parent / f"{self.root.name}-outside-proof.json"
        outside.write_text("outside\n")
        self.addCleanup(outside.unlink)
        (self.root / "verification/link").symlink_to(outside.parent, target_is_directory=True)
        digest = hashlib.sha256(outside.read_bytes()).hexdigest()
        path = self.root / "verification/provider-evidence.toml"
        source = path.read_text().replace('evidence_paths = ["VERIFICATION.md"]', f'evidence_paths = ["verification/link/{outside.name}"]')
        source = source.replace(
            next(iter(tomllib.loads(source)["profile"][1]["evidence_sha256"])),
            digest,
        )
        path.write_text(source)
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-PROVIDER-011", self.codes(self.report()))

    def test_pass_baseline_requires_closed_hosted_provenance(self) -> None:
        path = self.root / "verification/1.7-performance-baseline.json"
        source_digest = json.loads((self.root / "verification/source-manifest.json").read_text())["source_tree_sha256"]
        path.write_text(json.dumps({
            "schema_version": 1,
            "kind": "minco.performance-baseline.v1",
            "status": "PASS",
            "candidate_version": WORKSPACE_VERSION,
            "source_tree_sha256": source_digest,
            "source_revision": "a" * 40,
            "production_slo": False,
            "provider_contact": False,
            "topology": {"runtime": "local_native", "ingress": "local_tcp"},
            "runner": {"scope": "github_hosted"},
            "environment": {"dimensions": {}, "fingerprint_sha256": hashlib.sha256(b"{}").hexdigest()},
            "classification": {"warm": "yes", "cold_start_measured": None},
            "baseline": {},
            "candidate": {},
            "limitations": ["test"],
        }, indent=2, sort_keys=True) + "\n")
        report = self.report()
        self.assertTrue({"PERF-BASELINE-011", "PERF-BASELINE-012", "PERF-BASELINE-014"}.issubset(self.codes(report)))

    def test_pass_baseline_binds_runner_to_the_verified_source_tree(self) -> None:
        path = self.root / "verification/1.7-performance-baseline.json"
        baseline = self.passing_baseline()
        path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        self.assertEqual(self.report()["status"], "PASS")

        baseline["runner"]["source_tree_sha256"] = "f" * 64
        path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        self.assertIn("PERF-BASELINE-016", self.codes(self.report()))

    def test_worker_failure_count_and_error_rate_must_agree(self) -> None:
        path = self.root / "verification/1.7-performance-baseline.json"
        baseline = self.passing_baseline()
        baseline["candidate"]["worker"]["failures"] = 1001
        path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        self.assertIn("PERF-MEASURE-011", self.codes(self.report()))

        baseline = self.passing_baseline()
        baseline["candidate"]["worker"]["failures"] = 1
        baseline["candidate"]["worker"]["error_rate"] = 0.0
        path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        self.assertIn("PERF-MEASURE-012", self.codes(self.report()))

    def test_malformed_record_types_return_stable_fail_findings(self) -> None:
        baseline_path = self.root / "verification/1.7-performance-baseline.json"
        baseline = self.passing_baseline()
        baseline["candidate"]["api"]["requests"] = 0
        baseline["candidate"]["api"]["failures"] = 0
        baseline_path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        report = self.report()
        self.assertEqual(report["status"], "FAIL")
        self.assertIn("PERF-MEASURE-006", self.codes(report))

        policy_path = self.root / "verification/performance-policy.toml"
        policy_path.write_text(
            policy_path.read_text().replace(
                "[minimum_samples]\napi_requests = 80\nworker_messages = 1000",
                'minimum_samples = "invalid"',
            )
        )
        report = self.report()
        self.assertEqual(report["status"], "FAIL")
        self.assertIn("PERF-POLICY-003", self.codes(report))

        shutil.copy2(ROOT / "verification/performance-policy.toml", policy_path)
        provider_path = self.root / "verification/provider-evidence.toml"
        provider_path.write_text(
            provider_path.read_text().replace("max_age_days = 7", 'max_age_days = "7"')
        )
        report = self.report()
        self.assertEqual(report["status"], "FAIL")
        self.assertIn("EVIDENCE-PROVIDER-008", self.codes(report))

        shutil.copy2(ROOT / "verification/provider-evidence.toml", provider_path)
        capability_path = self.root / "verification/aws-capability-candidates.toml"
        capability_path.write_text(
            capability_path.read_text().replace(
                "upstream_sources = [",
                'upstream_sources = "invalid" # [',
                1,
            )
        )
        report = self.report()
        self.assertEqual(report["status"], "FAIL")
        self.assertIn("EVIDENCE-CAPABILITY-009", self.codes(report))

    def test_supported_capability_requires_implementation_tests_and_live_proof(self) -> None:
        path = self.root / "verification/aws-capability-candidates.toml"
        path.write_text(path.read_text().replace('support_state = "declared"', 'support_state = "supported"', 1))
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-CAPABILITY-012", self.codes(self.report()))

    def test_evidence_artifact_digest_and_symlink_fail_closed(self) -> None:
        (self.root / "VERIFICATION.md").write_text("changed historical evidence\n")
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-PROVIDER-012", self.codes(self.report()))

        (self.root / "VERIFICATION.md").unlink()
        (self.root / "real-evidence.md").write_text("real\n")
        (self.root / "VERIFICATION.md").symlink_to(self.root / "real-evidence.md")
        self.refresh_manifest_and_baseline()
        self.assertIn("EVIDENCE-PROVIDER-011", self.codes(self.report()))

    def test_source_manifest_exclusions_are_narrow(self) -> None:
        for relative in (
            "verification/performance-policy.toml",
            "verification/provider-evidence.toml",
            "verification/aws-capability-candidates.toml",
            "scripts/validate_operational_evidence.py",
            "scripts/test/operational_evidence.py",
            "docs/adrs/0037-release-bound-delivery-evidence.md",
            "docs/research/aws-rust-capability-review-2026-08.md",
        ):
            path = ROOT / relative
            self.assertTrue(SOURCE_MANIFEST.included(ROOT, path), relative)
        for relative in (
            "verification/1.7-performance-baseline.json",
            "verification/handover.json",
            "verification/handover.md",
            "verification/handover/client.json",
            "verification/operational-evidence-validation.json",
            "verification/static-validation.json",
            "verification/deep-review.json",
            "verification/publish-validation.json",
            "verification/feedback-task-receipts/example.json",
            "verification/provider-evidence-receipts/example.json",
        ):
            self.assertFalse(SOURCE_MANIFEST.included(ROOT, ROOT / relative), relative)


if __name__ == "__main__":
    unittest.main()
