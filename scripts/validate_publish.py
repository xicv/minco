#!/usr/bin/env python3
"""Validate Minco's crates.io packaging contract without invoking Cargo.

This gate checks metadata, publish restrictions, package contents, versioned path
dependencies, the umbrella crate, Cargo subcommand naming, and the initial
multi-package publication order. It complements, but never replaces,
`cargo publish --dry-run` on the pinned toolchain.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_METADATA = (
    "description",
    "documentation",
    "homepage",
    "repository",
    "readme",
    "license",
)
DEPENDENCY_SECTIONS = ("dependencies", "build-dependencies")


@dataclass(frozen=True)
class Finding:
    code: str
    severity: str
    message: str
    path: str | None = None


class PublishValidator:
    def __init__(
        self,
        root: Path,
        check_registry: bool,
        expect_unpublished: bool,
        require_registry: bool,
    ) -> None:
        self.root = root
        self.check_registry = check_registry or expect_unpublished or require_registry
        self.expect_unpublished = expect_unpublished
        self.require_registry = require_registry
        self.registry_checks_succeeded = 0
        self.findings: list[Finding] = []
        self.metrics: dict[str, Any] = {}
        self.workspace = tomllib.loads((root / "Cargo.toml").read_text())
        self.workspace_version = str(self.workspace["workspace"]["package"]["version"])
        self.workspace_dependencies = self.workspace["workspace"].get("dependencies", {})
        self.packages: dict[str, tuple[Path, dict[str, Any]]] = {}

    def add(self, code: str, severity: str, message: str, path: Path | None = None) -> None:
        relative = None
        if path is not None:
            try:
                relative = str(path.relative_to(self.root))
            except ValueError:
                relative = str(path)
        self.findings.append(Finding(code, severity, message, relative))

    def error(self, code: str, message: str, path: Path | None = None) -> None:
        self.add(code, "error", message, path)

    def warning(self, code: str, message: str, path: Path | None = None) -> None:
        self.add(code, "warning", message, path)

    def run(self) -> dict[str, Any]:
        self.load_packages()
        self.validate_package_metadata()
        self.validate_internal_dependencies()
        self.validate_publish_order()
        self.validate_facade()
        self.validate_cargo_subcommand()
        if self.check_registry:
            self.validate_registry_names()

        errors = sum(item.severity == "error" for item in self.findings)
        warnings = sum(item.severity == "warning" for item in self.findings)
        publishable = sorted(
            name for name, (_, package) in self.packages.items() if package.get("publish") is not False
        )
        private = sorted(set(self.packages) - set(publishable))
        self.metrics.update(
            {
                "workspace_version": self.workspace_version,
                "workspace_packages": len(self.packages),
                "publishable_packages": len(publishable),
                "private_packages": len(private),
                "publishable_names": publishable,
                "private_names": private,
                "registry_check_performed": self.check_registry,
                "registry_checks_succeeded": self.registry_checks_succeeded,
                "expect_unpublished": self.expect_unpublished,
                "require_registry": self.require_registry,
            }
        )
        return {
            "schema_version": 1,
            "status": "ok" if errors == 0 else "failed",
            "errors": errors,
            "warnings": warnings,
            "metrics": self.metrics,
            "findings": [asdict(item) for item in self.findings],
            "limitations": [
                "This validator does not normalize Cargo manifests or compile packaged crates.",
                "Run cargo publish --dry-run for the complete selected package set before upload.",
            ],
        }

    def load_packages(self) -> None:
        members = self.workspace["workspace"].get("members", [])
        for member in members:
            manifest = self.root / member / "Cargo.toml"
            if not manifest.is_file():
                self.error("PUBLISH-001", f"workspace member has no manifest: {member}", manifest)
                continue
            data = tomllib.loads(manifest.read_text())
            package = data.get("package", {})
            name = package.get("name")
            if not name:
                self.error("PUBLISH-002", f"workspace member has no package name: {member}", manifest)
                continue
            if name in self.packages:
                self.error("PUBLISH-003", f"duplicate package name: {name}", manifest)
                continue
            self.packages[name] = (manifest, package | {"_manifest": data})

    def resolved_package_value(self, package: dict[str, Any], key: str) -> Any:
        value = package.get(key)
        if isinstance(value, dict) and value.get("workspace") is True:
            return self.workspace["workspace"].get("package", {}).get(key)
        return value

    def validate_package_metadata(self) -> None:
        for name, (manifest, package) in sorted(self.packages.items()):
            publish = package.get("publish")
            if publish is False:
                if not str(manifest.relative_to(self.root)).startswith("examples/"):
                    self.warning(
                        "PUBLISH-010",
                        f"non-example package {name} is private; confirm this is intentional",
                        manifest,
                    )
                continue
            if publish != ["crates-io"]:
                self.error(
                    "PUBLISH-011",
                    f"{name} must restrict publication to crates-io, found {publish!r}",
                    manifest,
                )
            if self.resolved_package_value(package, "version") != self.workspace_version:
                self.error(
                    "PUBLISH-012",
                    f"{name} does not inherit workspace version {self.workspace_version}",
                    manifest,
                )
            for key in REQUIRED_METADATA:
                value = self.resolved_package_value(package, key)
                if not value:
                    self.error("PUBLISH-013", f"{name} lacks package metadata: {key}", manifest)
            readme = self.resolved_package_value(package, "readme")
            if isinstance(readme, str) and not (manifest.parent / readme).is_file():
                self.error("PUBLISH-014", f"{name} readme does not exist: {readme}", manifest)
            for license_name in ("LICENSE-MIT", "LICENSE-APACHE"):
                if not (manifest.parent / license_name).is_file():
                    self.error(
                        "PUBLISH-015",
                        f"{name} package does not include {license_name}",
                        manifest,
                    )
            include = {
                value.removeprefix("/")
                for value in package.get("include", [])
                if isinstance(value, str)
            }
            for required in ("src/**", "Cargo.toml", "README.md", "LICENSE-MIT", "LICENSE-APACHE"):
                if required not in include:
                    self.error(
                        "PUBLISH-016",
                        f"{name} package.include lacks {required}",
                        manifest,
                    )
            description = str(package.get("description", ""))
            if len(description) > 255:
                self.error("PUBLISH-017", f"{name} description exceeds 255 characters", manifest)
            keywords = package.get("keywords", [])
            if len(keywords) > 5:
                self.error("PUBLISH-018", f"{name} has more than five keywords", manifest)
            for keyword in keywords:
                if len(keyword) > 20 or not re.fullmatch(r"[A-Za-z0-9_-]+", keyword):
                    self.error("PUBLISH-019", f"{name} has invalid keyword {keyword!r}", manifest)
            if not package.get("categories"):
                self.error("PUBLISH-020", f"{name} has no crates.io categories", manifest)

    def validate_internal_dependencies(self) -> None:
        publishable = {
            name for name, (_, package) in self.packages.items() if package.get("publish") is not False
        }
        for dependency in publishable:
            spec = self.workspace_dependencies.get(dependency)
            if not isinstance(spec, dict):
                self.error(
                    "PUBLISH-030",
                    f"workspace dependency {dependency} must specify path and version",
                    self.root / "Cargo.toml",
                )
                continue
            if spec.get("version") != self.workspace_version or not spec.get("path"):
                self.error(
                    "PUBLISH-031",
                    f"workspace dependency {dependency} must use path plus version {self.workspace_version}",
                    self.root / "Cargo.toml",
                )

        for name, (manifest, package) in sorted(self.packages.items()):
            if package.get("publish") is False:
                continue
            data = package["_manifest"]
            for section in DEPENDENCY_SECTIONS:
                for dependency, spec in data.get(section, {}).items():
                    internal_name = dependency
                    if isinstance(spec, dict) and spec.get("package"):
                        internal_name = spec["package"]
                    if internal_name not in publishable:
                        continue
                    if isinstance(spec, dict) and spec.get("workspace") is True:
                        inherited = self.workspace_dependencies.get(dependency)
                        if not isinstance(inherited, dict) or inherited.get("version") != self.workspace_version:
                            self.error(
                                "PUBLISH-032",
                                f"{name} inherits unversioned internal dependency {dependency}",
                                manifest,
                            )
                    elif not isinstance(spec, dict) or spec.get("version") != self.workspace_version:
                        self.error(
                            "PUBLISH-033",
                            f"{name} has unversioned internal dependency {dependency}",
                            manifest,
                        )

    def internal_dependencies(self, name: str) -> set[str]:
        manifest, package = self.packages[name]
        if package.get("publish") is False:
            return set()
        publishable = {
            item for item, (_, value) in self.packages.items() if value.get("publish") is not False
        }
        dependencies: set[str] = set()
        data = package["_manifest"]
        for section in DEPENDENCY_SECTIONS:
            for dependency, spec in data.get(section, {}).items():
                actual = spec.get("package", dependency) if isinstance(spec, dict) else dependency
                if actual in publishable:
                    dependencies.add(actual)
        return dependencies

    def validate_publish_order(self) -> None:
        metadata = self.workspace["workspace"].get("metadata", {})
        order = metadata.get("minco", {}).get("release", {}).get("publish", [])
        publishable = {
            name for name, (_, package) in self.packages.items() if package.get("publish") is not False
        }
        if len(order) != len(set(order)):
            self.error("PUBLISH-040", "publication order contains duplicate package names", self.root / "Cargo.toml")
        if set(order) != publishable:
            missing = sorted(publishable - set(order))
            extra = sorted(set(order) - publishable)
            self.error(
                "PUBLISH-041",
                f"publication order does not match publishable packages; missing={missing}, extra={extra}",
                self.root / "Cargo.toml",
            )
            return
        positions = {name: index for index, name in enumerate(order)}
        for package in order:
            for dependency in self.internal_dependencies(package):
                if positions[dependency] >= positions[package]:
                    self.error(
                        "PUBLISH-042",
                        f"{package} is ordered before internal dependency {dependency}",
                        self.root / "Cargo.toml",
                    )
        self.metrics["publish_order"] = order

    def validate_facade(self) -> None:
        if "minco" not in self.packages:
            self.error("PUBLISH-050", "workspace lacks the minco facade crate")
            return
        manifest, package = self.packages["minco"]
        data = package["_manifest"]
        features = data.get("features", {})
        default = set(features.get("default", []))
        if default != {"contract", "http", "default-plugins"}:
            self.error(
                "PUBLISH-051",
                f"minco default features changed unexpectedly: {sorted(default)}",
                manifest,
            )
        required = {
            "contract",
            "http",
            "plan",
            "release",
            "test",
            "default-plugins",
            "official-plugins",
            "plugin-health",
            "plugin-observability",
            "plugin-idempotency",
            "plugin-sessions",
            "plugin-identity",
            "plugin-object-storage",
            "plugin-events",
            "plugin-notifications",
            "plugin-audit",
            "plugin-feedback",
            "sqlx-postgres",
            "sqlx-sqlite",
            "aws-lambda",
            "full",
        }
        missing = sorted(required - set(features))
        if missing:
            self.error("PUBLISH-052", f"minco facade lacks required features: {missing}", manifest)
        source = manifest.parent / "src/lib.rs"
        text = source.read_text() if source.is_file() else ""
        for symbol in ("default_plugin_manager", "compose_defaults", "pub mod prelude"):
            if symbol not in text:
                self.error("PUBLISH-053", f"minco facade lacks {symbol}", source)

    def validate_cargo_subcommand(self) -> None:
        if "cargo-minco" not in self.packages:
            self.error("PUBLISH-060", "workspace lacks cargo-minco package")
            return
        manifest, package = self.packages["cargo-minco"]
        bins = package["_manifest"].get("bin", [])
        if not any(binary.get("name") == "cargo-minco" for binary in bins):
            self.error("PUBLISH-061", "cargo-minco package lacks cargo-minco binary", manifest)
        alias_path = self.root / ".cargo/config.toml"
        alias = tomllib.loads(alias_path.read_text()).get("alias", {}).get("minco")
        if not isinstance(alias, str) or "-p cargo-minco" not in alias:
            self.error("PUBLISH-062", "local cargo minco alias does not target cargo-minco", alias_path)
        source = self.root / "crates/minco-cli/src/main.rs"
        text = source.read_text() if source.is_file() else ""
        if "normalize_cargo_subcommand_args" not in text or 'OsStr::new("minco")' not in text:
            self.error(
                "PUBLISH-063",
                "cargo-minco does not normalize Cargo's injected subcommand argument",
                source,
            )
        include = {
            value.removeprefix("/")
            for value in package.get("include", [])
            if isinstance(value, str)
        }
        if "templates/**" not in include:
            self.error(
                "PUBLISH-064",
                "cargo-minco package.include must contain templates/** for `cargo minco new`",
                manifest,
            )
        library_source = self.root / "crates/minco-cli/src/lib.rs"
        if not library_source.is_file():
            self.error(
                "PUBLISH-068",
                "cargo-minco lacks a library documentation target required by docs.rs",
                library_source,
            )
        new_source = self.root / "crates/minco-cli/src/new_cmd.rs"
        new_text = new_source.read_text() if new_source.is_file() else ""
        if "create_project" not in new_text or "NewProjectOptions" not in new_text or "Command::New" not in text:
            self.error(
                "PUBLISH-065",
                "cargo-minco lacks the application scaffolding command",
                new_source,
            )
        scaffold_test = self.root / "scripts/test/scaffold_templates.py"
        if not scaffold_test.is_file():
            self.error(
                "PUBLISH-066",
                "cargo-minco application templates lack a deterministic validation test",
                scaffold_test,
            )
        if not (self.root / "Cargo.lock").is_file():
            self.warning(
                "PUBLISH-067",
                "Cargo.lock is absent; generate, review, and commit it before cargo publish --dry-run",
                self.root / "Cargo.lock",
            )

    def validate_registry_names(self) -> None:
        version = self.workspace_version
        for name, (manifest, package) in sorted(self.packages.items()):
            if package.get("publish") is False:
                continue
            url = f"https://crates.io/api/v1/crates/{name}"
            request = urllib.request.Request(url, headers={"User-Agent": "minco-publish-validator/0.1"})
            try:
                with urllib.request.urlopen(request, timeout=15) as response:
                    payload = json.load(response)
                self.registry_checks_succeeded += 1
            except urllib.error.HTTPError as exc:
                if exc.code == 404:
                    self.registry_checks_succeeded += 1
                    continue
                message = f"registry check failed for {name}: HTTP {exc.code}"
                if self.require_registry:
                    self.error("PUBLISH-070", message, manifest)
                else:
                    self.warning("PUBLISH-070", message, manifest)
                continue
            except OSError as exc:
                message = f"registry check unavailable for {name}: {exc}"
                if self.require_registry:
                    self.error("PUBLISH-071", message, manifest)
                else:
                    self.warning("PUBLISH-071", message, manifest)
                continue
            versions = {
                item.get("num") for item in payload.get("versions", []) if not item.get("yanked", False)
            }
            if self.expect_unpublished:
                self.error(
                    "PUBLISH-073",
                    f"crate name {name} already exists on crates.io; first publication cannot claim it",
                    manifest,
                )
            elif version in versions:
                self.error(
                    "PUBLISH-072",
                    f"{name} {version} already exists on crates.io; publishing this version will fail",
                    manifest,
                )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--check-registry", action="store_true")
    parser.add_argument(
        "--expect-unpublished",
        action="store_true",
        help="fail if any selected crate name already exists; use immediately before the first release",
    )
    parser.add_argument(
        "--require-registry",
        action="store_true",
        help="treat registry connectivity failures as errors",
    )
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report = PublishValidator(
        args.root.resolve(),
        args.check_registry,
        args.expect_unpublished,
        args.require_registry,
    ).run()
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered)
    sys.stdout.write(rendered)
    return 0 if report["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
