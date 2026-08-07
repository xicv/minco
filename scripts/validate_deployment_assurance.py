#!/usr/bin/env python3
"""Validate Minco's machine-readable deployment support and evidence claims."""
from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ASSURANCE_RELATIVE = Path("verification/deployment-assurance.toml")
TRUTH_RELATIVE = Path("verification/repository-truth.toml")
MODEL_RELATIVE = Path("crates/minco-plan/src/model.rs")
COST_RELATIVE = Path("crates/minco-plan/src/cost.rs")
SAM_RELATIVE = Path("crates/minco-plan/src/sam.rs")

SUPPORTED_STATUSES = {"stable", "bounded"}
KNOWN_STATUSES = SUPPORTED_STATUSES | {"declared", "deferred"}
PROFILE_KEYS = {
    "id",
    "status",
    "scope",
    "runtime",
    "ingress",
    "default",
    "zero_provisioned_compute",
    "idle_cost_classes",
    "wake_sources",
    "dimensions",
    "source_paths",
    "evidence_paths",
    "documentation_paths",
    "test_commands",
    "decision",
    "blockers",
}


@dataclass(frozen=True)
class Finding:
    code: str
    severity: str
    message: str
    path: str | None = None


def camel_to_snake(value: str) -> str:
    """Convert a Rust-style enum variant to the serialized snake-case form."""
    value = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", value)
    value = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", value)
    return value.lower()


