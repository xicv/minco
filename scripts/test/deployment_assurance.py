#!/usr/bin/env python3
"""Mutation tests for the deployment-assurance validator."""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "validate_deployment_assurance.py"
SPEC = importlib.util.spec_from_file_location("validate_deployment_assurance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)
Validator = MODULE.Validator

RELEASE_COMMIT = "4d81543f7c5adb773655f23278abfe084de9f3e0"


class DeploymentAssuranceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.write_fixture()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, relative: str, content: str = "evidence\n") -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def write_fixture(self) -> None:
        self.write(
            "verification/repository-truth.toml",
            f'published_baseline = "1.1.0"\npublished_release_commit = "{RELEASE_COMMIT}"\n',
        )
        self.write(
            "crates/minco-plan/src/model.rs",
            """
pub enum RuntimePlan {
    LambdaZipArm64,
    LocalNative,
}

pub enum IngressPlan {
    ApiGatewayHttpApi,
    LambdaFunctionUrl,
    LocalTcp,
}
""".lstrip(),
        )
        self.write(
            "crates/minco-plan/src/cost.rs",
            """
pub enum CostClass {
    ZeroCompute,
    RequestOnly,
    StorageOnly,
    ScheduledWakeup,
    FixedMonthly,
}
""".lstrip(),
        )
        self.write(
            "crates/minco-plan/src/sam.rs",
            "AWS::Serverless::HttpApi\nStageVariables:\nAutoPublishAlias: candidate\n",
        )
        for path in [
            "source.rs",
            "evidence.md",
            "docs.md",
            "scripts/candidate-load.sh",
            "scripts/candidate-recovery.sh",
        ]:
            self.write(path)
        self.write(
            "verification/deployment-assurance.toml",
            f"""
schema = 1
published_baseline = "1.1.0"
reviewed_release_commit = "{RELEASE_COMMIT}"
default_profile = "aws-http-api-lambda"

[policy]
required_aws_dimensions = ["contract", "code", "cost", "security", "performance", "recovery", "provider"]
required_local_dimensions = ["contract", "code", "performance"]
forbidden_default_cost_classes = ["fixed_monthly", "scheduled_wakeup"]
forbidden_default_wake_sources = ["poller", "schedule"]

[[profile]]
id = "aws-http-api-lambda"
status = "stable"
scope = "aws"
runtime = "LambdaZipArm64"
ingress = "ApiGatewayHttpApi"
default = true
zero_provisioned_compute = true
idle_cost_classes = ["zero_compute", "request_only", "storage_only"]
wake_sources = ["http_request"]
dimensions = ["contract", "code", "cost", "security", "performance", "recovery", "provider"]
source_paths = ["source.rs"]
evidence_paths = ["evidence.md", "scripts/candidate-load.sh", "scripts/candidate-recovery.sh"]
documentation_paths = ["docs.md"]
test_commands = ["scripts/candidate-load.sh", "scripts/candidate-recovery.sh"]
decision = "default"
blockers = []

[[profile]]
id = "aws-lambda-function-url"
status = "declared"
scope = "aws"
runtime = "LambdaZipArm64"
ingress = "LambdaFunctionUrl"
default = false
zero_provisioned_compute = true
idle_cost_classes = ["zero_compute", "request_only"]
wake_sources = ["http_request"]
dimensions = ["contract", "cost"]
source_paths = ["source.rs"]
evidence_paths = ["evidence.md"]
documentation_paths = ["docs.md"]
test_commands = []
decision = "not implemented"
blockers = ["renderer"]

[[profile]]
id = "local-native"
status = "stable"
scope = "local"
runtime = "LocalNative"
ingress = "LocalTcp"
default = false
zero_provisioned_compute = false
idle_cost_classes = []
wake_sources = ["developer_process"]
dimensions = ["contract", "code", "performance"]
source_paths = ["source.rs"]
evidence_paths = ["evidence.md"]
documentation_paths = ["docs.md"]
test_commands = ["local test"]
decision = "local"
blockers = []
""".lstrip(),
        )

    def findings(self) -> set[str]:
        return {finding["code"] for finding in Validator(self.root).run()["findings"]}

    def replace(self, relative: str, old: str, new: str) -> None:
        path = self.root / relative
        source = path.read_text()
        self.assertIn(old, source)
        path.write_text(source.replace(old, new, 1))

    def test_valid_fixture_passes(self) -> None:
        report = Validator(self.root).run()
        self.assertEqual(report["status"], "ok", report)

    def test_missing_ingress_variant_fails_closed(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'ingress = "LambdaFunctionUrl"',
            'ingress = "ApiGatewayHttpApi"',
        )
        self.assertIn("ASSURANCE-ENUM-007", self.findings())

    def test_function_url_cannot_be_promoted_without_renderer_support(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'id = "aws-lambda-function-url"\nstatus = "declared"',
            'id = "aws-lambda-function-url"\nstatus = "bounded"',
        )
        self.assertIn("ASSURANCE-INGRESS-002", self.findings())

    def test_new_function_url_renderer_requires_assurance_review(self) -> None:
        path = self.root / "crates/minco-plan/src/sam.rs"
        path.write_text(path.read_text() + "AWS::Lambda::Url\n")
        self.assertIn("ASSURANCE-INGRESS-003", self.findings())

    def test_default_profile_selector_matches_default_flag(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'default_profile = "aws-http-api-lambda"',
            'default_profile = "local-native"',
        )
        self.assertIn("ASSURANCE-DEFAULT-008", self.findings())

    def test_default_profile_rejects_scheduled_cost(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'idle_cost_classes = ["zero_compute", "request_only", "storage_only"]',
            'idle_cost_classes = ["zero_compute", "request_only", "scheduled_wakeup"]',
        )
        self.assertIn("ASSURANCE-DEFAULT-005", self.findings())

    def test_supported_aws_profile_requires_all_dimensions(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'wake_sources = ["http_request"]\ndimensions = ["contract", "code", "cost", "security", "performance", "recovery", "provider"]',
            'wake_sources = ["http_request"]\ndimensions = ["contract", "code", "cost", "security", "recovery", "provider"]',
        )
        self.assertIn("ASSURANCE-DIMENSION-001", self.findings())

    def test_missing_evidence_path_is_reported(self) -> None:
        self.replace(
            "verification/deployment-assurance.toml",
            'documentation_paths = ["docs.md"]',
            'documentation_paths = ["missing.md"]',
        )
        self.assertIn("ASSURANCE-PATH-002", self.findings())


if __name__ == "__main__":
    unittest.main()
