#!/usr/bin/env python3
"""Behavioral policy tests for Minco's bounded hosted CI surface."""
from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import tomllib
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
STATIC_VALIDATOR_PATH = ROOT / "scripts/validate_static.py"
RUST_TOOLCHAIN = ROOT / "rust-toolchain.toml"
PYPROJECT = ROOT / "pyproject.toml"
DOCS_WORKFLOW = WORKFLOW_DIRECTORY / "docs-pages.yml"
EXPECTED_RUST_TOOLCHAIN_ACTION = "6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772"
PACKAGE_MANIFEST = "package" + ".json"
PACKAGE_LOCK = "package-lock" + ".json"


def load_agent_workflows():
    spec = importlib.util.spec_from_file_location(
        "hosted_policy_agent_workflows", AGENT_WORKFLOWS_PATH
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_static_validator():
    spec = importlib.util.spec_from_file_location(
        "hosted_policy_static_validator", STATIC_VALIDATOR_PATH
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class HostedCiPolicyTests(unittest.TestCase):
    def test_language_and_package_pins_stay_synchronized(self) -> None:
        toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text())["toolchain"]
        pyproject = tomllib.loads(PYPROJECT.read_text())
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())

        self.assertEqual(toolchain["channel"], "1.97.1")
        self.assertEqual(workspace["workspace"]["package"]["rust-version"], "1.97.1")
        self.assertEqual(pyproject["tool"]["uv"]["required-version"], "==0.12.3")

        dependencies = workspace["workspace"]["dependencies"]
        expected_cargo_versions = {
            "aws-config": "1.10",
            "aws-lc-rs": "1.18.0",
            "aws-sdk-s3": "1.141.0",
            "base64": "0.23",
            "clap": "4.6",
            "hex": "0.4.3",
            "hmac": "0.13",
            "http": "1.5",
            "rmcp": "3.1.2",
            "sha2": "0.11",
            "tokio": "1.53",
            "zeroize": "1.9",
        }
        for name, expected in expected_cargo_versions.items():
            specification = dependencies[name]
            observed = specification["version"] if isinstance(specification, dict) else specification
            self.assertEqual(observed, expected, name)

        docs_workflow = yaml.load(DOCS_WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        docs_steps = docs_workflow["jobs"]["build"]["steps"]
        setup_node = next(step for step in docs_steps if "actions/setup-node" in step.get("uses", ""))
        setup_uv = next(step for step in docs_steps if "astral-sh/setup-uv" in step.get("uses", ""))
        self.assertEqual(setup_node["with"]["node-version"], "24.19.0")
        self.assertEqual(setup_uv["with"]["version"], "0.12.3")

        for path, job in ((WORKFLOW, "quality"), (PUBLISH_WORKFLOW, "release")):
            workflow = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            steps = workflow["jobs"][job]["steps"]
            uv_step = next(step for step in steps if "astral-sh/setup-uv" in step.get("uses", ""))
            rust_step = next(step for step in steps if "dtolnay/rust-toolchain" in step.get("uses", ""))
            self.assertEqual(uv_step["with"]["version"], "0.12.3")
            self.assertEqual(rust_step["with"]["toolchain"], "1.97.1")
            self.assertEqual(
                rust_step["uses"],
                f"dtolnay/rust-toolchain@{EXPECTED_RUST_TOOLCHAIN_ACTION}",
            )

        package_roots = (
            ROOT / "docs-site",
            ROOT / "plugins/minco-plugin-feedback",
            ROOT / "proofs/realtime-appsync/browser",
            ROOT / "proofs/realtime-pusher/browser",
        )
        for package_root in package_roots:
            manifest = json.loads((package_root / PACKAGE_MANIFEST).read_text())
            lock = json.loads((package_root / PACKAGE_LOCK).read_text())
            self.assertEqual(manifest["devDependencies"]["@playwright/test"], "1.62.1")
            self.assertEqual(
                lock["packages"][""]["devDependencies"]["@playwright/test"],
                "1.62.1",
            )
            self.assertEqual(lock["packages"]["node_modules/@playwright/test"]["version"], "1.62.1")

        docs_package = json.loads((ROOT / "docs-site" / PACKAGE_MANIFEST).read_text())
        docs_lock = json.loads((ROOT / "docs-site" / PACKAGE_LOCK).read_text())
        self.assertEqual(docs_package["devDependencies"]["vitepress"], "1.6.4")
        self.assertEqual(docs_package["overrides"], {"nanoid": "3.3.18", "vite": "6.4.3"})
        self.assertEqual(docs_lock["packages"]["node_modules/vitepress"]["version"], "1.6.4")
        self.assertEqual(docs_lock["packages"]["node_modules/nanoid"]["version"], "3.3.18")
        self.assertEqual(docs_lock["packages"]["node_modules/vite"]["version"], "6.4.3")

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

    def test_static_validator_rejects_a_task_specific_workflow(self) -> None:
        validate_static = load_static_validator()
        with tempfile.TemporaryDirectory(prefix="minco-workflow-policy-") as temporary:
            root = Path(temporary)
            (root / ".github/workflows").mkdir(parents=True)
            (root / "quality.toml").write_text(
                "[gates.static]\ncommands = [\"uv run --locked python scripts/validate_static.py\"]\n\n"
                "[gates.rust]\ncommands = [\"cargo check --locked\"]\n\n"
                "[gates.security]\ncommands = [\"cargo deny check\"]\n\n"
                "[gates.e2e]\ncommands = [\"scripts/test/e2e.sh\"]\n"
            )
            for name in (
                "docs-pages.yml",
                "minco-manual.yml",
                "publish-crates.yml",
                "waffo-payments.yml",
            ):
                (root / ".github/workflows" / name).write_text("name: fixture\n")

            validator = validate_static.Validator(root)
            validator.validate_quality_configuration()
            workflow_allowlist_code = "STATIC-QUALITY-" + "005"

            self.assertTrue(
                any(
                    finding.code == workflow_allowlist_code
                    for finding in validator.findings
                ),
                "the standalone static gate must fail closed on workflow allowlist drift",
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
            "scripts/ci/local-assurance.sh --ephemeral",
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
            "scripts/test/quality_assurance.py",
            "scripts/quality_assurance.py",
            "scripts/test/release_identity.py",
            "scripts/release/release_identity.py --check",
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

    def test_measured_assurance_remains_local_only(self) -> None:
        local_release = LOCAL_RELEASE.read_text()
        self.assertIn("scripts/ci/local-assurance.sh --ephemeral", local_release)
        self.assertIn("cargo-nextest", local_release)
        self.assertIn("cargo-llvm-cov", local_release)
        self.assertIn("cargo-mutants", local_release)
        self.assertIn("cargo-semver-checks", local_release)
        for workflow in WORKFLOW_DIRECTORY.glob("*.yml"):
            with self.subTest(workflow=workflow.name):
                self.assertNotIn("local-assurance", workflow.read_text())

    def test_canonical_assurance_check_remains_available(self) -> None:
        assurance = (ROOT / "scripts/ci/local-assurance.sh").read_text()

        self.assertIn('mode="${1:-execute}"', assurance)
        self.assertIn('if [[ "$mode" == "--check" ]]', assurance)
        self.assertIn("--check-output verification/quality-assurance.json", assurance)

    def test_local_release_executes_ephemeral_assurance_without_tracked_outputs(self) -> None:
        local_release = LOCAL_RELEASE.read_text()
        assurance_path = ROOT / "scripts/ci/local-assurance.sh"

        self.assertIn("scripts/ci/local-assurance.sh --ephemeral", local_release)
        with tempfile.TemporaryDirectory(prefix="minco-assurance-wrapper-") as temporary:
            root = Path(temporary)
            binary_dir = root / "bin"
            tool_root = root / "tools"
            (tool_root / "bin").mkdir(parents=True)
            binary_dir.mkdir()
            command_log = root / "commands.log"
            (binary_dir / "uv").write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf 'uv' >> \"$MINCO_ASSURANCE_COMMAND_LOG\"\n"
                "printf ' <%s>' \"$@\" >> \"$MINCO_ASSURANCE_COMMAND_LOG\"\n"
                "printf '\\n' >> \"$MINCO_ASSURANCE_COMMAND_LOG\"\n"
            )
            (binary_dir / "rustup").write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf 'llvm-tools-aarch64-apple-darwin (installed)\\n'\n"
            )
            for path in binary_dir.iterdir():
                path.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{binary_dir}:{environment['PATH']}"
            environment["MINCO_QUALITY_TOOL_ROOT"] = str(tool_root)
            environment["MINCO_ASSURANCE_COMMAND_LOG"] = str(command_log)

            subprocess.run(
                ["bash", str(assurance_path), "--ephemeral"],
                cwd=ROOT,
                env=environment,
                check=True,
            )

            self.assertEqual(
                command_log.read_text().splitlines(),
                [
                    "uv <run> <--locked> <python> <scripts/quality_assurance.py> "
                    f"<--execute> <--tool-root> <{tool_root}> "
                    "<--output> <target/minco/quality-assurance/release-receipt.json> "
                    "<--performance-output> "
                    "<target/minco/quality-assurance/release-candidate-load.json>",
                    "uv <run> <--locked> <python> <scripts/quality_assurance.py> "
                    f"<--tool-root> <{tool_root}> <--check-output> "
                    "<target/minco/quality-assurance/release-receipt.json>",
                ],
            )


if __name__ == "__main__":
    unittest.main()