def enum_variants(source: str, enum_name: str) -> set[str]:
    """Return unit or data enum variant names from one Rust enum declaration."""
    match = re.search(
        rf"pub\s+enum\s+{re.escape(enum_name)}\s*\{{(?P<body>.*?)^\}}",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        return set()
    return set(
        re.findall(
            r"^\s*([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)",
            match.group("body"),
            flags=re.MULTILINE,
        )
    )


def safe_relative_path(value: str) -> bool:
    path = Path(value)
    return bool(value) and not path.is_absolute() and ".." not in path.parts


def string_list(value: Any) -> list[str] | None:
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        return None
    return value


class Validator:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.findings: list[Finding] = []
        self.metrics: dict[str, Any] = {}

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
        assurance_path = self.root / ASSURANCE_RELATIVE
        truth_path = self.root / TRUTH_RELATIVE
        model_path = self.root / MODEL_RELATIVE
        cost_path = self.root / COST_RELATIVE
        sam_path = self.root / SAM_RELATIVE
        required_paths = [assurance_path, truth_path, model_path, cost_path, sam_path]
        for path in required_paths:
            if not path.is_file():
                self.error(
                    "ASSURANCE-FILE-001",
                    f"required deployment-assurance input is missing: {self.relative(path)}",
                    path,
                )
        if self.findings:
            return self.report()

        try:
            assurance = tomllib.loads(assurance_path.read_text())
            truth = tomllib.loads(truth_path.read_text())
        except (OSError, tomllib.TOMLDecodeError) as error:
            self.error("ASSURANCE-DATA-001", f"invalid assurance input: {error}")
            return self.report()

        if assurance.get("schema") != 1:
            self.error(
                "ASSURANCE-SCHEMA-001",
                "deployment assurance schema must be 1",
                assurance_path,
            )
        baseline = assurance.get("published_baseline")
        if baseline != truth.get("published_baseline"):
            self.error(
                "ASSURANCE-TRUTH-001",
                "deployment assurance published_baseline differs from repository truth",
                assurance_path,
            )
        reviewed_commit = assurance.get("reviewed_release_commit")
        truth_commit = truth.get("published_release_commit")
        if not isinstance(reviewed_commit, str) or not re.fullmatch(
            r"[0-9a-f]{40}", reviewed_commit
        ):
            self.error(
                "ASSURANCE-TRUTH-002",
                "reviewed_release_commit must be one exact lowercase commit SHA",
                assurance_path,
            )
        elif reviewed_commit != truth_commit:
            self.error(
                "ASSURANCE-TRUTH-003",
                "deployment assurance release commit differs from repository truth",
                assurance_path,
            )

        policy = assurance.get("policy")
        if not isinstance(policy, dict):
            self.error(
                "ASSURANCE-POLICY-001",
                "deployment assurance requires one policy table",
                assurance_path,
            )
            return self.report()
        required_dimensions = self.validated_policy_list(
            policy, "required_aws_dimensions", assurance_path
        )
        local_dimensions = self.validated_policy_list(
            policy, "required_local_dimensions", assurance_path
        )
        forbidden_cost_classes = self.validated_policy_list(
            policy, "forbidden_default_cost_classes", assurance_path
        )
        forbidden_wake_sources = self.validated_policy_list(
            policy, "forbidden_default_wake_sources", assurance_path
        )

        model_source = model_path.read_text()
        cost_source = cost_path.read_text()
        sam_source = sam_path.read_text()
        runtime_variants = enum_variants(model_source, "RuntimePlan")
        ingress_variants = enum_variants(model_source, "IngressPlan")
        cost_variants = {
            camel_to_snake(variant) for variant in enum_variants(cost_source, "CostClass")
        }
        if not runtime_variants:
            self.error(
                "ASSURANCE-ENUM-001",
                "RuntimePlan variants could not be derived",
                model_path,
            )
        if not ingress_variants:
            self.error(
                "ASSURANCE-ENUM-002",
                "IngressPlan variants could not be derived",
                model_path,
            )
        if not cost_variants:
            self.error(
                "ASSURANCE-ENUM-004",
                "CostClass variants could not be derived",
                cost_path,
            )

        profiles = assurance.get("profile")
        if not isinstance(profiles, list) or not profiles:
            self.error(
                "ASSURANCE-PROFILE-001",
                "deployment assurance requires at least one profile",
                assurance_path,
            )
            return self.report()

        profile_ids: set[str] = set()
        topology_keys: set[tuple[str, str]] = set()
        claimed_runtimes: set[str] = set()
        claimed_ingresses: set[str] = set()
        defaults: list[dict[str, Any]] = []
        function_url_profile: dict[str, Any] | None = None
        api_gateway_profile: dict[str, Any] | None = None

        for index, profile in enumerate(profiles):
            if not isinstance(profile, dict):
                self.error(
                    "ASSURANCE-PROFILE-002",
                    f"profile at index {index} must be a table",
                    assurance_path,
                )
                continue
            unknown_keys = sorted(set(profile) - PROFILE_KEYS)
            if unknown_keys:
                self.error(
                    "ASSURANCE-PROFILE-003",
                    f"profile {profile.get('id', index)!r} has unknown keys: {unknown_keys}",
                    assurance_path,
                )
            profile_id = profile.get("id")
            if not isinstance(profile_id, str) or not re.fullmatch(
                r"[a-z0-9]+(?:-[a-z0-9]+)*", profile_id
            ):
                self.error(
                    "ASSURANCE-PROFILE-004",
                    f"profile at index {index} has an invalid stable id",
                    assurance_path,
                )
                profile_id = f"index-{index}"
            elif profile_id in profile_ids:
                self.error(
                    "ASSURANCE-PROFILE-005",
                    f"duplicate deployment assurance profile id: {profile_id}",
                    assurance_path,
                )
            profile_ids.add(profile_id)

            status = profile.get("status")
            if status not in KNOWN_STATUSES:
                self.error(
                    "ASSURANCE-STATUS-001",
                    f"profile {profile_id} has unsupported status {status!r}",
                    assurance_path,
                )
            scope = profile.get("scope")
            if scope not in {"aws", "local"}:
                self.error(
                    "ASSURANCE-SCOPE-001",
                    f"profile {profile_id} must use aws or local scope",
                    assurance_path,
                )

            runtime = profile.get("runtime")
            ingress = profile.get("ingress")
            if not isinstance(runtime, str) or runtime not in runtime_variants:
                self.error(
                    "ASSURANCE-ENUM-005",
                    f"profile {profile_id} references unknown RuntimePlan {runtime!r}",
                    assurance_path,
                )
            else:
                claimed_runtimes.add(runtime)
            if not isinstance(ingress, str) or ingress not in ingress_variants:
                self.error(
                    "ASSURANCE-ENUM-006",
                    f"profile {profile_id} references unknown IngressPlan {ingress!r}",
                    assurance_path,
                )
            else:
                claimed_ingresses.add(ingress)
            if isinstance(runtime, str) and isinstance(ingress, str):
                topology = (runtime, ingress)
                if topology in topology_keys:
                    self.error(
                        "ASSURANCE-PROFILE-006",
                        f"duplicate runtime/ingress assurance topology: {runtime}/{ingress}",
                        assurance_path,
                    )
                topology_keys.add(topology)

            default = profile.get("default")
            if not isinstance(default, bool):
                self.error(
                    "ASSURANCE-DEFAULT-001",
                    f"profile {profile_id} default must be boolean",
                    assurance_path,
                )
            elif default:
                defaults.append(profile)

            dimensions = self.validated_profile_list(
                profile, "dimensions", profile_id, assurance_path
            )
            idle_cost_classes = self.validated_profile_list(
                profile, "idle_cost_classes", profile_id, assurance_path
            )
            wake_sources = self.validated_profile_list(
                profile, "wake_sources", profile_id, assurance_path
            )
            source_paths = self.validated_profile_list(
                profile, "source_paths", profile_id, assurance_path
            )
            evidence_paths = self.validated_profile_list(
                profile, "evidence_paths", profile_id, assurance_path
            )
            documentation_paths = self.validated_profile_list(
                profile, "documentation_paths", profile_id, assurance_path
            )
            test_commands = self.validated_profile_list(
                profile, "test_commands", profile_id, assurance_path
            )
            blockers = self.validated_profile_list(
                profile, "blockers", profile_id, assurance_path
            )
            for cost_class in idle_cost_classes:
                if cost_class not in cost_variants:
                    self.error(
                        "ASSURANCE-COST-001",
                        f"profile {profile_id} references unknown CostClass {cost_class!r}",
                        assurance_path,
                    )

            for key, paths in (
                ("source_paths", source_paths),
                ("evidence_paths", evidence_paths),
                ("documentation_paths", documentation_paths),
            ):
                for configured in paths:
                    if not safe_relative_path(configured):
                        self.error(
                            "ASSURANCE-PATH-001",
                            f"profile {profile_id} has unsafe {key} entry {configured!r}",
                            assurance_path,
                        )
                    elif not (self.root / configured).is_file():
                        self.error(
                            "ASSURANCE-PATH-002",
                            f"profile {profile_id} references missing {key} entry {configured}",
                            self.root / configured,
                        )

            supported = status in SUPPORTED_STATUSES
            if supported:
                required = required_dimensions if scope == "aws" else local_dimensions
                missing = sorted(set(required) - set(dimensions))
                if missing:
                    self.error(
                        "ASSURANCE-DIMENSION-001",
                        f"supported profile {profile_id} lacks dimensions: {missing}",
                        assurance_path,
                    )
                for key, values in (
                    ("source_paths", source_paths),
                    ("evidence_paths", evidence_paths),
                    ("documentation_paths", documentation_paths),
                    ("test_commands", test_commands),
                ):
                    if not values:
                        self.error(
                            "ASSURANCE-EVIDENCE-001",
                            f"supported profile {profile_id} requires non-empty {key}",
                            assurance_path,
                        )
                if scope == "aws":
                    if profile.get("zero_provisioned_compute") is not True:
                        self.error(
                            "ASSURANCE-COST-002",
                            f"supported AWS profile {profile_id} must preserve zero provisioned compute",
                            assurance_path,
                        )
                    if not idle_cost_classes:
                        self.error(
                            "ASSURANCE-COST-003",
                            f"supported AWS profile {profile_id} must expose residual idle cost classes",
                            assurance_path,
                        )
                    if not wake_sources:
                        self.error(
                            "ASSURANCE-COST-004",
                            f"supported AWS profile {profile_id} must expose wake sources",
                            assurance_path,
                        )
                    joined_checks = "\n".join(test_commands + evidence_paths).lower()
                    if "candidate-load" not in joined_checks and "perf" not in joined_checks:
                        self.error(
                            "ASSURANCE-PERF-001",
                            f"supported AWS profile {profile_id} lacks performance evidence",
                            assurance_path,
                        )
                    if "candidate-recovery" not in joined_checks and "recovery" not in joined_checks:
                        self.error(
                            "ASSURANCE-RECOVERY-001",
                            f"supported AWS profile {profile_id} lacks recovery evidence",
                            assurance_path,
                        )
            else:
                if profile.get("default") is True:
                    self.error(
                        "ASSURANCE-DEFAULT-002",
                        f"non-supported profile {profile_id} cannot be the default",
                        assurance_path,
                    )
                if not isinstance(profile.get("decision"), str) or not profile[
                    "decision"
                ].strip():
                    self.error(
                        "ASSURANCE-DECISION-001",
                        f"non-supported profile {profile_id} requires a reviewable decision",
                        assurance_path,
                    )
                if not blockers:
                    self.error(
                        "ASSURANCE-DECISION-002",
                        f"non-supported profile {profile_id} requires explicit blockers",
                        assurance_path,
                    )

            if ingress == "LambdaFunctionUrl":
                function_url_profile = profile
            if ingress == "ApiGatewayHttpApi":
                api_gateway_profile = profile

        missing_runtimes = sorted(runtime_variants - claimed_runtimes)
        missing_ingresses = sorted(ingress_variants - claimed_ingresses)
        if missing_runtimes:
            self.error(
                "ASSURANCE-ENUM-003",
                f"RuntimePlan variants lack assurance profiles: {missing_runtimes}",
                assurance_path,
            )
        if missing_ingresses:
            self.error(
                "ASSURANCE-ENUM-007",
                f"IngressPlan variants lack assurance profiles: {missing_ingresses}",
                assurance_path,
            )

        configured_default = assurance.get("default_profile")
        configured_default_valid = isinstance(
            configured_default, str
        ) and bool(
            re.fullmatch(
                r"[a-z0-9]+(?:-[a-z0-9]+)*",
                configured_default,
            )
        )
        if not configured_default_valid:
            self.error(
                "ASSURANCE-DEFAULT-007",
                "default_profile must be one stable profile id",
                assurance_path,
            )

        if len(defaults) != 1:
            self.error(
                "ASSURANCE-DEFAULT-003",
                "deployment assurance must select exactly one default profile",
                assurance_path,
            )
        else:
            default = defaults[0]
            if (
                configured_default_valid
                and configured_default != default.get("id")
            ):
                self.error(
                    "ASSURANCE-DEFAULT-008",
                    "default_profile differs from the profile marked default",
                    assurance_path,
                )
            if default.get("status") != "stable" or default.get("scope") != "aws":
                self.error(
                    "ASSURANCE-DEFAULT-004",
                    "the default profile must be a stable AWS profile",
                    assurance_path,
                )
            default_costs = set(string_list(default.get("idle_cost_classes")) or [])
            forbidden_costs = sorted(default_costs & set(forbidden_cost_classes))
            if forbidden_costs:
                self.error(
                    "ASSURANCE-DEFAULT-005",
                    f"the default profile uses forbidden idle cost classes: {forbidden_costs}",
                    assurance_path,
                )
            default_wakes = set(string_list(default.get("wake_sources")) or [])
            forbidden_wakes = sorted(default_wakes & set(forbidden_wake_sources))
            if forbidden_wakes:
                self.error(
                    "ASSURANCE-DEFAULT-006",
                    f"the default profile uses forbidden wake sources: {forbidden_wakes}",
                    assurance_path,
                )

        function_url_implemented = any(
            marker in sam_source
            for marker in ("AWS::Lambda::Url", "FunctionUrlConfig:", "FunctionUrlAuthType")
        )
        if function_url_profile is None:
            self.error(
                "ASSURANCE-INGRESS-001",
                "LambdaFunctionUrl requires an explicit assurance profile",
                assurance_path,
            )
        elif function_url_profile.get("status") in SUPPORTED_STATUSES:
            if not function_url_implemented:
                self.error(
                    "ASSURANCE-INGRESS-002",
                    "LambdaFunctionUrl cannot be supported before the SAM renderer implements it",
                    sam_path,
                )
        elif function_url_implemented:
            self.error(
                "ASSURANCE-INGRESS-003",
                "LambdaFunctionUrl implementation appeared while its assurance status remains non-supported",
                assurance_path,
            )

        api_gateway_markers = (
            "AWS::Serverless::HttpApi",
            "StageVariables:",
            "AutoPublishAlias: candidate",
        )
        if api_gateway_profile is None or api_gateway_profile.get("status") not in SUPPORTED_STATUSES:
            self.error(
                "ASSURANCE-INGRESS-004",
                "ApiGatewayHttpApi must retain one supported assurance profile",
                assurance_path,
            )
        for marker in api_gateway_markers:
            if marker not in sam_source:
                self.error(
                    "ASSURANCE-INGRESS-005",
                    f"supported ApiGatewayHttpApi renderer marker is missing: {marker}",
                    sam_path,
                )

        self.metrics.update(
            {
                "profiles": len(profiles),
                "supported_profiles": sum(
                    isinstance(profile, dict)
                    and profile.get("status") in SUPPORTED_STATUSES
                    for profile in profiles
                ),
                "runtime_variants": len(runtime_variants),
                "ingress_variants": len(ingress_variants),
                "cost_classes": len(cost_variants),
            }
        )
        return self.report()

    def validated_policy_list(
        self, policy: dict[str, Any], key: str, assurance_path: Path
    ) -> list[str]:
        values = string_list(policy.get(key))
        if values is None or not values or len(values) != len(set(values)):
            self.error(
                "ASSURANCE-POLICY-002",
                f"policy.{key} must be a non-empty unique string list",
                assurance_path,
            )
            return []
        return values

    def validated_profile_list(
        self,
        profile: dict[str, Any],
        key: str,
        profile_id: str,
        assurance_path: Path,
    ) -> list[str]:
        values = string_list(profile.get(key))
        if values is None or len(values) != len(set(values)) or any(not value for value in values):
            self.error(
                "ASSURANCE-PROFILE-007",
                f"profile {profile_id} {key} must be a unique string list",
                assurance_path,
            )
            return []
        return values

    def report(self) -> dict[str, Any]:
        ordered = sorted(
            self.findings,
            key=lambda finding: (
                finding.severity,
                finding.code,
                finding.path or "",
                finding.message,
            ),
        )
        errors = sum(finding.severity == "error" for finding in ordered)
        warnings = sum(finding.severity == "warning" for finding in ordered)
        return {
            "schema_version": 1,
            "status": "ok" if errors == 0 else "failed",
            "root": "." if self.root == ROOT else str(self.root),
            "errors": errors,
            "warnings": warnings,
            "metrics": self.metrics,
            "findings": [asdict(finding) for finding in ordered],
        }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = Validator(args.root.resolve()).run()
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered)
    else:
        print(rendered, end="")
    return 0 if report["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
