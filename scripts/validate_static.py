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
import py_compile
import re
import subprocess
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head"}


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
            "root": str(self.root),
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
        ]
        for item in required:
            path = self.root / item
            if not path.is_file():
                self.error("STATIC-001", f"required file is missing: {item}", path)

    def validate_data_files(self) -> None:
        counts = {"toml": 0, "yaml": 0, "json": 0}
        for path in sorted(self.root.rglob("*")):
            if not path.is_file() or any(part in {"target", ".git", ".jj", "__pycache__"} for part in path.parts):
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
                idempotent = bool(operation.get("x-minco-idempotent"))
                if idempotent:
                    parameters = operation.get("parameters") or []
                    header = any(
                        isinstance(parameter, dict)
                        and parameter.get("in") == "header"
                        and str(parameter.get("name", "")).lower() == "idempotency-key"
                        and parameter.get("required") is True
                        for parameter in parameters
                    )
                    if not header:
                        self.error("STATIC-CONTRACT-007", f"{operation_id} lacks required Idempotency-Key", contract_path)
                public = operation.get("security") == [] or operation.get("x-minco-auth") == "public"
                operations.append({
                    "operation_id": operation_id,
                    "method": method,
                    "path": route,
                    "authenticated": not public,
                    "idempotent": idempotent,
                })
        schemas = ((raw.get("components") or {}).get("schemas") or {})
        for name, schema in schemas.items():
            if isinstance(schema, dict) and schema.get("type") == "object" and schema.get("additionalProperties") is not False:
                self.error("STATIC-CONTRACT-008", f"object schema {name} must set additionalProperties: false", contract_path)
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

    def validate_rust_lexically(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.rs")):
            if any(part in {"target", ".git", ".jj"} for part in path.parts):
                continue
            count += 1
            issue = balanced_rust(path.read_text())
            if issue:
                self.error("STATIC-RUST-001", issue, path)
        self.metrics["rust_files"] = count

    def validate_python(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.py")):
            if any(part in {"target", ".git", ".jj", "__pycache__"} for part in path.parts):
                continue
            count += 1
            try:
                py_compile.compile(str(path), doraise=True)
            except py_compile.PyCompileError as exc:
                self.error("STATIC-PYTHON-001", str(exc), path)
        self.metrics["python_files"] = count

    def validate_shell(self) -> None:
        count = 0
        for path in sorted(self.root.rglob("*.sh")):
            if any(part in {"target", ".git", ".jj"} for part in path.parts):
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
        plan = json.loads(plan_path.read_text())
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
        database = plan.get("database", {})
        if database.get("kind") != "neon_postgres":
            self.warning("STATIC-DB-001", "default minimal-idle plan is not using the expected Neon profile", plan_path)
        if template_path.is_file():
            text = template_path.read_text()
            for token in ["AWS::EC2::NatGateway", "ProvisionedConcurrency", "Type: AWS::RDS::DBInstance", "AllowOrigins: ['*']"]:
                if token in text:
                    self.error("STATIC-SAM-001", f"minimal template contains forbidden token {token}", template_path)
            for operation in self.contract_operations:
                if operation["path"] not in text or operation["method"].upper() not in text:
                    self.error("STATIC-SAM-002", f"template omits route {operation['method'].upper()} {operation['path']}", template_path)
            for origin in origins:
                if origin not in text:
                    self.error("STATIC-SAM-003", f"template omits exact origin {origin}", template_path)

    def validate_no_placeholders(self) -> None:
        patterns = [
            r"\btodo!\s*\(", r"\bunimplemented!\s*\(",
            r"panic!\s*\(\s*\"not implemented", r"PLACEHOLDER_SECRET",
        ]
        count = 0
        for path in sorted(self.root.rglob("*")):
            if not path.is_file() or any(part in {"target", ".git", ".jj", "__pycache__"} for part in path.parts):
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
