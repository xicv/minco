#!/usr/bin/env python3
"""Behavioral policy tests for Minco's bounded hosted CI surface."""
from __future__ import annotations

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
ESSENTIAL = ROOT / "scripts/ci/hosted-essential.sh"
LOCAL_RELEASE = ROOT / "scripts/ci/local-release.sh"
LOCAL_RUNTIME = ROOT / "scripts/ci/local-runtime.sh"
WORKFLOW = ROOT / ".github/workflows/minco-manual.yml"
WORKFLOW_DIRECTORY = ROOT / ".github/workflows"
PUBLISH_WORKFLOW = WORKFLOW_DIRECTORY / "publish-crates.yml"
DOCS_PLAYWRIGHT = ROOT / "docs-site/playwright.config.mts"
AGENT_WORKFLOWS_PATH = ROOT / "scripts/test/agent_workflows.py"


def load_agent_workflows():
    spec = importlib.util.spec_from_file_location(
        "hosted_policy_agent_workflows", AGENT_WORKFLOWS_PATH
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class HostedCiPolicyTests(unittest.TestCase):
    def test_agent_workflow_receipts_are_confined_to_verification(self) -> None:
        agent_workflows = load_agent_workflows()
        with self.assertRaisesRegex(ValueError, "remain under verification"):
            agent_workflows.confined_evidence_path(Path("../outside.json"))
        path, relative = agent_workflows.confined_evidence_path(
            Path("verification/agent-workflows.json")
        )
        self.assertEqual(path, ROOT / "verification/agent-workflows.json")
        self.assertEqual(relative, "verification/agent-workflows.json")

    def test_docs_browser_server_owns_a_configurable_strict_port(self) -> None:
        config = DOCS_PLAYWRIGHT.read_text()
        self.assertIn("MINCO_DOCS_PORT", config)
        self.assertIn("--strictPort", config)
        self.assertNotIn("'http://127.0.0.1:4173/minco/'", config)

    def test_hosted_essential_runs_only_bounded_clean_runner_commands(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-hosted-ci-") as temporary:
            root = Path(temporary)
            binary_dir = root / "bin"
            binary_dir.mkdir()
            command_log = root / "commands.log"
            for command in ("cargo", "uv"):
                wrapper = binary_dir / command
                wrapper.write_text(
                    "#!/usr/bin/env bash\n"
                    "set -euo pipefail\n"
                    f"printf '{command}' >> \"$MINCO_CI_COMMAND_LOG\"\n"
                    "printf ' <%s>' \"$@\" >> \"$MINCO_CI_COMMAND_LOG\"\n"
                    "printf '\\n' >> \"$MINCO_CI_COMMAND_LOG\"\n"
                )
                wrapper.chmod(0o755)

            environment = os.environ.copy()
            environment["PATH"] = f"{binary_dir}:{environment['PATH']}"
            environment["MINCO_CI_COMMAND_LOG"] = str(command_log)
            subprocess.run(
                ["bash", str(ESSENTIAL)],
                cwd=ROOT,
                env=environment,
                check=True,
            )

            self.assertEqual(
                command_log.read_text().splitlines(),
                [
                    "uv <run> <--locked> <python> <scripts/validate_static.py>",
                    "cargo <build> <--quiet> <--locked> <-p> <cargo-minco>",
                    "uv <run> <--locked> <python> <scripts/docs/generate_reference.py> <--check>",
                    "uv <run> <--locked> <python> <scripts/test/agent_workflows.py> <--check-output> <verification/agent-workflows.json>",
                    "uv <run> <--locked> <python> <scripts/test/repository_truth.py>",
                    "uv <run> <--locked> <python> <scripts/validate_deployment_assurance.py>",
                    "uv <run> <--locked> <python> <scripts/test/deployment_assurance.py>",
                    "uv <run> <--locked> <python> <scripts/test/current_product_truth.py>",
                    "uv <run> <--locked> <python> <scripts/validate_operational_evidence.py> <--check-output> <verification/operational-evidence-validation.json>",
                    "uv <run> <--locked> <python> <scripts/test/operational_evidence.py>",
                    "uv <run> <--locked> <python> <scripts/test/hosted_ci_policy.py>",
                    "uv <run> <--locked> <python> <scripts/test/examples/validate.py> <--check>",
                    "cargo <fmt> <--all> <--> <--check>",
                    "cargo <check> <--workspace> <--all-targets> <--all-features> <--locked>",
                    "cargo <test> <-p> <cargo-minco> <--test> <agent_skills> <--locked>",
                    "uv <run> <--locked> <python> <scripts/source_manifest.py> <--check>",
                ],
            )

    def test_repository_has_only_platform_required_workflows(self) -> None:
        workflow_files = [
            *WORKFLOW_DIRECTORY.glob("*.yml"),
            *WORKFLOW_DIRECTORY.glob("*.yaml"),
        ]
        self.assertEqual(
            {path.name for path in workflow_files},
            {"docs-pages.yml", "minco-manual.yml", "publish-crates.yml"},
        )

    def test_manual_workflow_is_only_a_bounded_clean_runner_check(self) -> None:
        workflow = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(workflow["on"]), {"workflow_dispatch"})
        self.assertNotIn("inputs", workflow["on"]["workflow_dispatch"] or {})

        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertEqual(workflow["jobs"]["quality"]["timeout-minutes"], "20")
        self.assertEqual(
            workflow["concurrency"],
            {
                "group": "${{ github.workflow }}-${{ github.ref }}",
                "cancel-in-progress": "true",
            },
        )

        steps = {
            step.get("name", step.get("uses", "")): step
            for step in workflow["jobs"]["quality"]["steps"]
        }
        essential = steps["Essential clean-runner gate"]
        self.assertEqual(essential["run"], "scripts/ci/hosted-essential.sh")
        self.assertNotIn("if", essential)

        workflow_source = WORKFLOW.read_text()
        for forbidden in (
            "Swatinem/rust-cache",
            "scripts/quality.sh",
            "upload-artifact",
            "cargo-lambda",
            "clippy",
            "setup-zig",
            "rustack",
            "scripts/test/e2e.sh",
            "profile:",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, workflow_source)

    def test_local_release_retains_the_removed_release_matrix(self) -> None:
        release = LOCAL_RELEASE.read_text()
        required_commands = [
            "./scripts/quality.sh",
            "proofs/realtime-appsync/scripts/test-local.sh",
            "scripts/release/candidate-recovery.sh",
            "scripts/release/candidate-load.sh",
            "scripts/release/publish.sh --skip-quality",
            "scripts/aws/plan.sh",
            "scripts/aws/validate.sh",
            "scripts/aws/build-lambda.sh",
            "scripts/aws/build-worker-lambda.sh",
            "scripts/ci/local-runtime.sh",
            "scripts/dev/rustack-smoke.sh",
            "scripts/test/e2e.sh",
        ]
        positions = [release.index(command) for command in required_commands]
        self.assertEqual(positions, sorted(positions))

    def test_publish_workflow_does_not_repeat_authoritative_local_quality(self) -> None:
        workflow = yaml.load(PUBLISH_WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertEqual(
            workflow["jobs"]["release"]["permissions"],
            {"contents": "read", "id-token": "write"},
        )

        source = PUBLISH_WORKFLOW.read_text()
        for forbidden in (
            "Swatinem/rust-cache",
            "jj-cli",
            "ripgrep",
            "cargo fmt",
            "cargo check",
            "cargo clippy",
            "cargo test --workspace",
            "scripts/test/generated_apps.sh",
            "cargo doc",
            "scripts/quality.sh",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)

    def test_publish_workflow_prefetches_locked_dependencies_before_offline_archive_tests(
        self,
    ) -> None:
        workflow = yaml.load(PUBLISH_WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        steps = workflow["jobs"]["release"]["steps"]
        named_steps = {
            step.get("name", step.get("uses", "")): (index, step)
            for index, step in enumerate(steps)
        }

        fetch_index, fetch = named_steps[
            "Fetch locked Rust dependencies for offline archive tests"
        ]
        verify_index, _ = named_steps["Verify release tag"]
        token_index, _ = named_steps["Obtain short-lived crates.io token"]
        publish_index, _ = named_steps["Publish selected crate family"]
        self.assertEqual(fetch["run"], "cargo fetch --locked")
        self.assertLess(verify_index, fetch_index)
        self.assertLess(fetch_index, token_index)
        self.assertLess(token_index, publish_index)

    def test_local_runtime_qualifies_owned_postgres_and_rustack(self) -> None:
        runtime = LOCAL_RUNTIME.read_text()
        for required in (
            "trap cleanup EXIT",
            "cargo build --locked -p cargo-minco",
            "__local-service start postgres",
            '__local-service stop "$service"',
            "stop_service postgres",
            "__local-service start rustack",
            "stop_service rustack",
            "--aws-services sts",
        ):
            with self.subTest(required=required):
                self.assertIn(required, runtime)

    def test_local_quality_retains_the_complete_authoritative_matrix(self) -> None:
        quality = (ROOT / "scripts/quality.sh").read_text()
        required_commands = [
            "scripts/validate_static.py --output verification/static-validation.json",
            "scripts/docs/generate-reference.sh --check",
            "scripts/test/agent_workflows.py --check-output verification/agent-workflows.json",
            "scripts/test/repository_truth.py",
            "scripts/validate_deployment_assurance.py",
            "scripts/validate_operational_evidence.py",
            "scripts/test/operational_evidence.py",
            "scripts/test/deployment_assurance.py",
            "scripts/test/current_product_truth.py",
            "scripts/test/hosted_ci_policy.py",
            "scripts/test/examples/test_recipes.py",
            "scripts/test/examples/validate.py --check",
            "scripts/validate_publish.py --output verification/publish-validation.json",
            "scripts/deep_review.py",
            "scripts/test/feedback_browser.sh",
            "cargo fmt --all -- --check",
            "cargo check --workspace --all-targets --all-features --locked",
            "cargo test -p cargo-minco --test agent_skills --locked",
            "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
            "cargo test --workspace --all-targets --all-features --locked",
            "scripts/test/generated_apps.sh",
            "cargo doc --workspace --all-features --no-deps --locked",
            "cargo deny check advisories bans licenses sources",
            "cargo audit",
            "npm audit --prefix plugins/minco-plugin-feedback --audit-level=high",
            "gitleaks dir . --no-banner --redact",
            "scripts/source_manifest.py --check",
        ]
        for command in required_commands:
            with self.subTest(command=command):
                self.assertIn(command, quality)


if __name__ == "__main__":
    unittest.main()
