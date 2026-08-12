#!/usr/bin/env python3
"""Contract tests for the golden-topology cost regression gate."""
from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "cost_regression.py"
SPEC = importlib.util.spec_from_file_location("minco_cost_regression", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cost regression module is unavailable")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def projection(*, request_resource: str = "http_api") -> dict:
    return {
        "database": {
            "provider": "neon_free",
            "complete": False,
            "monthly_usd": None,
            "components": [],
            "missing_rates": ["current allowance"],
            "evidence": [
                {
                    "name": "compute_allowance",
                    "cost_class": "zero_compute",
                    "pricing_confidence": "free_tier_dependent",
                }
            ],
            "notes": ["dated pricing limitation"],
        },
        "runtime": {
            "complete": False,
            "schedules": [],
            "workers": [],
            "queues": [],
            "realtime": None,
            "fixed_cost_resources": [],
            "request_based_resources": [request_resource, "lambda:api"],
            "missing_rates": [
                "regional_api_gateway_request_rate",
                "regional_lambda_request_and_duration_rates",
            ],
            "evidence": [
                {
                    "name": request_resource,
                    "cost_class": "request_only",
                    "pricing_confidence": "region_dependent",
                }
            ],
        },
        "database_profile": "neon_postgres",
        "structural_diagnostics": [],
        "overall_estimate_complete": False,
        "note": "human explanation is intentionally outside the cost authority",
    }


class CostRegressionTests(unittest.TestCase):
    def test_canonical_projection_ignores_only_the_human_note(self) -> None:
        expected = MODULE.canonical_projection(projection())
        changed_note = projection()
        changed_note["note"] = "wording changed"
        self.assertEqual(MODULE.canonical_projection(changed_note), expected)

        changed_resource = projection(request_resource="lambda_function_url")
        self.assertNotEqual(MODULE.canonical_projection(changed_resource), expected)

    def test_report_rejects_duplicate_profile_ids_and_paths(self) -> None:
        profile = {
            "id": "orders-neon-free",
            "config": "examples/orders/config/minco.dev.toml",
            "config_sha256": "0" * 64,
            "projection_sha256": "1" * 64,
            "projection": MODULE.canonical_projection(projection()),
        }
        report = {
            "schema_version": 1,
            "kind": "minco.topology-cost-regression.v1",
            "provider_contact": False,
            "production_budget": False,
            "profiles": [profile, dict(profile)],
            "limitations": ["not a provider bill"],
        }
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

        report["profiles"][1] = {
            **profile,
            "id": "orders-neon-launch",
        }
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

        report["profiles"] = [
            {
                **profile,
                "projection": {**profile["projection"], "hidden": "authority"},
            }
        ]
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

        report["profiles"] = [{**profile, "projection": "forged"}]
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

        report["profiles"] = [{**profile, "id": "Orders/escape"}]
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

        report["profiles"] = [profile]
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-003"):
            MODULE.validate_report(report)

    def test_boolean_schema_version_is_not_integer_one(self) -> None:
        report = {
            "schema_version": True,
            "kind": "minco.topology-cost-regression.v1",
            "provider_contact": False,
            "production_budget": False,
            "profiles": [],
            "limitations": ["not a provider bill"],
        }
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-001"):
            MODULE.validate_report(report)

    def test_current_report_detects_config_and_projection_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / "examples/orders/config/minco.dev.toml"
            config.parent.mkdir(parents=True)
            config.write_text("schema_version = 1\n")
            current = MODULE.build_report(
                root=root,
                profiles=(("orders-neon-free", config.relative_to(root)),),
                runner=lambda _root, _path: projection(),
            )
            MODULE.validate_current_report(
                current,
                root=root,
                profiles=(("orders-neon-free", config.relative_to(root)),),
                runner=lambda _root, _path: projection(),
            )

            config.write_text("schema_version = 2\n")
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-004"):
                MODULE.validate_current_report(
                    current,
                    root=root,
                    profiles=(("orders-neon-free", config.relative_to(root)),),
                    runner=lambda _root, _path: projection(),
                )

            config.write_text("schema_version = 1\n")
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-004"):
                MODULE.validate_current_report(
                    current,
                    root=root,
                    profiles=(("orders-neon-free", config.relative_to(root)),),
                    runner=lambda _root, _path: projection(
                        request_resource="lambda_function_url"
                    ),
                )

    def test_canonical_bytes_and_non_finite_numbers_fail_closed(self) -> None:
        non_finite = projection()
        non_finite["database"]["monthly_usd"] = float("nan")
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-002"):
            MODULE.canonical_projection(non_finite)

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "baseline.json"
            path.write_text(json.dumps({"schema_version": 1}))
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-001"):
                MODULE.load_canonical_report(path)
            path.unlink()
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-001"):
                MODULE.load_canonical_report(path)

    def test_cli_failures_are_bounded_and_do_not_echo_stderr(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "target/debug/cargo-minco"
            binary.parent.mkdir(parents=True)
            binary.write_text("test fixture")

            with mock.patch.object(
                MODULE.subprocess,
                "run",
                side_effect=MODULE.subprocess.TimeoutExpired(["cost"], 30),
            ):
                with self.assertRaisesRegex(
                    ValueError,
                    r"^COST-REGRESSION-007: .* exceeded 30 seconds$",
                ):
                    MODULE.run_cost_cli(root, Path("config.toml"))

            failed = MODULE.subprocess.CompletedProcess(
                args=["cost"],
                returncode=2,
                stdout="",
                stderr="OPERATOR_TOKEN=must-not-appear",
            )
            with mock.patch.object(MODULE.subprocess, "run", return_value=failed):
                with self.assertRaisesRegex(
                    ValueError,
                    r"^COST-REGRESSION-007: .* exit 2$",
                ) as failure:
                    MODULE.run_cost_cli(root, Path("config.toml"))
            self.assertNotIn("must-not-appear", str(failure.exception))

    def test_config_symlinks_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "real.toml"
            target.write_text("schema_version = 1\n")
            config = root / "examples/orders/config/minco.dev.toml"
            config.parent.mkdir(parents=True)
            config.symlink_to(target)
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-005"):
                MODULE.build_report(
                    root=root,
                    profiles=(("orders-neon-free", config.relative_to(root)),),
                    runner=lambda _root, _path: projection(),
                )

    def test_missing_config_fails_with_stable_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "COST-REGRESSION-005"):
                MODULE.build_report(
                    root=root,
                    profiles=(
                        (
                            "orders-neon-free",
                            Path("examples/orders/config/missing.toml"),
                        ),
                    ),
                    runner=lambda _root, _path: projection(),
                )

    def test_golden_policy_rejects_hidden_aws_cost_in_local_topology(self) -> None:
        profiles = []
        fixed = {
            "orders-local-sqlite": ["database:sqlite_persistent_host"],
            "orders-rds-postgres": ["database:rds_postgres"],
            "orders-self-hosted-postgres": ["database:self_hosted_postgres"],
        }
        for identifier, config in MODULE.PROFILES:
            value = projection()
            if identifier == "orders-local-sqlite":
                value["runtime"]["request_based_resources"] = []
                value["runtime"]["missing_rates"] = []
                value["runtime"]["evidence"] = []
                value["runtime"]["complete"] = True
                value["overall_estimate_complete"] = True
            value["runtime"]["fixed_cost_resources"] = fixed.get(identifier, [])
            canonical = MODULE.canonical_projection(value)
            profiles.append(
                {
                    "id": identifier,
                    "config": config.as_posix(),
                    "config_sha256": "0" * 64,
                    "projection_sha256": MODULE.sha256_bytes(
                        MODULE.canonical_bytes(canonical)
                    ),
                    "projection": canonical,
                }
            )
        report = {
            "schema_version": 1,
            "kind": "minco.topology-cost-regression.v1",
            "provider_contact": False,
            "production_budget": False,
            "profiles": profiles,
            "limitations": ["not a provider bill"],
        }
        MODULE.validate_golden_invariants(report)
        report["profiles"][0]["projection"]["runtime"][
            "request_based_resources"
        ] = ["sqs:hidden"]
        with self.assertRaisesRegex(ValueError, "COST-REGRESSION-009"):
            MODULE.validate_golden_invariants(report)


if __name__ == "__main__":
    unittest.main(verbosity=2)
