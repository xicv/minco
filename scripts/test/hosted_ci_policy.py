#!/usr/bin/env python3
"""Behavioral policy tests for Minco's bounded hosted CI surface."""
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
ESSENTIAL = ROOT / "scripts/ci/hosted-essential.sh"
WORKFLOW = ROOT / ".github/workflows/minco-manual.yml"
DOCS_PLAYWRIGHT = ROOT / "docs-site/playwright.config.mts"


class HostedCiPolicyTests(unittest.TestCase):
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
                    "uv <run> <--locked> <python> <scripts/test/repository_truth.py>",
                    "uv <run> <--locked> <python> <scripts/test/hosted_ci_policy.py>",
                    "uv <run> <--locked> <python> <scripts/test/examples/validate.py> <--check>",
                    "cargo <fmt> <--all> <--> <--check>",
                    "cargo <check> <--workspace> <--all-targets> <--all-features> <--locked>",
                    "uv <run> <--locked> <python> <scripts/source_manifest.py> <--check>",
                ],
            )

    def test_manual_workflow_defaults_to_bounded_essential_profile(self) -> None:
        workflow = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(workflow["on"]), {"workflow_dispatch"})

        dispatch = workflow["on"]["workflow_dispatch"]
        profile = dispatch["inputs"]["profile"]
        self.assertEqual(profile["default"], "essential")
        self.assertEqual(profile["type"], "choice")
        self.assertEqual(profile["options"], ["essential", "release"])

        self.assertEqual(workflow["permissions"], {"contents": "read"})
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
        self.assertEqual(
            essential["if"],
            "${{ inputs.profile == 'essential' }}",
        )

        rust_cache = next(
            step
            for step in workflow["jobs"]["quality"]["steps"]
            if step.get("uses", "").startswith("Swatinem/rust-cache@")
        )
        self.assertEqual(rust_cache["with"]["cache-targets"], "false")
        self.assertEqual(rust_cache["with"]["cache-on-failure"], "false")

        release_only = {
            "Set up release Node runtime",
            "Install release qualification tools",
            "Full release quality reproduction",
            "Standalone AppSync proof",
            "Upload Feedback browser evidence",
            "Publish dry run",
            "Install pinned Cargo Lambda",
            "Install pinned Zig",
            "Plan, SAM, and native ARM64 Lambda qualification",
            "Rustack and Minco SSM adapter conformance",
            "E2E",
        }
        self.assertTrue(release_only.issubset(steps))
        for name in release_only:
            self.assertIn("inputs.profile == 'release'", steps[name]["if"])

        self.assertEqual(
            steps["Standalone AppSync proof"]["run"],
            "proofs/realtime-appsync/scripts/test-local.sh",
        )

    def test_local_quality_retains_the_complete_authoritative_matrix(self) -> None:
        quality = (ROOT / "scripts/quality.sh").read_text()
        required_commands = [
            "scripts/validate_static.py --output verification/static-validation.json",
            "scripts/docs/generate-reference.sh --check",
            "scripts/test/repository_truth.py",
            "scripts/test/hosted_ci_policy.py",
            "scripts/test/examples/test_recipes.py",
            "scripts/test/examples/validate.py --check",
            "scripts/validate_publish.py --output verification/publish-validation.json",
            "scripts/deep_review.py",
            "scripts/test/feedback_browser.sh",
            "cargo fmt --all -- --check",
            "cargo check --workspace --all-targets --all-features --locked",
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
