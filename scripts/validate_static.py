#!/usr/bin/env python3
"""Deterministic Minco repository validation without a Rust compiler.

This gate verifies repository shape, contracts, generated metadata, architecture boundaries,
plugin/task graphs, deployment policy, scripts and lexical Rust structure. It complements—but
never replaces—`cargo fmt`, Clippy, compilation and executable tests.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head"}
MUTATING_METHODS = {"post", "put", "patch", "delete"}
IGNORED_PARTS = {"target", ".git", ".jj", ".venv", "__pycache__", "node_modules"}
IGNORED_RELATIVE_PREFIXES = {
    Path("docs-site/.vitepress/cache"),
    Path("docs-site/.vitepress/dist"),
}


def ignored_path(root: Path, path: Path) -> bool:
    """Return whether a generated or dependency path is outside source validation."""
    relative = path.relative_to(root)
    return any(part in IGNORED_PARTS for part in relative.parts) or any(
        relative.is_relative_to(prefix) for prefix in IGNORED_RELATIVE_PREFIXES
    )


def report_root(root: Path, default_root: Path = ROOT) -> str:
    """Keep repository-root evidence stable across checkout locations."""
    return "." if root == default_root else str(root)


class CloudFormationLoader(yaml.SafeLoader):
    """Safe YAML loader that preserves CloudFormation intrinsic tags as data."""


def _cloudformation_tag(loader: CloudFormationLoader, tag_suffix: str, node: yaml.Node) -> Any:
    if isinstance(node, yaml.ScalarNode):
        value = loader.construct_scalar(node)
    elif isinstance(node, yaml.SequenceNode):
        value = loader.construct_sequence(node)
    else:
        value = loader.construct_mapping(node)
    return {f"Fn::{tag_suffix}": value}


CloudFormationLoader.add_multi_constructor("!", _cloudformation_tag)


@dataclass
class Finding:
    code: str
    severity: str
    message: str
    path: str | None = None


class Validator:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.findings: list[Finding] = []
        self.metrics: dict[str, Any] = {}
        self.contract_operations: list[dict[str, Any]] = []

    def error(self, code: str, message: str, path: Path | None = None) -> None:
        self.findings.append(Finding(code, "error", message, self.relative(path)))

    def warning(self, code: str, message: str, path: Path | None = None) -> None:
        self.findings.append(Finding(code, "warning", message, self.relative(path)))

    def relative(self, path: Path | None) -> str | None:
        if path is None:
            return None
        try:
            return str(path.relative_to(self.root))
        except ValueError:
            return str(path)

    def run(self) -> dict[str, Any]:
        self.validate_required_files()
        self.validate_data_files()
        self.validate_workspace()
        self.validate_repository_truth()
        self.validate_contract()
        self.validate_architecture()
        self.validate_plugins()
        self.validate_roadmap_tasks()
        self.validate_quality_configuration()
        self.validate_rust_lexically()
        self.validate_python()
        self.validate_shell()
        self.validate_deployment_artifacts()
        self.validate_no_placeholders()
        errors = sum(f.severity == "error" for f in self.findings)
        warnings = sum(f.severity == "warning" for f in self.findings)
        return {
            "schema_version": 1,
            "status": "ok" if errors == 0 else "failed",
            "root": report_root(self.root),
            "errors": errors,
            "warnings": warnings,
            "metrics": self.metrics,
            "findings": [asdict(f) for f in self.findings],
            "limitations": [
                "Static validation does not compile Rust or resolve crate APIs.",
                "Run cargo fmt, cargo clippy, cargo test, Cargo Lambda and SAM validation on the pinned toolchain before release.",
            ],
        }

    def validate_required_files(self) -> None:
        required = [
            "Cargo.toml", "rust-toolchain.toml", "minco.toml", "quality.toml",
            "pyproject.toml", "uv.lock",
            "README.md", "AGENTS.md", "CODEX_HANDOFF.md", "VERIFICATION.md",
            "PUBLISHING.md",
            "SECURITY.md", "CONTRIBUTING.md", "LICENSE-MIT", "LICENSE-APACHE",
            "CHANGELOG.md", ".env.example", "plugins/catalog.toml",
            "docs/development/publishing.md",
            "docs/development/using-minco-crate.md", "scripts/validate_publish.py",
            "scripts/release/publish.py", "scripts/release/publish.sh",
            "scripts/release/package-list.sh", ".github/workflows/publish-crates.yml",
            "config/jj/repo.toml", "examples/orders/openapi/openapi.yaml",
            "examples/orders/config/minco.dev.toml", "roadmap/roadmap.yaml",
            "infra/aws/generated/plan.json", "infra/aws/generated/template.yaml",
            "verification/adoption-measurements.json",
            "verification/adoption-baseline.json",
            "verification/source-manifest.json",
        ]
        for item in required:
            path = self.root / item
            if not path.is_file():
                self.error("STATIC-001", f"required file is missing: {item}", path)

    def validate_data_files(self) -> None:
        counts = {"toml": 0, "yaml": 0, "json": 0}
        for path in sorted(self.root.rglob("*")):
            if not path.is_file() or ignored_path(self.root, path):
                continue
            try:
                if path.suffix == ".toml":
                    tomllib.loads(path.read_text())
                    counts["toml"] += 1
                elif path.suffix in {".yaml", ".yml"}:
                    yaml.load(path.read_text(), Loader=CloudFormationLoader)
                    counts["yaml"] += 1
                elif path.suffix == ".json":
                    json.loads(path.read_text())
                    counts["json"] += 1
            except Exception as exc:
                self.error("STATIC-DATA-001", f"invalid {path.suffix} document: {exc}", path)
        self.metrics.update({f"{key}_files": value for key, value in counts.items()})

    def validate_workspace(self) -> None:
        cargo_path = self.root / "Cargo.toml"
        if not cargo_path.is_file():
            return
        cargo = tomllib.loads(cargo_path.read_text())
        workspace = cargo.get("workspace", {})
        members = workspace.get("members", [])
        default_members = workspace.get("default-members", [])
        self.metrics["workspace_members"] = len(members)
        if len(members) != len(set(members)):
            self.error("STATIC-CARGO-001", "workspace contains duplicate member paths", cargo_path)
        for default in default_members:
            if default not in members:
                self.error("STATIC-CARGO-002", f"default member {default} is not a workspace member", cargo_path)
        names: dict[str, Path] = {}
        for member in members:
            member_path = self.root / member
            manifest = member_path / "Cargo.toml"
            if not manifest.is_file():
                self.error("STATIC-CARGO-003", f"workspace member has no Cargo.toml: {member}", manifest)
                continue
            data = tomllib.loads(manifest.read_text())
            package = data.get("package", {})
            name = package.get("name")
            if not name:
                self.error("STATIC-CARGO-004", f"workspace member has no package.name: {member}", manifest)
            elif name in names:
                self.error("STATIC-CARGO-005", f"duplicate package name {name}", manifest)
            else:
                names[name] = manifest
            has_target = any((member_path / candidate).is_file() for candidate in ["src/lib.rs", "src/main.rs"])
            has_target = has_target or bool(data.get("bin")) or bool(data.get("lib"))
            if not has_target:
                self.error("STATIC-CARGO-006", f"workspace member has no Rust target: {member}", member_path)
        toolchain_path = self.root / "rust-toolchain.toml"
        if toolchain_path.is_file():
            toolchain = tomllib.loads(toolchain_path.read_text()).get("toolchain", {})
            if toolchain.get("channel") != "1.97.1":
                self.error("STATIC-CARGO-007", "Rust must be pinned to 1.97.1", toolchain_path)
            for component in ["clippy", "rustfmt"]:
                if component not in toolchain.get("components", []):
                    self.error("STATIC-CARGO-008", f"toolchain lacks required component {component}", toolchain_path)

    def validate_repository_truth(self) -> None:
        truth_path = self.root / "verification/repository-truth.toml"
        measurements_path = self.root / "verification/adoption-measurements.json"
        cargo_path = self.root / "Cargo.toml"
        facade_path = self.root / "crates/minco/Cargo.toml"
        catalog_path = self.root / "plugins/catalog.toml"
        roadmap_path = self.root / "roadmap/roadmap.yaml"
        task_root = self.root / "tasks"
        required = [truth_path, cargo_path, facade_path, catalog_path, roadmap_path]
        if not all(path.is_file() for path in required) or not task_root.is_dir():
            return

        truth = tomllib.loads(truth_path.read_text())
        cargo = tomllib.loads(cargo_path.read_text())
        facade = tomllib.loads(facade_path.read_text())
        catalog = tomllib.loads(catalog_path.read_text())
        workspace_version = cargo["workspace"]["package"]["version"]
        if truth.get("workspace_version") != workspace_version:
            self.error(
                "STATIC-TRUTH-VERSION-001",
                "repository truth version differs from workspace.package.version",
                truth_path,
            )
        if not re.fullmatch(r"\d+\.\d+\.\d+", str(truth.get("published_baseline", ""))):
            self.error(
                "STATIC-TRUTH-PUBLISHED-001",
                "published_baseline must be an exact semantic version",
                truth_path,
            )
        published_baseline = str(truth.get("published_baseline", ""))
        expected_release_state = (
            "published" if published_baseline == workspace_version else "candidate"
        )
        if truth.get("workspace_release_state") != expected_release_state:
            self.error(
                "STATIC-TRUTH-RELEASE-001",
                f"workspace_release_state must be {expected_release_state!r} when "
                f"workspace version is {workspace_version} and published baseline is "
                f"{published_baseline}",
                truth_path,
            )
        if expected_release_state == "candidate":
            version_parts = tuple(int(part) for part in workspace_version.split("."))
            baseline_parts = tuple(int(part) for part in published_baseline.split("."))
            if version_parts <= baseline_parts:
                self.error(
                    "STATIC-TRUTH-RELEASE-004",
                    "candidate workspace version must be newer than the published baseline",
                    truth_path,
                )
            guide_path = (
                self.root
                / "docs/adoption"
                / f"{published_baseline}-to-{workspace_version}.md"
            )
            guide_source = guide_path.read_text() if guide_path.is_file() else ""
            guide_markers = [
                f"Published baseline: `{published_baseline}`",
                f"Candidate workspace version: `{workspace_version}`",
                "Candidate publication status: `unpublished`",
            ]
            if any(marker not in guide_source for marker in guide_markers):
                self.error(
                    "STATIC-TRUTH-RELEASE-002",
                    "unpublished candidate requires a versioned upgrade guide with "
                    "baseline, workspace-version and publication-state markers",
                    guide_path,
                )
            changelog_path = self.root / "CHANGELOG.md"
            changelog_source = (
                changelog_path.read_text() if changelog_path.is_file() else ""
            )
            release_notes = re.search(
                rf"^## \[{re.escape(workspace_version)}\] - \d{{4}}-\d{{2}}-\d{{2}}\n"
                r"(?P<body>.*?)(?=^## \[|\Z)",
                changelog_source,
                flags=re.MULTILINE | re.DOTALL,
            )
            notes_body = release_notes.group("body").strip() if release_notes else ""
            if (
                not notes_body
                or "No changes yet." in notes_body
                or not re.search(r"(?m)^- \S", notes_body)
            ):
                self.error(
                    "STATIC-TRUTH-RELEASE-003",
                    "unpublished candidate requires dated, substantive changelog notes",
                    changelog_path,
                )
        worker_source_path = self.root / "extensions/minco-aws-worker/src/lib.rs"
        worker_source = worker_source_path.read_text() if worker_source_path.is_file() else ""
        worker_manifest_path = self.root / "extensions/minco-aws-worker/Cargo.toml"
        if worker_manifest_path.is_file():
            worker_dependencies = tomllib.loads(worker_manifest_path.read_text()).get(
                "dependencies", {}
            )
            worker_sdks = sorted(
                dependency
                for dependency in worker_dependencies
                if dependency.startswith("aws-sdk-") or dependency == "aws-config"
            )
            if worker_sdks:
                self.error(
                    "STATIC-BUDGET-003",
                    f"minco-aws-worker must not depend on AWS SDK clients: {worker_sdks}",
                    worker_manifest_path,
                )
        budgets = truth.get("budgets", {})
        artifact_budget = budgets.get("native_lambda_zip_max_bytes")
        if not isinstance(artifact_budget, int) or artifact_budget <= 0:
            self.error(
                "STATIC-BUDGET-007",
                "native_lambda_zip_max_bytes must be a positive integer",
                truth_path,
            )
        if measurements_path.is_file():
            measurements = json.loads(measurements_path.read_text())
            baseline_snapshot_path = self.root / "verification/adoption-baseline.json"
            if (
                baseline_snapshot_path.is_file()
                and measurements.get("baseline")
                != json.loads(baseline_snapshot_path.read_text())
            ):
                self.error(
                    "STATIC-MEASURE-001",
                    "adoption report baseline differs from the immutable baseline snapshot",
                    measurements_path,
                )
            candidate_revision = measurements.get("candidate", {}).get("revision")
            if not isinstance(candidate_revision, str) or not re.fullmatch(
                r"source-tree-sha256:[0-9a-f]{64}",
                candidate_revision,
            ):
                self.error(
                    "STATIC-MEASURE-002",
                    "adoption report candidate requires an immutable source-tree-sha256 revision",
                    measurements_path,
                )
            qualified_candidate_sha256 = truth.get(
                "qualified_candidate_source_tree_sha256"
            )
            expected_revision = f"source-tree-sha256:{qualified_candidate_sha256}"
            if (
                not isinstance(qualified_candidate_sha256, str)
                or not re.fullmatch(r"[0-9a-f]{64}", qualified_candidate_sha256)
                or candidate_revision != expected_revision
            ):
                self.error(
                    "STATIC-MEASURE-004",
                    "adoption report candidate revision differs from the immutable qualified candidate in repository truth",
                    measurements_path,
                )
            baseline_facade = measurements.get("baseline", {}).get("facade", {})
            candidate_facade = measurements.get("candidate", {}).get("facade", {})
            official_features = set(
                facade.get("features", {}).get("official-plugins", [])
            )
            catalog_feature_by_crate = {
                entry.get("crate"): entry.get("feature")
                for entry in catalog.get("plugin", [])
            }
            new_official_plugin_packages = {
                package
                for package in truth.get("new_publishable_packages", [])
                if isinstance(package, str)
                and catalog_feature_by_crate.get(package) in official_features
            }
            for profile in ["no_default_features", "default_features", "official_plugins"]:
                baseline_packages = baseline_facade.get(profile, {}).get(
                    "normal_dependency_packages"
                )
                candidate_packages = candidate_facade.get(profile, {}).get(
                    "normal_dependency_packages"
                )
                allowed_growth = (
                    len(new_official_plugin_packages)
                    if profile == "official_plugins"
                    else 0
                )
                expected_packages = (
                    baseline_packages + allowed_growth
                    if isinstance(baseline_packages, int)
                    else baseline_packages
                )
                if expected_packages != candidate_packages:
                    self.error(
                        "STATIC-BUDGET-004",
                        f"{profile} dependency package count expected {expected_packages}; found {candidate_packages}",
                        measurements_path,
                    )
            for label, artifact in (
                measurements.get("candidate", {})
                .get("native_arm64_artifacts", {})
                .items()
            ):
                compressed = artifact.get("compressed_bytes")
                uncompressed = artifact.get("uncompressed_bytes")
                artifact_sha256 = artifact.get("sha256")
                if not isinstance(compressed, int) or compressed <= 0:
                    self.error(
                        "STATIC-BUDGET-005",
                        f"{label} has no positive compressed-byte measurement",
                        measurements_path,
                    )
                elif isinstance(artifact_budget, int) and compressed > artifact_budget:
                    self.error(
                        "STATIC-BUDGET-006",
                        f"{label} ZIP is {compressed} bytes; budget is {artifact_budget}",
                        measurements_path,
                    )
                if not isinstance(uncompressed, int) or uncompressed <= 0:
                    self.error(
                        "STATIC-MEASURE-003",
                        f"{label} has no positive uncompressed-byte measurement",
                        measurements_path,
                    )
                if not isinstance(artifact_sha256, str) or not re.fullmatch(
                    r"[0-9a-f]{64}",
                    artifact_sha256,
                ):
                    self.error(
                        "STATIC-MEASURE-005",
                        f"{label} requires an exact artifact SHA-256",
                        measurements_path,
                    )
        constant_budgets = {
            "MAX_BATCH_SIZE": budgets.get("worker_max_batch_size"),
            "MAX_MESSAGE_BYTES": budgets.get("worker_max_message_bytes"),
        }
        for constant, expected in constant_budgets.items():
            match = re.search(
                rf"const {constant}: usize = ([0-9_ *]+);",
                worker_source,
            )
            actual = parse_usize_product(match.group(1)) if match is not None else None
            if actual != expected:
                self.error(
                    "STATIC-BUDGET-002",
                    f"{constant} is {actual}; repository budget requires {expected}",
                    worker_source_path,
                )

        members = cargo["workspace"].get("members", [])
        package_by_name: dict[str, dict[str, Any]] = {}
        path_by_name: dict[str, str] = {}
        publishable: list[str] = []
        for member in members:
            manifest_path = self.root / member / "Cargo.toml"
            if not manifest_path.is_file():
                continue
            manifest = tomllib.loads(manifest_path.read_text())
            package = manifest.get("package", {})
            name = package.get("name")
            if not isinstance(name, str):
                continue
            package_by_name[name] = manifest
            path_by_name[name] = member
            if package.get("publish") is not False:
                publishable.append(name)
        release_packages = cargo["workspace"]["metadata"]["minco"]["release"]["publish"]
        package_tests = cargo["workspace"]["metadata"]["minco"]["release"][
            "package_tests"
        ]
        expected_package_count = truth.get("publishable_package_count")
        if expected_package_count != len(publishable):
            self.error(
                "STATIC-TRUTH-PACKAGES-001",
                f"repository truth expects {expected_package_count} publishable packages; found {len(publishable)}",
                truth_path,
            )
        if set(release_packages) != set(publishable) or len(release_packages) != len(publishable):
            self.error(
                "STATIC-TRUTH-PACKAGES-002",
                "release package inventory differs from publishable workspace packages",
                cargo_path,
            )
        published_package_count = truth.get("published_package_count")
        if (
            not isinstance(published_package_count, int)
            or published_package_count <= 0
            or published_package_count > len(publishable)
        ):
            self.error(
                "STATIC-TRUTH-PUBLISHED-002",
                "published_package_count must be positive and no larger than the publishable family",
                truth_path,
            )
        new_publishable_packages = truth.get("new_publishable_packages", [])
        if (
            not isinstance(new_publishable_packages, list)
            or any(
                not isinstance(package, str) or package not in publishable
                for package in new_publishable_packages
            )
        ):
            self.error(
                "STATIC-TRUTH-PACKAGES-003",
                "new_publishable_packages must name publishable workspace packages",
                truth_path,
            )
        elif not set(new_publishable_packages).issubset(package_tests):
            missing = sorted(set(new_publishable_packages) - set(package_tests))
            self.error(
                "STATIC-TRUTH-PACKAGES-004",
                f"new publishable packages lack archive tests: {missing}",
                cargo_path,
            )
        if truth.get("published_baseline") == workspace_version:
            if published_package_count != len(publishable):
                self.error(
                    "STATIC-TRUTH-PUBLISHED-002",
                    "the current published baseline must contain the full publishable family",
                    truth_path,
                )
            if new_publishable_packages:
                self.error(
                    "STATIC-TRUTH-PUBLISHED-003",
                    "the current published baseline cannot retain first-publication candidates",
                    truth_path,
                )

        features = facade.get("features", {})
        dependencies = facade.get("dependencies", {})
        catalog_entries = catalog.get("plugin", [])
        catalog_by_id = {entry.get("id"): entry for entry in catalog_entries}
        ids = [entry.get("id") for entry in catalog_entries]
        crates = [entry.get("crate") for entry in catalog_entries]
        catalog_features = [entry.get("feature") for entry in catalog_entries]
        if len(ids) != len(set(ids)):
            self.error("STATIC-TRUTH-CATALOG-003", "catalog contains duplicate IDs", catalog_path)
        if len(crates) != len(set(crates)):
            self.error("STATIC-TRUTH-CATALOG-004", "catalog contains duplicate crates", catalog_path)
        if len(catalog_features) != len(set(catalog_features)):
            self.error("STATIC-TRUTH-CATALOG-005", "catalog contains duplicate facade features", catalog_path)
        for entry in catalog_entries:
            plugin_id = entry.get("id")
            package = entry.get("crate")
            path = entry.get("path")
            kind = entry.get("kind")
            feature = entry.get("feature")
            if kind not in {"plugin", "adapter", "runtime"}:
                self.error(
                    "STATIC-TRUTH-CATALOG-001",
                    f"catalog entry {plugin_id} has invalid kind {kind!r}",
                    catalog_path,
                )
            if package not in package_by_name or path_by_name.get(package) != path:
                self.error(
                    "STATIC-TRUTH-CATALOG-002",
                    f"catalog entry {plugin_id} path/package differs from the workspace",
                    catalog_path,
                )
            if feature not in features:
                self.error(
                    "STATIC-TRUTH-FACADE-001",
                    f"catalog entry {plugin_id} references missing facade feature {feature}",
                    facade_path,
                )
            elif package not in dependencies or f"dep:{package}" not in feature_closure_tokens(features, [feature]):
                self.error(
                    "STATIC-TRUTH-FACADE-002",
                    f"facade feature {feature} does not select {package}",
                    facade_path,
                )
            if kind == "plugin" and isinstance(path, str):
                source = "\n".join(
                    source_path.read_text()
                    for source_path in sorted((self.root / path / "src").glob("*.rs"))
                )
                expected_stability = str(entry.get("stability", "")).title()
                if f'PluginId::new("{plugin_id}")' not in source:
                    self.error(
                        "STATIC-TRUTH-DESCRIPTOR-001",
                        f"catalog plugin {plugin_id} has no matching runtime descriptor ID",
                        self.root / path,
                    )
                if f"PluginStability::{expected_stability}" not in source:
                    self.error(
                        "STATIC-TRUTH-DESCRIPTOR-002",
                        f"catalog plugin {plugin_id} stability differs from its runtime descriptor",
                        self.root / path,
                    )
                runtime_default = "descriptor.default_enabled = true" in source
                if bool(entry.get("default_enabled")) != runtime_default:
                    self.error(
                        "STATIC-TRUTH-DESCRIPTOR-003",
                        f"catalog plugin {plugin_id} default selection differs from its runtime descriptor",
                        self.root / path,
                    )

        facade_extension_crates = {
            dependency
            for dependency, specification in dependencies.items()
            if isinstance(specification, dict)
            and specification.get("optional") is True
            and path_by_name.get(dependency, "").startswith(("plugins/", "extensions/"))
        }
        missing_catalog_crates = sorted(facade_extension_crates - set(crates))
        if missing_catalog_crates:
            self.error(
                "STATIC-TRUTH-CATALOG-006",
                f"facade plugin/extension dependencies absent from catalog: {missing_catalog_crates}",
                catalog_path,
            )

        default_plugin_features = {
            entry["feature"]
            for entry in catalog_entries
            if entry.get("kind") == "plugin" and entry.get("default_enabled") is True
        }
        selected_by_default = {
            token
            for token in feature_closure_tokens(features, features.get("default", []))
            if token.startswith("plugin-")
        }
        if selected_by_default != default_plugin_features:
            self.error(
                "STATIC-TRUTH-FACADE-003",
                "facade default plugin features differ from catalog defaults",
                facade_path,
            )
        official_plugin_features = {
            entry["feature"] for entry in catalog_entries if entry.get("kind") == "plugin"
        }
        selected_official = {
            token
            for token in feature_closure_tokens(features, ["official-plugins"])
            if token.startswith("plugin-")
        }
        if selected_official != official_plugin_features:
            self.error(
                "STATIC-TRUTH-FACADE-004",
                "official-plugins feature differs from the official plugin catalog",
                facade_path,
            )
        default_tokens = feature_closure_tokens(features, features.get("default", []))
        forbidden_default = sorted(
            token
            for token in default_tokens
            if any(fragment in token for fragment in ["aws-", "sqlx", "lambda"])
        )
        if forbidden_default:
            self.error(
                "STATIC-BUDGET-001",
                f"default facade enables provider/runtime features: {forbidden_default}",
                facade_path,
            )

        tasks_by_milestone: dict[str, list[str]] = {}
        task_status: dict[str, str] = {}
        for task_path in sorted(task_root.rglob("*.md")):
            source = task_path.read_text()
            if not source.startswith("---\n") or "\n---\n" not in source[4:]:
                continue
            task = yaml.safe_load(source[4:].split("\n---\n", 1)[0])
            tasks_by_milestone.setdefault(task.get("milestone"), []).append(task.get("status"))
            task_status[task.get("id")] = task.get("status")
        roadmap = yaml.safe_load(roadmap_path.read_text())
        milestones = {
            milestone["id"]: milestone for milestone in roadmap.get("milestones", [])
        }
        for milestone in milestones.values():
            statuses = tasks_by_milestone.get(milestone["id"], [])
            status = milestone.get("status")
            incomplete_tasks = [task_status for task_status in statuses if task_status != "complete"]
            incomplete_dependencies = [
                dependency
                for dependency in milestone.get("depends_on", [])
                if milestones.get(dependency, {}).get("status") != "complete"
            ]
            if status == "complete" and (incomplete_tasks or incomplete_dependencies):
                self.error(
                    "STATIC-TRUTH-ROADMAP-001",
                    f"complete milestone {milestone['id']} has incomplete tasks or prerequisites",
                    roadmap_path,
                )
            if status == "planned" and any(
                task_status in {"ready", "active", "complete"} for task_status in statuses
            ):
                self.error(
                    "STATIC-TRUTH-ROADMAP-002",
                    f"planned milestone {milestone['id']} has ready, active, or completed task evidence",
                    roadmap_path,
                )
            if (
                status == "active"
                and statuses
                and not incomplete_tasks
                and not incomplete_dependencies
            ):
                self.error(
                    "STATIC-TRUTH-ROADMAP-003",
                    f"active milestone {milestone['id']} has complete tasks and prerequisites",
                    roadmap_path,
                )
        gate_task = truth.get("adoption_gate_task")
        if gate_task not in task_status:
            self.error(
                "STATIC-TRUTH-ADOPTION-001",
                f"adoption gate task {gate_task!r} does not exist",
                truth_path,
            )

        plan_path = self.root / "infra/aws/generated/plan.json"
        if plan_path.is_file():
            plan = json.loads(plan_path.read_text())
            for descriptor in plan.get("application_graph", {}).get("plugins", []):
                entry = catalog_by_id.get(descriptor.get("id"))
                if entry is None:
                    self.error(
                        "STATIC-TRUTH-PLAN-001",
                        f"generated plan plugin {descriptor.get('id')} is absent from the catalog",
                        plan_path,
                    )
                elif descriptor.get("stability") != entry.get("stability"):
                    self.error(
                        "STATIC-TRUTH-PLAN-002",
                        f"generated plan plugin {descriptor.get('id')} stability differs from the catalog",
                        plan_path,
                    )

        markers = {
            self.root / "README.md": [
                f"Published baseline: `{truth['published_baseline']}`",
                f"Current workspace version: `{workspace_version}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
                f"Current publishable package count: `{expected_package_count}`",
            ],
            self.root / "VERIFICATION.md": [
                f"Current workspace version: `{workspace_version}`",
                f"Published baseline: `{truth['published_baseline']}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
            ],
            self.root / "CODEX_HANDOFF.md": [
                f"Published baseline: `{truth['published_baseline']}`",
                f"Current workspace version: `{workspace_version}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
            ],
            self.root / "docs/adoption/incremental-adoption.md": [
                f"Published baseline: `{truth['published_baseline']}`",
                f"Current workspace version: `{workspace_version}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
            ],
            self.root / "docs/development/publishing.md": [
                f"published `{truth['published_baseline']}` release",
            ],
            self.root / "docs/development/using-minco-crate.md": [
                f"Published baseline: `{truth['published_baseline']}`",
                f"Current workspace version: `{workspace_version}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
            ],
            self.root / "docs/vision/minco-framework-definition.md": [
                f"Published baseline: `{truth['published_baseline']}`",
                f"Current workspace version: `{workspace_version}`",
                f"Workspace release state: `{truth['workspace_release_state']}`",
            ],
            self.root / "docs/reference/cli.md": [
                "cargo minco deploy verify",
                "cargo minco promote",
            ],
        }
        for document, expected_markers in markers.items():
            source = document.read_text() if document.is_file() else ""
            for marker in expected_markers:
                if marker not in source:
                    self.error(
                        "STATIC-TRUTH-DOCS-001",
                        f"current truth document lacks marker: {marker}",
                        document,
                    )

    def validate_contract(self) -> None:
        manifest_path = self.root / "minco.toml"
        if not manifest_path.is_file():
            return
        manifest = tomllib.loads(manifest_path.read_text())
        contract_path = self.root / manifest["contract"]
        generated_path = self.root / manifest["generated"]
        if not contract_path.is_file():
            return
        raw = yaml.safe_load(contract_path.read_text())
        canonical = json.dumps(raw, sort_keys=True, separators=(",", ":")).encode()
        digest = hashlib.sha256(canonical).hexdigest()
        self.metrics["contract_sha256"] = digest
        if not str(raw.get("openapi", "")).startswith("3.1."):
            self.error("STATIC-CONTRACT-001", "contract must use OpenAPI 3.1.x", contract_path)
        seen: set[str] = set()
        operations: list[dict[str, Any]] = []
        for route, item in (raw.get("paths") or {}).items():
            if not isinstance(item, dict):
                self.error("STATIC-CONTRACT-002", f"path item {route} must be an object", contract_path)
                continue
            for method, operation in item.items():
                if method not in HTTP_METHODS:
                    continue
                operation = operation or {}
                operation_id = operation.get("operationId")
                if not operation_id or not re.fullmatch(r"[a-z][A-Za-z0-9]*", str(operation_id)):
                    self.error("STATIC-CONTRACT-003", f"invalid operationId for {method.upper()} {route}", contract_path)
                    continue
                if operation_id in seen:
                    self.error("STATIC-CONTRACT-004", f"duplicate operationId {operation_id}", contract_path)
                seen.add(operation_id)
                responses = operation.get("responses") or {}
                if not any(str(status).startswith("2") for status in responses):
                    self.error("STATIC-CONTRACT-005", f"{operation_id} has no 2xx response", contract_path)
                if not any(str(status) == "default" or str(status).startswith(("4", "5")) for status in responses):
                    self.error("STATIC-CONTRACT-006", f"{operation_id} has no error response", contract_path)
                for status, response in responses.items():
                    if (
                        str(status) == "default"
                        or str(status).startswith(("4", "5"))
                    ) and not response_has_problem_media(raw, response):
                        self.error(
                            "STATIC-CONTRACT-017",
                            f"{operation_id} error response {status} must use application/problem+json",
                            contract_path,
                        )
                idempotency_value = operation.get("x-minco-idempotent")
                if idempotency_value is not None and not isinstance(idempotency_value, bool):
                    self.error(
                        "STATIC-CONTRACT-014",
                        f"{operation_id} x-minco-idempotent must be boolean",
                        contract_path,
                    )
                idempotent = idempotency_value is True
                parameters, parameter_error = effective_parameters(
                    raw,
                    item.get("parameters"),
                    operation.get("parameters"),
                )
                if parameter_error:
                    self.error(
                        "STATIC-CONTRACT-021",
                        f"{operation_id} has a non-local or unresolved parameter reference",
                        contract_path,
                    )
                has_idempotency_header = any(
                    isinstance(parameter, dict)
                    and parameter.get("in") == "header"
                    and str(parameter.get("name", "")).lower() == "idempotency-key"
                    and parameter.get("required") is True
                    for parameter in parameters
                )
                if idempotent:
                    if not has_idempotency_header:
                        self.error("STATIC-CONTRACT-007", f"{operation_id} lacks required Idempotency-Key", contract_path)
                elif method in MUTATING_METHODS and has_idempotency_header:
                    self.error(
                        "STATIC-CONTRACT-015",
                        f"{operation_id} Idempotency-Key requires x-minco-idempotent: true",
                        contract_path,
                    )
                security = operation.get("security", raw.get("security"))
                public, valid_security = security_allows_anonymous(security)
                if not valid_security:
                    self.error(
                        "STATIC-CONTRACT-020",
                        f"{operation_id} effective OpenAPI security must be an array of objects whose scheme values are string arrays",
                        contract_path,
                    )
                auth = operation.get("x-minco-auth")
                if not valid_auth_policy(auth, public):
                    self.error(
                        "STATIC-CONTRACT-016",
                        f"{operation_id} x-minco-auth contradicts effective OpenAPI security or has an invalid permission policy",
                        contract_path,
                    )
                operations.append({
                    "operation_id": operation_id,
                    "method": method,
                    "path": route,
                    "authenticated": not public,
                    "idempotent": idempotent,
                })
        schemas = ((raw.get("components") or {}).get("schemas") or {})
        for location, schema in walk_openapi_schema_objects(raw):
            additional = schema.get("additionalProperties")
            open_policy = schema.get("x-minco-open-object")
            if additional is False:
                if open_policy is not None:
                    self.error(
                        "STATIC-CONTRACT-019",
                        f"closed object {location} declares x-minco-open-object",
                        contract_path,
                    )
                continue
            rationale = (
                open_policy.get("rationale")
                if isinstance(open_policy, dict)
                else None
            )
            explicit_open = additional is True or isinstance(additional, dict)
            if not explicit_open or not isinstance(rationale, str) or not rationale.strip():
                self.error(
                    "STATIC-CONTRACT-008",
                    f"object {location} must be closed or declare explicit additionalProperties and x-minco-open-object.rationale",
                    contract_path,
                )
        self.contract_operations = sorted(operations, key=lambda item: item["operation_id"])
        self.metrics["contract_operations"] = len(operations)
        self.metrics["contract_schemas"] = len(schemas)
        if not generated_path.is_file():
            self.error("STATIC-CONTRACT-009", "generated Rust contract is missing", generated_path)
            return
        generated = generated_path.read_text()
        match = re.search(r"Contract SHA-256: ([0-9a-f]{64})", generated)
        if not match or match.group(1) != digest:
            self.error("STATIC-CONTRACT-010", "generated contract digest is stale", generated_path)
        generated_operations = set(re.findall(r"ContractOperation::new\(\n\s*\"([^\"]+)\"", generated))
        expected = {item["operation_id"] for item in operations}
        if generated_operations != expected:
            self.error("STATIC-CONTRACT-011", f"generated operations differ: expected {sorted(expected)}, got {sorted(generated_operations)}", generated_path)
        api_path = self.root / "examples/orders/api/src/lib.rs"
        if api_path.is_file():
            source = api_path.read_text()
            bound_block = re.search(r"BOUND_OPERATIONS:.*?=\s*&\[(.*?)\];", source, re.DOTALL)
            if not bound_block:
                self.error("STATIC-CONTRACT-012", "API has no BOUND_OPERATIONS inventory", api_path)
            else:
                bound = set(re.findall(r"generated::([A-Z][A-Z0-9_]*)", bound_block.group(1)))
                expected_constants = {screaming_snake(item["operation_id"]) for item in operations}
                if bound != expected_constants:
                    self.error("STATIC-CONTRACT-013", f"bound operations differ: expected {sorted(expected_constants)}, got {sorted(bound)}", api_path)

    def validate_architecture(self) -> None:
        manifest_path = self.root / "minco.toml"
        if not manifest_path.is_file():
            return
        manifest = tomllib.loads(manifest_path.read_text())
        architecture = manifest.get("architecture", {})
        rules = {
            "domain_roots": ("domain", {"axum", "sqlx", "lambda_http", "lambda_runtime", "minco-http", "minco-core"}),
            "application_roots": ("application", {"axum", "sqlx", "lambda_http", "lambda_runtime", "minco-http"}),
        }
        for key, (layer, forbidden) in rules.items():
            for configured in architecture.get(key, []):
                root = self.root / configured
                for cargo in root.rglob("Cargo.toml"):
                    dependencies = tomllib.loads(cargo.read_text()).get("dependencies", {})
                    for dependency in dependencies:
                        if dependency in forbidden or dependency.startswith("aws-sdk-"):
                            self.error("STATIC-ARCH-001", f"{layer} depends on forbidden crate {dependency}", cargo)
        for configured in architecture.get("api_roots", []):
            root = self.root / configured
            for rust in root.rglob("*.rs"):
                source = rust.read_text()
                if "sqlx::" in source or "use sqlx" in source:
                    self.error("STATIC-ARCH-002", "API delivery contains SQLx usage", rust)
        core_manifest = self.root / "crates/minco-core/Cargo.toml"
        if core_manifest.is_file():
            deps = tomllib.loads(core_manifest.read_text()).get("dependencies", {})
            for forbidden in ["axum", "sqlx", "lambda_http", "lambda_runtime"]:
                if forbidden in deps:
                    self.error("STATIC-ARCH-003", f"minco-core depends on provider/runtime crate {forbidden}", core_manifest)
            for dependency in deps:
                if dependency.startswith("aws-sdk-"):
                    self.error("STATIC-ARCH-004", f"minco-core depends on AWS SDK crate {dependency}", core_manifest)

    def validate_plugins(self) -> None:
        manifest_path = self.root / "minco.toml"
        catalog_path = self.root / "plugins/catalog.toml"
        if not manifest_path.is_file() or not catalog_path.is_file():
            return
        manifest = tomllib.loads(manifest_path.read_text())
        catalog = tomllib.loads(catalog_path.read_text())
        entries = catalog.get("plugin", [])
        ids: set[str] = set()
        catalog_ids: set[str] = set()
        for entry in entries:
            plugin_id = entry.get("id", "")
            catalog_ids.add(plugin_id)
            if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", plugin_id):
                self.error("STATIC-PLUGIN-001", f"invalid plugin ID {plugin_id!r}", catalog_path)
            if plugin_id in ids:
                self.error("STATIC-PLUGIN-002", f"duplicate plugin ID {plugin_id}", catalog_path)
            ids.add(plugin_id)
            package = entry.get("crate", "")
            candidates = [self.root / parent / package / "Cargo.toml" for parent in ["plugins", "extensions", "crates"]]
            if not any(candidate.is_file() for candidate in candidates):
                self.error("STATIC-PLUGIN-003", f"plugin {plugin_id} references missing package {package}", catalog_path)
        selected = manifest.get("plugins", {})
        enabled = set(selected.get("enabled", []))
        disabled = set(selected.get("disabled", []))
        overlap = enabled & disabled
        if overlap:
            self.error("STATIC-PLUGIN-004", f"plugins are both enabled and disabled: {sorted(overlap)}", manifest_path)
        for plugin_id in enabled | disabled:
            if plugin_id not in catalog_ids:
                self.error("STATIC-PLUGIN-005", f"selected plugin {plugin_id} is absent from the catalog", manifest_path)
        self.metrics["plugins"] = len(entries)

    def validate_roadmap_tasks(self) -> None:
        manifest_path = self.root / "minco.toml"
        if not manifest_path.is_file():
            return
        manifest = tomllib.loads(manifest_path.read_text())
        roadmap_path = self.root / manifest["roadmap"]
        task_root = self.root / manifest["tasks"]
        if not roadmap_path.is_file() or not task_root.is_dir():
            return
        roadmap = yaml.safe_load(roadmap_path.read_text())
        milestones = {item["id"]: item for item in roadmap.get("milestones", [])}
        if len(milestones) != len(roadmap.get("milestones", [])):
            self.error("STATIC-ROADMAP-001", "duplicate milestone IDs", roadmap_path)
        for milestone_id, milestone in milestones.items():
            for dependency in milestone.get("depends_on", []):
                if dependency not in milestones:
                    self.error("STATIC-ROADMAP-002", f"milestone {milestone_id} depends on unknown {dependency}", roadmap_path)
        tasks: dict[str, dict[str, Any]] = {}
        for path in sorted(task_root.rglob("*.md")):
            source = path.read_text()
            if not source.startswith("---\n") or "\n---\n" not in source[4:]:
                self.error("STATIC-TASK-001", "task lacks YAML front matter", path)
                continue
            front, body = source[4:].split("\n---\n", 1)
            try:
                task = yaml.safe_load(front)
            except Exception as exc:
                self.error("STATIC-TASK-002", f"invalid task front matter: {exc}", path)
                continue
            task_id = task.get("id")
            if not task_id:
                self.error("STATIC-TASK-003", "task has no ID", path)
                continue
            if task_id in tasks:
                self.error("STATIC-TASK-004", f"duplicate task ID {task_id}", path)
            task["_path"] = path
            tasks[task_id] = task
            if not body.strip():
                self.error("STATIC-TASK-005", "task body is empty", path)
            if task.get("milestone") not in milestones:
                self.error("STATIC-TASK-006", f"task references unknown milestone {task.get('milestone')}", path)
            for field in ["status", "priority", "area", "checks", "owned_paths"]:
                if field not in task:
                    self.error("STATIC-TASK-007", f"task lacks required field {field}", path)
            if task.get("status") == "complete" and not task.get("checks"):
                self.error("STATIC-TASK-008", "completed task retains no checks", path)
        for task_id, task in tasks.items():
            for dependency in task.get("depends_on", []):
                if dependency not in tasks:
                    self.error("STATIC-TASK-009", f"task {task_id} depends on unknown {dependency}", task["_path"])
        self._check_cycles({key: value.get("depends_on", []) for key, value in milestones.items()}, "milestone", roadmap_path)
        self._check_cycles({key: value.get("depends_on", []) for key, value in tasks.items()}, "task", task_root)
        self.metrics["milestones"] = len(milestones)
        self.metrics["tasks"] = len(tasks)

    def _check_cycles(self, graph: dict[str, list[str]], kind: str, path: Path) -> None:
        visiting: set[str] = set()
        visited: set[str] = set()

        def visit(node: str) -> None:
            if node in visiting:
                self.error("STATIC-GRAPH-001", f"{kind} dependency cycle includes {node}", path)
                return
            if node in visited:
                return
            visiting.add(node)
            for dependency in graph.get(node, []):
                visit(dependency)
            visiting.remove(node)
            visited.add(node)

        for node in graph:
            visit(node)

    def validate_quality_configuration(self) -> None:
        path = self.root / "quality.toml"
        if not path.is_file():
            return
        quality = tomllib.loads(path.read_text())
        gates = quality.get("gates", {})
        for required in ["static", "rust", "security", "e2e"]:
            commands = gates.get(required, {}).get("commands", [])
            if not commands:
                self.error("STATIC-QUALITY-001", f"quality gate {required} has no commands", path)
            if not all(isinstance(command, str) and command.strip() for command in commands):
                self.error("STATIC-QUALITY-002", f"quality gate {required} has an invalid command", path)
        for command in gates.get("static", {}).get("commands", []):
            if ".py" in command and not command.startswith("uv run --locked python "):
                self.error(
                    "STATIC-QUALITY-003",
                    f"Python quality command does not use the locked uv environment: {command}",
                    path,
                )
        workflow_root = self.root / ".github/workflows"
        for workflow in sorted(workflow_root.glob("*.y*ml")):
            for action, reference in re.findall(
                r"^\s*(?:-\s*)?uses:\s*([^@\s]+)@([^\s#]+)",
                workflow.read_text(),
                flags=re.MULTILINE,
            ):
                if not re.fullmatch(r"[0-9a-f]{40}", reference):
                    self.error(
                        "STATIC-QUALITY-004",
                        f"GitHub action is not pinned to an immutable commit: {action}@{reference}",
                        workflow,
                    )

    def validate_rust_lexically(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.rs")):
            if ignored_path(self.root, path):
                continue
            count += 1
            issue = balanced_rust(path.read_text())
            if issue:
                self.error("STATIC-RUST-001", issue, path)
        self.metrics["rust_files"] = count

    def validate_python(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.py")):
            if ignored_path(self.root, path):
                continue
            count += 1
            try:
                compile(path.read_text(), str(path), "exec")
            except SyntaxError as exc:
                self.error("STATIC-PYTHON-001", str(exc), path)
        self.metrics["python_files"] = count

    def validate_shell(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.sh")):
            if ignored_path(self.root, path):
                continue
            count += 1
            result = subprocess.run(["bash", "-n", str(path)], capture_output=True, text=True)
            if result.returncode:
                self.error("STATIC-SHELL-001", result.stderr.strip(), path)
            if path.read_text().startswith("#!/usr/bin/env bash") and "set -euo pipefail" not in path.read_text()[:200]:
                self.error("STATIC-SHELL-002", "bash script lacks set -euo pipefail", path)
        self.metrics["shell_files"] = count

    def validate_deployment_artifacts(self) -> None:
        manifest_path = self.root / "minco.toml"
        plan_path = self.root / "infra/aws/generated/plan.json"
        template_path = self.root / "infra/aws/generated/template.yaml"
        if not manifest_path.is_file() or not plan_path.is_file():
            return
        manifest = tomllib.loads(manifest_path.read_text())
        config_path = self.root / manifest["deployment_config"]
        config = tomllib.loads(config_path.read_text())
        plan_source = plan_path.read_text()
        plan = json.loads(plan_source)
        canonical_plan = json.dumps(plan, indent=2, sort_keys=True) + "\n"
        if plan_source != canonical_plan:
            self.error(
                "STATIC-PLAN-000",
                "generated plan must use canonical sorted JSON with a trailing newline",
                plan_path,
            )
        for key, value in config.items():
            if key != "routes" and plan.get(key) != value:
                self.error("STATIC-PLAN-001", f"generated plan differs from deployment config at {key}", plan_path)
        expected_routes = [
            {key: operation[key] for key in ["operation_id", "method", "path", "authenticated"]}
            for operation in self.contract_operations
        ]
        expected_routes.sort(key=lambda item: item["operation_id"])
        if plan.get("routes") != expected_routes:
            self.error("STATIC-PLAN-002", "generated plan routes differ from the OpenAPI operation inventory", plan_path)
        functions = plan.get("functions", [])
        if len(functions) != 1:
            self.error("STATIC-PLAN-003", "minimal plan must contain exactly one function", plan_path)
        for function in functions:
            if function.get("provisioned_concurrency", 0) != 0:
                self.error("STATIC-COST-001", "plan enables provisioned concurrency", plan_path)
            maximum = plan.get("cost_policy", {}).get("max_reserved_concurrency", 5)
            if function.get("reserved_concurrency", 0) > maximum:
                self.error("STATIC-COST-002", "plan exceeds reserved concurrency policy", plan_path)
        potential = sum(
            function.get("reserved_concurrency", 0) * function.get("database_connections_per_instance", 0)
            for function in functions
        )
        if potential > plan.get("cost_policy", {}).get("max_database_connections", 20):
            self.error("STATIC-COST-003", f"plan can open {potential} database connections", plan_path)
        if plan.get("uses_nat_gateway"):
            self.error("STATIC-COST-004", "minimal plan enables a NAT Gateway", plan_path)
        if plan.get("scheduled_wakeups"):
            self.error("STATIC-COST-005", "minimal plan contains scheduled wakeups", plan_path)
        origins = plan.get("allowed_origins", [])
        if not origins or "*" in origins:
            self.error("STATIC-HTTP-001", "plan requires non-wildcard exact origins", plan_path)
        headers = plan.get("allowed_headers", [])
        if (
            not headers
            or "*" in headers
            or len(headers) != len({str(header).lower() for header in headers})
            or any(
                not re.fullmatch(r"[!#$%&'*+\-.^_`|~0-9A-Za-z]+", str(header))
                for header in headers
            )
        ):
            self.error(
                "STATIC-HTTP-002",
                "plan requires unique non-wildcard valid exact request headers",
                plan_path,
            )
        database = plan.get("database", {})
        if database.get("kind") != "neon_postgres":
            self.warning("STATIC-DB-001", "default minimal-idle plan is not using the expected Neon profile", plan_path)
        if template_path.is_file():
            text = template_path.read_text()
            for token in [
                "AWS::EC2::NatGateway",
                "ProvisionedConcurrency",
                "Type: AWS::RDS::DBInstance",
                "AllowOrigins: ['*']",
                "DefaultAuthorizer:",
                "lambdaVersion:",
            ]:
                if token in text:
                    self.error("STATIC-SAM-001", f"minimal template contains forbidden token {token}", template_path)
            for token in [
                "CandidateApiInvokePermission:",
                "FunctionName: !Ref ApiFunction.Alias",
                "LiveFunctionAlias:",
                "LiveApiInvokePermission:",
                "FunctionName: !Ref LiveFunctionAlias",
                "lambdaAlias: 'candidate'",
                "lambdaAlias: 'live'",
            ]:
                if token not in text:
                    self.error(
                        "STATIC-SAM-005",
                        f"minimal template omits required alias boundary {token}",
                        template_path,
                    )
            for operation in self.contract_operations:
                if operation["path"] not in text or operation["method"].upper() not in text:
                    self.error("STATIC-SAM-002", f"template omits route {operation['method'].upper()} {operation['path']}", template_path)
            for origin in origins:
                if origin not in text:
                    self.error("STATIC-SAM-003", f"template omits exact origin {origin}", template_path)
            for configured_header in headers:
                if configured_header not in text:
                    self.error(
                        "STATIC-SAM-004",
                        f"template omits exact request header {configured_header}",
                        template_path,
                    )

    def validate_no_placeholders(self) -> None:
        patterns = [
            r"\btodo!\s*\(", r"\bunimplemented!\s*\(",
            r"panic!\s*\(\s*\"not implemented", r"PLACEHOLDER_SECRET",
        ]
        count = 0
        for path in sorted(self.root.rglob("*")):
            if not path.is_file() or ignored_path(self.root, path):
                continue
            if path.resolve() in {Path(__file__).resolve(), (self.root / "scripts/deep_review.py").resolve()}:
                continue
            if path.suffix not in {".rs", ".sh", ".py", ".toml", ".yaml", ".yml", ".sql"}:
                continue
            count += 1
            source = path.read_text(errors="replace")
            for pattern in patterns:
                if re.search(pattern, source, flags=re.IGNORECASE):
                    self.error("STATIC-PLACEHOLDER-001", f"placeholder implementation matches {pattern}", path)
        self.metrics["implementation_files_scanned"] = count


def response_has_problem_media(document: dict[str, Any], response: Any) -> bool:
    if not isinstance(response, dict):
        return False
    reference = response.get("$ref")
    if isinstance(reference, str) and reference.startswith("#/"):
        current: Any = document
        for segment in reference[2:].split("/"):
            if not isinstance(current, dict) or segment not in current:
                return False
            current = current[segment]
        response = current
    return (
        isinstance(response, dict)
        and isinstance(response.get("content"), dict)
        and "application/problem+json" in response["content"]
    )


def feature_closure_tokens(
    features: dict[str, list[str]],
    roots: list[str],
) -> set[str]:
    seen: set[str] = set()

    def visit(token: str) -> None:
        if token in seen:
            return
        seen.add(token)
        if token in features:
            for child in features[token]:
                visit(child)

    for root in roots:
        visit(root)
    return seen


def derive_milestone_status(statuses: list[str]) -> str:
    if statuses and all(status == "complete" for status in statuses):
        return "complete"
    if any(status in {"active", "complete"} for status in statuses):
        return "active"
    return "planned"


def parse_usize_product(expression: str) -> int:
    result = 1
    for factor in expression.split("*"):
        result *= int(factor.strip().replace("_", ""))
    return result


def security_allows_anonymous(security: Any) -> tuple[bool, bool]:
    if security is None:
        return True, True
    if not isinstance(security, list):
        return False, False
    allows_anonymous = not security
    for requirement in security:
        if not isinstance(requirement, dict):
            return False, False
        allows_anonymous = allows_anonymous or not requirement
        for scopes in requirement.values():
            if not isinstance(scopes, list) or not all(
                isinstance(scope, str) for scope in scopes
            ):
                return False, False
    return allows_anonymous, True


def valid_auth_policy(auth: Any, public: bool) -> bool:
    if auth is None:
        return True
    if auth == "public":
        return public
    if auth == "authenticated":
        return not public
    if not isinstance(auth, dict) or public or auth.get("mode") != "permission_scoped":
        return False
    permissions = auth.get("permissions")
    return (
        isinstance(permissions, list)
        and bool(permissions)
        and all(
            isinstance(permission, str)
            and re.fullmatch(r"[a-z0-9._:-]{1,128}", permission)
            for permission in permissions
        )
        and len(permissions) == len(set(permissions))
    )


def resolve_local_reference(document: dict[str, Any], value: Any) -> Any | None:
    if not isinstance(value, dict):
        return value
    reference = value.get("$ref")
    if reference is None:
        return value
    if not isinstance(reference, str) or not reference.startswith("#/"):
        return None
    current: Any = document
    for segment in reference[2:].split("/"):
        segment = segment.replace("~1", "/").replace("~0", "~")
        if not isinstance(current, dict) or segment not in current:
            return None
        current = current[segment]
    return current


def effective_parameters(
    document: dict[str, Any],
    path_parameters: Any,
    operation_parameters: Any,
) -> tuple[list[dict[str, Any]], bool]:
    effective: dict[tuple[str, str], dict[str, Any]] = {}
    invalid_reference = False
    for parameters in (path_parameters, operation_parameters):
        if not isinstance(parameters, list):
            continue
        for value in parameters:
            parameter = resolve_local_reference(document, value)
            if parameter is None:
                invalid_reference = True
                continue
            if not isinstance(parameter, dict):
                continue
            name = parameter.get("name")
            parameter_in = parameter.get("in")
            if isinstance(name, str) and isinstance(parameter_in, str):
                effective[(name.lower(), parameter_in.lower())] = parameter
    return list(effective.values()), invalid_reference


def walk_openapi_schema_objects(document: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    objects: list[tuple[str, dict[str, Any]]] = []

    def visit_schema(value: Any, location: str) -> None:
        if not isinstance(value, dict):
            return
        schema_type = value.get("type")
        if schema_type == "object" or (
            isinstance(schema_type, list) and "object" in schema_type
        ):
            objects.append((location, value))
        for keyword in ("properties", "patternProperties", "dependentSchemas", "$defs"):
            children = value.get(keyword)
            if isinstance(children, dict):
                for name, child in children.items():
                    visit_schema(child, f"{location}.{keyword}.{name}")
        for keyword in ("allOf", "anyOf", "oneOf", "prefixItems"):
            children = value.get(keyword)
            if isinstance(children, list):
                for index, child in enumerate(children):
                    visit_schema(child, f"{location}.{keyword}[{index}]")
        for keyword in (
            "items",
            "contains",
            "not",
            "if",
            "then",
            "else",
            "propertyNames",
            "additionalProperties",
            "unevaluatedProperties",
        ):
            child = value.get(keyword)
            if isinstance(child, dict):
                visit_schema(child, f"{location}.{keyword}")

    def visit_content(value: Any, location: str) -> None:
        if not isinstance(value, dict):
            return
        for media_type, media in value.items():
            if isinstance(media, dict) and "schema" in media:
                visit_schema(media["schema"], f"{location}.{media_type}.schema")
            encodings = media.get("encoding") if isinstance(media, dict) else None
            if isinstance(encodings, dict):
                for property_name, encoding in encodings.items():
                    headers = encoding.get("headers") if isinstance(encoding, dict) else None
                    if isinstance(headers, dict):
                        for name, header in headers.items():
                            visit_parameter(
                                header,
                                f"{location}.{media_type}.encoding.{property_name}.headers.{name}",
                            )

    def visit_parameter(value: Any, location: str) -> None:
        if not isinstance(value, dict):
            return
        if "schema" in value:
            visit_schema(value["schema"], f"{location}.schema")
        visit_content(value.get("content"), f"{location}.content")

    def visit_request_body(value: Any, location: str) -> None:
        if isinstance(value, dict):
            visit_content(value.get("content"), f"{location}.content")

    def visit_response(value: Any, location: str) -> None:
        if not isinstance(value, dict):
            return
        visit_content(value.get("content"), f"{location}.content")
        headers = value.get("headers")
        if isinstance(headers, dict):
            for name, header in headers.items():
                visit_parameter(header, f"{location}.headers.{name}")

    def visit_parameters(value: Any, location: str) -> None:
        if isinstance(value, list):
            for index, parameter in enumerate(value):
                visit_parameter(parameter, f"{location}[{index}]")

    def visit_callback(value: Any, location: str) -> None:
        if not isinstance(value, dict) or "$ref" in value:
            return
        for expression, path_item in value.items():
            visit_path_item(path_item, f"{location}.{expression}")

    def visit_path_item(value: Any, location: str) -> None:
        if not isinstance(value, dict):
            return
        visit_parameters(value.get("parameters"), f"{location}.parameters")
        for method in ("get", "put", "post", "delete", "options", "head", "patch", "trace"):
            operation = value.get(method)
            if not isinstance(operation, dict):
                continue
            operation_location = f"{location}.{method}"
            visit_parameters(
                operation.get("parameters"),
                f"{operation_location}.parameters",
            )
            visit_request_body(
                operation.get("requestBody"),
                f"{operation_location}.requestBody",
            )
            responses = operation.get("responses")
            if isinstance(responses, dict):
                for status, response in responses.items():
                    visit_response(
                        response,
                        f"{operation_location}.responses.{status}",
                    )
            callbacks = operation.get("callbacks")
            if isinstance(callbacks, dict):
                for name, callback in callbacks.items():
                    visit_callback(
                        callback,
                        f"{operation_location}.callbacks.{name}",
                    )

    components = document.get("components") or {}
    schemas = components.get("schemas") or {}
    if isinstance(schemas, dict):
        for name, schema in schemas.items():
            visit_schema(schema, f"$.components.schemas.{name}")
    component_visitors = {
        "parameters": visit_parameter,
        "headers": visit_parameter,
        "requestBodies": visit_request_body,
        "responses": visit_response,
    }
    if isinstance(components, dict):
        for section, visitor in component_visitors.items():
            entries = components.get(section)
            if isinstance(entries, dict):
                for name, value in entries.items():
                    visitor(value, f"$.components.{section}.{name}")
        callbacks = components.get("callbacks")
        if isinstance(callbacks, dict):
            for name, callback in callbacks.items():
                visit_callback(callback, f"$.components.callbacks.{name}")
        path_items = components.get("pathItems")
        if isinstance(path_items, dict):
            for name, path_item in path_items.items():
                visit_path_item(path_item, f"$.components.pathItems.{name}")
    for root in ("paths", "webhooks"):
        items = document.get(root)
        if isinstance(items, dict):
            for name, path_item in items.items():
                visit_path_item(path_item, f"$.{root}.{name}")
    return objects


def screaming_snake(value: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", value).replace("-", "_").upper()

def balanced_rust(source: str) -> str | None:
    """Check delimiter balance while skipping Rust comments and literal bodies.

    This is intentionally lexical only. It understands nested block comments, normal/byte
    strings, raw/byte-raw strings, character literals, and lifetimes well enough to avoid
    treating braces inside literals as code.
    """
    stack: list[tuple[str, int]] = []
    pairs = {')': '(', ']': '[', '}': '{'}
    i = 0
    line = 1
    mode = "code"
    raw_hashes = 0
    block_depth = 0

    def starts_raw(at: int) -> tuple[bool, int, int]:
        # Returns (matched, quote_index, hashes). Supports r"", r#""#, br"", br#""#.
        cursor = at
        if source.startswith("br", cursor):
            cursor += 2
        elif source.startswith("r", cursor):
            cursor += 1
        else:
            return False, at, 0
        hashes = 0
        while cursor < len(source) and source[cursor] == '#':
            hashes += 1
            cursor += 1
        if cursor < len(source) and source[cursor] == '"':
            return True, cursor, hashes
        return False, at, 0

    def starts_char(at: int) -> bool:
        # A lifetime ('a, '_) has no terminating quote. A character literal does.
        cursor = at + 1
        escaped = False
        while cursor < len(source) and source[cursor] != "\n":
            current = source[cursor]
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == "'":
                return True
            # Lifetimes are short identifiers and cannot contain punctuation or whitespace.
            elif cursor > at + 1 and current in " \t,;:<>[](){}=&|+-*/.!?":
                return False
            cursor += 1
        return False

    while i < len(source):
        char = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""

        if mode == "line_comment":
            if char == "\n":
                line += 1
                mode = "code"
            i += 1
            continue

        if mode == "block_comment":
            if char == "\n":
                line += 1
            if char == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if char == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    mode = "code"
                continue
            i += 1
            continue

        if mode == "string":
            if char == "\n":
                line += 1
            if char == "\\":
                i += 2
                continue
            if char == '"':
                mode = "code"
            i += 1
            continue

        if mode == "char":
            if char == "\n":
                return "unterminated char literal"
            if char == "\\":
                i += 2
                continue
            if char == "'":
                mode = "code"
            i += 1
            continue

        if mode == "raw":
            if char == "\n":
                line += 1
            if char == '"' and source[i + 1:i + 1 + raw_hashes] == "#" * raw_hashes:
                i += 1 + raw_hashes
                mode = "code"
                continue
            i += 1
            continue

        # code mode
        if char == "\n":
            line += 1
            i += 1
            continue
        if char == "/" and nxt == "/":
            mode = "line_comment"
            i += 2
            continue
        if char == "/" and nxt == "*":
            mode = "block_comment"
            block_depth = 1
            i += 2
            continue

        matched_raw, quote_index, hashes = starts_raw(i)
        if matched_raw:
            mode = "raw"
            raw_hashes = hashes
            i = quote_index + 1
            continue

        if char == "b" and nxt == '"':
            mode = "string"
            i += 2
            continue
        if char == '"':
            mode = "string"
            i += 1
            continue
        if char == "b" and nxt == "'" and starts_char(i + 1):
            mode = "char"
            i += 2
            continue
        if char == "'" and starts_char(i):
            mode = "char"
            i += 1
            continue

        if char in "([{":
            stack.append((char, line))
        elif char in ")]}" :
            if not stack or stack[-1][0] != pairs[char]:
                return f"unmatched {char!r} at line {line}"
            stack.pop()
        i += 1

    if mode == "block_comment":
        return "unterminated block comment"
    if mode == "string":
        return "unterminated string literal"
    if mode == "char":
        return "unterminated char literal"
    if mode == "raw":
        return "unterminated raw string literal"
    if stack:
        token, opened = stack[-1]
        return f"unclosed {token!r} opened at line {opened}"
    return None



def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    report = Validator(root).run()
    rendered = json.dumps(report, indent=2) + "\n"
    output = args.output or root / "target/minco/static-validation.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(rendered)
    print(rendered, end="")
    return 0 if report["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
