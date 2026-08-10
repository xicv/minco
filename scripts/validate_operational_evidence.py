#!/usr/bin/env python3
"""Validate deterministic performance, provider and capability evidence."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
import re
import stat
import sys
import tomllib
from dataclasses import asdict, dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TRUTH_RELATIVE = Path("verification/repository-truth.toml")
POLICY_RELATIVE = Path("verification/performance-policy.toml")
BASELINE_RELATIVE = Path("verification/1.2-performance-baseline.json")
PROVIDER_RELATIVE = Path("verification/provider-evidence.toml")
CAPABILITY_RELATIVE = Path("verification/aws-capability-candidates.toml")
MANIFEST_RELATIVE = Path("verification/source-manifest.json")
VALIDATION_RECEIPT_RELATIVE = Path("verification/operational-evidence-validation.json")
PROVIDER_RECEIPT_PREFIX = Path("verification/provider-evidence-receipts")
HEX_64 = re.compile(r"[0-9a-f]{64}")
EXACT_REVISION = re.compile(r"[0-9a-f]{40}|[0-9a-f]{64}")
GIT_REVISION = re.compile(r"[0-9a-f]{40}")
STABLE_ID = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")

POLICY_KEYS = {
    "schema", "kind", "candidate_version", "candidate_release_state",
    "baseline_path", "source_manifest_path", "production_slo", "runtime",
    "ingress", "required_runner_scope", "provider_contact", "minimum_samples",
    "metric",
}
METRIC_KEYS = {
    "id", "baseline_path", "candidate_path", "direction",
    "absolute_regression_budget", "relative_regression_budget", "zero_allowed", "unit",
}
PROFILE_KEYS = {
    "id", "support_status", "evidence_state", "source_revision", "observed_at",
    "reviewed_at", "evidence_kind", "provider_contact", "aws_region", "account_scope",
    "dimensions_proven", "cleanup_state", "retained_resources", "max_age_days",
    "evidence_paths", "evidence_sha256", "qualification_receipt_path", "limitations",
}
PROVIDER_RECEIPT_KEYS = {
    "schema_version", "kind", "receipt_digest", "profile_id", "source_tree_sha256",
    "observed_at", "reviewed_at", "evidence_kind", "provider_contact", "aws_region",
    "account_scope", "dimensions_proven", "cleanup_state", "retained_resources",
    "evidence_paths", "evidence_sha256", "limitations",
}
CANDIDATE_KEYS = {
    "id", "name", "support_state", "residual_idle_cost_class", "wake_sources",
    "security_auth_implications", "prerequisites", "implementation_paths",
    "tests_evidence", "blockers", "adoption_trigger", "decision_date", "review_date",
    "upstream_sources",
}
LESSON_KEYS = {
    "id", "project", "adopt", "reject", "zero_idle_effect", "ai_first_effect",
    "evidence_needed", "upstream_sources",
}


@dataclass(frozen=True)
class Finding:
    code: str
    severity: str
    message: str
    path: str | None = None


def reject_constant(value: str) -> None:
    raise ValueError(f"non-finite JSON number is forbidden: {value}")


def strict_json(path: Path) -> Any:
    return json.loads(path.read_text(), parse_constant=reject_constant)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def finite_number(value: Any, *, non_negative: bool = True) -> bool:
    return (
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and math.isfinite(value)
        and (not non_negative or value >= 0)
    )


def string_list(value: Any, *, non_empty: bool = False) -> bool:
    return (
        isinstance(value, list)
        and all(isinstance(item, str) and item for item in value)
        and (not non_empty or bool(value))
    )


def safe_relative(value: Any) -> bool:
    if not isinstance(value, str) or not value:
        return False
    path = Path(value)
    return not path.is_absolute() and ".." not in path.parts


def read_confined_regular(root: Path, value: Any) -> bytes:
    """Read one project file through no-follow directory descriptors."""
    if not safe_relative(value):
        raise ValueError("path must be normalized and project-relative")
    relative = Path(value)
    root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    current_fd = root_fd
    try:
        for component in relative.parts[:-1]:
            next_fd = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=current_fd,
            )
            if current_fd != root_fd:
                os.close(current_fd)
            current_fd = next_fd
        file_fd = os.open(relative.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=current_fd)
        try:
            metadata = os.fstat(file_fd)
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError("path is not a regular file")
            chunks: list[bytes] = []
            while chunk := os.read(file_fd, 1024 * 1024):
                chunks.append(chunk)
            return b"".join(chunks)
        finally:
            os.close(file_fd)
    finally:
        if current_fd != root_fd:
            os.close(current_fd)
        os.close(root_fd)


def parse_iso_date(value: Any) -> date | None:
    if not isinstance(value, str):
        return None
    try:
        parsed = date.fromisoformat(value)
    except ValueError:
        return None
    return parsed if parsed.isoformat() == value else None


def parse_utc(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value.endswith("Z"):
        return None
    try:
        parsed = datetime.fromisoformat(value.removesuffix("Z") + "+00:00")
    except ValueError:
        return None
    return parsed if parsed.tzinfo == timezone.utc else None


def nested_number(document: dict[str, Any], dotted: str) -> float | None:
    value: Any = document
    for part in dotted.split("."):
        if not isinstance(value, dict) or part not in value:
            return None
        value = value[part]
    return float(value) if finite_number(value) else None


class Validator:
    def __init__(
        self,
        root: Path,
        *,
        effective_date_override: date | None = None,
        require_current_provider: bool = False,
    ) -> None:
        self.root = root.resolve()
        self.override = effective_date_override
        self.require_current_provider = require_current_provider
        self.findings: list[Finding] = []
        self.metrics: dict[str, Any] = {}
        self.effective_date: date | None = None
        self.effective_date_source = "unavailable"
        self.source_digest: str | None = None

    def relative(self, path: Path | None) -> str | None:
        if path is None:
            return None
        try:
            return path.relative_to(self.root).as_posix()
        except ValueError:
            return str(path)

    def add(self, code: str, severity: str, message: str, path: Path | None = None) -> None:
        self.findings.append(Finding(code, severity, message, self.relative(path)))

    def error(self, code: str, message: str, path: Path | None = None) -> None:
        self.add(code, "error", message, path)

    def warning(self, code: str, message: str, path: Path | None = None) -> None:
        self.add(code, "warning", message, path)

    def load_toml(self, relative: Path) -> dict[str, Any] | None:
        path = self.root / relative
        try:
            value = tomllib.loads(path.read_text())
        except (OSError, tomllib.TOMLDecodeError) as error:
            self.error("EVIDENCE-DATA-001", f"cannot read {relative}: {error}", path)
            return None
        if not isinstance(value, dict):
            self.error("EVIDENCE-DATA-002", f"{relative} must contain one TOML table", path)
            return None
        return value

    def load_json(self, relative: Path) -> dict[str, Any] | None:
        path = self.root / relative
        try:
            value = strict_json(path)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            self.error("PERF-DATA-001", f"cannot read {relative}: {error}", path)
            return None
        if not isinstance(value, dict):
            self.error("PERF-DATA-002", f"{relative} must contain one JSON object", path)
            return None
        return value

    def validate_manifest(self) -> None:
        path = self.root / MANIFEST_RELATIVE
        try:
            checked = strict_json(path)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            self.error("EVIDENCE-SOURCE-001", f"cannot read source manifest: {error}", path)
            return
        source_script = ROOT / "scripts/source_manifest.py"
        spec = importlib.util.spec_from_file_location("minco_operational_source_manifest", source_script)
        if spec is None or spec.loader is None:
            self.error("EVIDENCE-SOURCE-002", "cannot load canonical source-manifest checker", source_script)
            return
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        try:
            expected = module.render(module.build_report(self.root))
        except (OSError, KeyError, ValueError) as error:
            self.error("EVIDENCE-SOURCE-003", f"cannot compute current source authority: {error}")
            return
        if path.read_text() != expected:
            self.error("EVIDENCE-SOURCE-004", "source manifest is stale for the current tree", path)
            return
        digest = checked.get("source_tree_sha256")
        if not isinstance(digest, str) or HEX_64.fullmatch(digest) is None:
            self.error("EVIDENCE-SOURCE-005", "source manifest digest must be lowercase SHA-256", path)
            return
        self.source_digest = digest

    def validate_truth(self, truth: dict[str, Any], path: Path) -> None:
        value = truth.get("operational_evidence_effective_date")
        reviewed = parse_iso_date(value)
        if reviewed is None:
            self.error(
                "EVIDENCE-DATE-001",
                "repository truth requires operational_evidence_effective_date as YYYY-MM-DD",
                path,
            )
            return
        self.effective_date = self.override or reviewed
        self.effective_date_source = "cli" if self.override else "repository_truth"

    def validate_policy(self, policy: dict[str, Any], truth: dict[str, Any], path: Path) -> None:
        unknown = sorted(set(policy) - POLICY_KEYS)
        if unknown:
            self.error("PERF-POLICY-001", f"unknown performance policy keys: {', '.join(unknown)}", path)
        expected = {
            "schema": 1,
            "kind": "minco.performance-policy.v1",
            "candidate_version": truth.get("workspace_version"),
            "candidate_release_state": "candidate",
            "baseline_path": BASELINE_RELATIVE.as_posix(),
            "source_manifest_path": MANIFEST_RELATIVE.as_posix(),
            "production_slo": False,
            "runtime": "local_native",
            "ingress": "local_tcp",
            "required_runner_scope": "github_hosted",
            "provider_contact": False,
        }
        for key, value in expected.items():
            if policy.get(key) != value:
                self.error("PERF-POLICY-002", f"performance policy {key} must be {value!r}", path)
        minimum = policy.get("minimum_samples")
        if not isinstance(minimum, dict) or any(
            not isinstance(minimum.get(key), int)
            or isinstance(minimum.get(key), bool)
            or minimum[key] <= 0
            for key in ("api_requests", "worker_messages")
        ):
            self.error("PERF-POLICY-003", "minimum_samples must contain positive API and worker counts", path)
        metrics = policy.get("metric")
        if not isinstance(metrics, list) or not metrics:
            self.error("PERF-POLICY-004", "performance policy requires metrics", path)
            return
        ids: set[str] = set()
        for metric in metrics:
            if not isinstance(metric, dict) or set(metric) != METRIC_KEYS:
                self.error("PERF-POLICY-005", "each performance metric must use the closed schema", path)
                continue
            identifier = metric.get("id")
            if not isinstance(identifier, str) or STABLE_ID.fullmatch(identifier.replace("_", "-")) is None or identifier in ids:
                self.error("PERF-POLICY-006", "performance metric IDs must be unique and stable", path)
            else:
                ids.add(identifier)
            if metric.get("direction") not in {"lower", "higher"}:
                self.error("PERF-POLICY-007", f"metric {identifier} has invalid direction", path)
            for key in ("absolute_regression_budget", "relative_regression_budget"):
                if not finite_number(metric.get(key)):
                    self.error("PERF-POLICY-008", f"metric {identifier} has invalid {key}", path)
            if not isinstance(metric.get("zero_allowed"), bool):
                self.error("PERF-POLICY-009", f"metric {identifier} zero_allowed must be boolean", path)

    def validate_latency(self, value: Any, label: str, path: Path) -> None:
        if not isinstance(value, dict):
            self.error("PERF-MEASURE-001", f"{label} latency must be an object", path)
            return
        values = [value.get(key) for key in ("minimum_ms", "p50_ms", "p95_ms", "p99_ms", "maximum_ms")]
        if any(not finite_number(item) for item in values):
            self.error("PERF-MEASURE-002", f"{label} latency values must be finite and non-negative", path)
        elif values != sorted(values):
            self.error("PERF-MEASURE-003", f"{label} latency percentiles must be monotonic", path)

    def validate_measurement(self, value: Any, label: str, minimum: dict[str, Any], path: Path) -> None:
        if not isinstance(value, dict):
            self.error("PERF-MEASURE-004", f"{label} measurement must be an object", path)
            return
        api = value.get("api")
        worker = value.get("worker")
        artifacts = value.get("artifacts")
        if not isinstance(api, dict) or not isinstance(worker, dict) or not isinstance(artifacts, dict):
            self.error("PERF-MEASURE-005", f"{label} requires API, worker and artifact dimensions", path)
            return
        self.validate_latency(api.get("latency"), f"{label} API", path)
        requests = api.get("requests")
        failures = api.get("failures")
        error_rate = api.get("error_rate")
        requests_valid = isinstance(requests, int) and not isinstance(requests, bool) and requests > 0
        if not requests_valid or requests < minimum.get("api_requests", 1):
            self.error("PERF-MEASURE-006", f"{label} API sample count is below policy", path)
        if not isinstance(failures, int) or isinstance(failures, bool) or failures < 0 or failures > (requests if isinstance(requests, int) else -1):
            self.error("PERF-MEASURE-007", f"{label} API failure count is invalid", path)
        elif requests_valid and (
            not finite_number(error_rate)
            or abs(error_rate - failures / requests) > 1e-12
        ):
            self.error("PERF-MEASURE-008", f"{label} API error rate contradicts counts", path)
        messages = worker.get("messages")
        if not isinstance(messages, int) or isinstance(messages, bool) or messages < minimum.get("worker_messages", 1):
            self.error("PERF-MEASURE-009", f"{label} worker sample count is below policy", path)
        worker_failures = worker.get("failures")
        worker_error_rate = worker.get("error_rate")
        if (
            not isinstance(messages, int)
            or isinstance(messages, bool)
            or messages <= 0
            or not isinstance(worker_failures, int)
            or isinstance(worker_failures, bool)
            or worker_failures < 0
            or worker_failures > (messages if isinstance(messages, int) else -1)
        ):
            self.error("PERF-MEASURE-011", f"{label} worker failure count is invalid", path)
        elif not finite_number(worker_error_rate) or abs(worker_error_rate - worker_failures / messages) > 1e-12:
            self.error("PERF-MEASURE-012", f"{label} worker error rate contradicts counts", path)
        for section in (api, worker, artifacts):
            for key, item in section.items():
                if isinstance(item, (int, float)) and not finite_number(item):
                    self.error("PERF-MEASURE-010", f"{label} {key} must be finite and non-negative", path)

    def validate_performance(self, policy: dict[str, Any], baseline: dict[str, Any], path: Path) -> None:
        expected_common = {
            "schema_version": 1,
            "kind": "minco.performance-baseline.v1",
            "candidate_version": policy.get("candidate_version"),
            "production_slo": False,
            "provider_contact": False,
        }
        for key, expected in expected_common.items():
            if baseline.get(key) != expected:
                self.error("PERF-BASELINE-001", f"performance baseline {key} must be {expected!r}", path)
        source = baseline.get("source_tree_sha256")
        if not isinstance(source, str) or HEX_64.fullmatch(source) is None:
            self.error("PERF-BASELINE-002", "performance baseline requires an exact source SHA-256", path)
        elif self.source_digest is not None and source != self.source_digest:
            self.error("PERF-BASELINE-003", "performance baseline source does not match the current verified tree", path)
        status = baseline.get("status")
        self.metrics["performance_status"] = status
        if status == "NOT RUN":
            allowed = set(expected_common) | {
                "status", "reason", "limitations", "source_tree_sha256",
            }
            if set(baseline) != allowed:
                self.error("PERF-BASELINE-004", "NOT RUN baseline must not contain measurement fields", path)
            if not isinstance(baseline.get("reason"), str) or not baseline["reason"]:
                self.error("PERF-BASELINE-005", "NOT RUN baseline requires a reason", path)
            if not string_list(baseline.get("limitations"), non_empty=True):
                self.error("PERF-BASELINE-006", "NOT RUN baseline requires limitations", path)
            self.warning("PERF-BASELINE-007", "exact-tree hosted Linux performance evidence is NOT RUN", path)
            return
        if status != "PASS":
            self.error("PERF-BASELINE-008", "performance baseline status must be PASS or NOT RUN", path)
            return
        required = {
            "source_revision", "topology", "runner", "environment", "classification",
            "baseline", "candidate", "limitations",
        }
        if not required.issubset(baseline):
            self.error("PERF-BASELINE-009", "PASS baseline omits required provenance or measurements", path)
            return
        if baseline.get("topology") != {"runtime": policy.get("runtime"), "ingress": policy.get("ingress")}:
            self.error("PERF-BASELINE-010", "performance topology differs from policy", path)
        runner = baseline.get("runner")
        runner_keys = {
            "scope", "repository", "source_sha", "run_id", "run_attempt",
            "runner_os", "runner_arch", "runner_image", "source_tree_sha256",
        }
        if (
            not isinstance(runner, dict)
            or set(runner) != runner_keys
            or runner.get("scope") != policy.get("required_runner_scope")
            or runner.get("repository") != "xicv/minco"
            or GIT_REVISION.fullmatch(str(runner.get("source_sha"))) is None
            or runner.get("source_sha") != baseline.get("source_revision")
            or not str(runner.get("run_id", "")).isdigit()
            or not str(runner.get("run_attempt", "")).isdigit()
            or str(runner.get("runner_os", "")).lower() != "linux"
            or not isinstance(runner.get("runner_arch"), str)
            or not runner["runner_arch"]
            or not isinstance(runner.get("runner_image"), str)
            or not runner["runner_image"]
        ):
            self.error("PERF-BASELINE-011", "PASS baseline requires the reviewed hosted Linux runner scope", path)
        elif (
            runner.get("source_tree_sha256") != source
            or runner.get("source_tree_sha256") != self.source_digest
        ):
            self.error(
                "PERF-BASELINE-016",
                "hosted runner provenance does not bind the current verified source tree",
                path,
            )
        environment = baseline.get("environment")
        dimensions = environment.get("dimensions") if isinstance(environment, dict) else None
        dimension_keys = {"os", "os_release", "architecture", "python", "github_actions"}
        if (
            not isinstance(environment, dict)
            or set(environment) != {"dimensions", "fingerprint_sha256"}
            or not isinstance(dimensions, dict)
            or set(dimensions) != dimension_keys
            or dimensions.get("os") != "linux"
            or dimensions.get("github_actions") is not True
            or not all(dimensions.get(key) for key in ("os_release", "architecture", "python"))
        ):
            self.error("PERF-BASELINE-012", "PASS baseline requires environment dimensions", path)
        else:
            fingerprint = sha256_bytes(json.dumps(dimensions, separators=(",", ":"), sort_keys=True).encode())
            if environment.get("fingerprint_sha256") != fingerprint:
                self.error("PERF-BASELINE-013", "environment fingerprint does not match its dimensions", path)
        classification = baseline.get("classification")
        if (
            not isinstance(classification, dict)
            or set(classification) != {"warm", "cold_start_measured"}
            or classification.get("warm") is not True
            or classification.get("cold_start_measured") is not False
        ):
            self.error("PERF-BASELINE-014", "PASS baseline requires explicit warm/cold classification", path)
        if GIT_REVISION.fullmatch(str(baseline.get("source_revision"))) is None:
            self.error("PERF-BASELINE-015", "PASS baseline requires an exact Git source revision", path)
        minimum_value = policy.get("minimum_samples")
        minimum = minimum_value if isinstance(minimum_value, dict) else {}
        reference = baseline.get("baseline")
        candidate = baseline.get("candidate")
        self.validate_measurement(reference, "baseline", minimum, path)
        self.validate_measurement(candidate, "candidate", minimum, path)
        if isinstance(reference, dict) and isinstance(candidate, dict):
            if reference.get("environment_fingerprint_sha256") != candidate.get("environment_fingerprint_sha256"):
                self.error("PERF-COMPARE-001", "baseline and candidate environments are not comparable", path)
            for metric in policy.get("metric", []):
                if not isinstance(metric, dict):
                    continue
                before = nested_number(reference, str(metric.get("baseline_path", "")))
                after = nested_number(candidate, str(metric.get("candidate_path", "")))
                if before is None or after is None:
                    self.error("PERF-COMPARE-002", f"metric {metric.get('id')} is missing or non-finite", path)
                    continue
                if before == 0:
                    if not metric.get("zero_allowed"):
                        self.error("PERF-COMPARE-003", f"metric {metric.get('id')} has a forbidden zero baseline", path)
                    if after > 0:
                        self.error("PERF-COMPARE-004", f"metric {metric.get('id')} regressed unboundedly from zero", path)
                    continue
                direction = metric.get("direction")
                absolute = (after - before) if direction == "lower" else (before - after)
                relative = absolute / before
                if absolute > metric.get("absolute_regression_budget", 0) or relative > metric.get("relative_regression_budget", 0):
                    self.error("PERF-COMPARE-005", f"metric {metric.get('id')} exceeds its regression budget", path)

    def validate_provider(self, document: dict[str, Any], path: Path) -> None:
        if set(document) != {"schema", "kind", "effective_date_authority", "profile"}:
            self.error("EVIDENCE-PROVIDER-001", "provider ledger uses an unknown or incomplete top-level schema", path)
        if document.get("schema") != 1 or document.get("kind") != "minco.provider-evidence.v1":
            self.error("EVIDENCE-PROVIDER-002", "provider ledger schema/kind is invalid", path)
        profiles = document.get("profile")
        if not isinstance(profiles, list) or not profiles:
            self.error("EVIDENCE-PROVIDER-003", "provider ledger requires profiles", path)
            return
        ids: set[str] = set()
        current_count = 0
        for profile in profiles:
            if not isinstance(profile, dict) or set(profile) != PROFILE_KEYS:
                self.error("EVIDENCE-PROVIDER-004", "provider profile must use the closed schema", path)
                continue
            identifier = profile.get("id")
            if not isinstance(identifier, str) or STABLE_ID.fullmatch(identifier) is None or identifier in ids:
                self.error("EVIDENCE-PROVIDER-005", "provider profile IDs must be unique and stable", path)
            else:
                ids.add(identifier)
            state = profile.get("evidence_state")
            if state not in {"current", "stale", "not_run"}:
                self.error("EVIDENCE-PROVIDER-006", f"provider profile {identifier} has invalid state", path)
                continue
            if profile.get("support_status") not in {"stable", "bounded", "historical", "declared", "deferred"}:
                self.error("EVIDENCE-PROVIDER-007", f"provider profile {identifier} has invalid support status", path)
            max_age = profile.get("max_age_days")
            max_age_valid = (
                isinstance(max_age, int)
                and not isinstance(max_age, bool)
                and max_age > 0
            )
            if not max_age_valid:
                self.error("EVIDENCE-PROVIDER-008", f"provider profile {identifier} has invalid maximum age", path)
            if not string_list(profile.get("limitations"), non_empty=True):
                self.error("EVIDENCE-PROVIDER-009", f"provider profile {identifier} requires limitations", path)
            paths = profile.get("evidence_paths")
            digests = profile.get("evidence_sha256")
            if not string_list(paths) or not string_list(digests) or len(paths) != len(digests):
                self.error("EVIDENCE-PROVIDER-010", f"provider profile {identifier} has invalid artifact bindings", path)
                paths, digests = [], []
            for relative, expected in zip(paths, digests, strict=True):
                try:
                    evidence_bytes = read_confined_regular(self.root, relative)
                except (OSError, ValueError):
                    self.error("EVIDENCE-PROVIDER-011", f"provider evidence path is unsafe or missing: {relative}", path)
                    continue
                if HEX_64.fullmatch(expected) is None or sha256_bytes(evidence_bytes) != expected:
                    self.error("EVIDENCE-PROVIDER-012", f"provider evidence digest mismatch: {relative}", path)
            reviewed = parse_utc(profile.get("reviewed_at"))
            if reviewed is None or self.effective_date is None or reviewed.date() > self.effective_date:
                self.error("EVIDENCE-PROVIDER-013", f"provider profile {identifier} has invalid review time", path)
            if state == "not_run":
                if profile.get("provider_contact") is not False or profile.get("observed_at") != "":
                    self.error("EVIDENCE-PROVIDER-014", f"NOT RUN profile {identifier} cannot claim provider contact", path)
                if profile.get("dimensions_proven") != [] or paths or profile.get("retained_resources") != []:
                    self.error("EVIDENCE-PROVIDER-015", f"NOT RUN profile {identifier} cannot claim proof or resources", path)
                if profile.get("cleanup_state") != "not_required_no_contact" or profile.get("evidence_kind") != "none":
                    self.error("EVIDENCE-PROVIDER-016", f"NOT RUN profile {identifier} has contradictory cleanup/evidence", path)
                if profile.get("qualification_receipt_path") != "":
                    self.error("EVIDENCE-PROVIDER-022", f"NOT RUN profile {identifier} cannot name a qualification receipt", path)
                continue
            observed = parse_utc(profile.get("observed_at"))
            if observed is None or reviewed is None or observed > reviewed:
                self.error("EVIDENCE-PROVIDER-017", f"provider profile {identifier} has invalid observed/review order", path)
                continue
            source_revision = profile.get("source_revision")
            direct_revision = isinstance(source_revision, str) and EXACT_REVISION.fullmatch(source_revision) is not None
            receipt_bound = state == "current" and source_revision == "receipt_bound"
            if profile.get("provider_contact") is not True or not (direct_revision or receipt_bound):
                self.error("EVIDENCE-PROVIDER-018", f"provider profile {identifier} lacks exact contacted-source proof", path)
            if (
                profile.get("evidence_kind") in {None, "", "none"}
                or profile.get("aws_region") in {None, "", "not_applicable"}
                or profile.get("account_scope") in {None, "", "none"}
                or not string_list(profile.get("dimensions_proven"), non_empty=True)
                or not paths
            ):
                self.error(
                    "EVIDENCE-PROVIDER-023",
                    f"provider profile {identifier} lacks meaningful kind, scope, dimensions or artifacts",
                    path,
                )
            cleanup_ok = profile.get("cleanup_state") == "verified_absent" and profile.get("retained_resources") == []
            if not cleanup_ok:
                self.error("EVIDENCE-PROVIDER-019", f"provider profile {identifier} lacks complete cleanup proof", path)
            receipt_current = False
            receipt_path = profile.get("qualification_receipt_path")
            if state == "current":
                receipt_current = self.validate_provider_receipt(profile, path)
            elif receipt_path != "":
                self.error("EVIDENCE-PROVIDER-022", f"stale profile {identifier} cannot name a current qualification receipt", path)
            source_current = receipt_current if state == "current" else source_revision == self.source_digest
            age_current = (
                self.effective_date is not None
                and max_age_valid
                and (self.effective_date - observed.date()).days <= max_age
            )
            computed = "current" if source_current and age_current and cleanup_ok else "stale"
            if state != computed:
                self.error("EVIDENCE-PROVIDER-020", f"provider profile {identifier} says {state} but computes as {computed}", path)
            if computed == "current" and profile.get("support_status") in {"stable", "bounded"}:
                current_count += 1
        self.metrics["current_provider_profiles"] = current_count
        if current_count == 0:
            severity = "error" if self.require_current_provider else "warning"
            self.add(
                "EVIDENCE-PROVIDER-021",
                severity,
                "no current exact-source live-provider evidence qualifies this candidate",
                path,
            )

    def validate_provider_receipt(self, profile: dict[str, Any], ledger_path: Path) -> bool:
        identifier = profile.get("id")
        relative = profile.get("qualification_receipt_path")
        if (
            not safe_relative(relative)
            or not Path(relative).is_relative_to(PROVIDER_RECEIPT_PREFIX)
            or Path(relative).suffix != ".json"
        ):
            self.error("EVIDENCE-PROVIDER-024", f"current profile {identifier} requires a confined receipt path", ledger_path)
            return False
        try:
            receipt_bytes = read_confined_regular(self.root, relative)
        except (OSError, ValueError):
            self.error("EVIDENCE-PROVIDER-025", f"provider qualification receipt is unsafe or missing: {relative}", ledger_path)
            return False
        try:
            receipt = json.loads(receipt_bytes, parse_constant=reject_constant)
        except (json.JSONDecodeError, ValueError) as error:
            self.error("EVIDENCE-PROVIDER-026", f"cannot read provider qualification receipt: {error}", ledger_path)
            return False
        if not isinstance(receipt, dict) or set(receipt) != PROVIDER_RECEIPT_KEYS:
            self.error("EVIDENCE-PROVIDER-027", "provider qualification receipt uses an open or incomplete schema", ledger_path)
            return False
        payload = dict(receipt)
        receipt_digest = payload.pop("receipt_digest", None)
        sealed = sha256_bytes(json.dumps(payload, allow_nan=False, separators=(",", ":"), sort_keys=True).encode())
        expected = {
            "schema_version": 1,
            "kind": "minco.provider-evidence-receipt.v1",
            "profile_id": identifier,
            "source_tree_sha256": self.source_digest,
            "observed_at": profile.get("observed_at"),
            "reviewed_at": profile.get("reviewed_at"),
            "evidence_kind": profile.get("evidence_kind"),
            "provider_contact": True,
            "aws_region": profile.get("aws_region"),
            "account_scope": profile.get("account_scope"),
            "dimensions_proven": profile.get("dimensions_proven"),
            "cleanup_state": "verified_absent",
            "retained_resources": [],
            "evidence_paths": profile.get("evidence_paths"),
            "evidence_sha256": profile.get("evidence_sha256"),
            "limitations": profile.get("limitations"),
        }
        if receipt_digest != sealed or any(receipt.get(key) != value for key, value in expected.items()):
            self.error("EVIDENCE-PROVIDER-028", "provider qualification receipt digest or ledger binding is invalid", ledger_path)
            return False
        return True

    def validate_capabilities(self, document: dict[str, Any], path: Path) -> None:
        if set(document) != {"schema", "kind", "reviewed_on", "candidate", "lesson"}:
            self.error("EVIDENCE-CAPABILITY-001", "capability ledger uses an unknown or incomplete schema", path)
        if document.get("schema") != 1 or document.get("kind") != "minco.aws-capability-candidates.v1":
            self.error("EVIDENCE-CAPABILITY-002", "capability ledger schema/kind is invalid", path)
        reviewed = parse_iso_date(document.get("reviewed_on"))
        if reviewed is None or self.effective_date is None or reviewed > self.effective_date:
            self.error("EVIDENCE-CAPABILITY-003", "capability ledger review date is invalid", path)
        candidates = document.get("candidate")
        lessons = document.get("lesson")
        if not isinstance(candidates, list) or not candidates:
            self.error("EVIDENCE-CAPABILITY-004", "capability ledger requires candidates", path)
            candidates = []
        if not isinstance(lessons, list) or not lessons:
            self.error("EVIDENCE-CAPABILITY-005", "capability ledger requires project lessons", path)
            lessons = []
        ids: set[str] = set()
        for candidate in candidates:
            if not isinstance(candidate, dict) or set(candidate) != CANDIDATE_KEYS:
                self.error("EVIDENCE-CAPABILITY-006", "candidate must use the closed schema", path)
                continue
            identifier = candidate.get("id")
            if not isinstance(identifier, str) or STABLE_ID.fullmatch(identifier) is None or identifier in ids:
                self.error("EVIDENCE-CAPABILITY-007", "candidate IDs must be unique and stable", path)
            else:
                ids.add(identifier)
            state = candidate.get("support_state")
            if state not in {"supported", "declared", "research", "deferred", "rejected"}:
                self.error("EVIDENCE-CAPABILITY-008", f"candidate {identifier} has invalid support state", path)
            for key in ("wake_sources", "security_auth_implications", "prerequisites", "implementation_paths", "tests_evidence", "blockers", "upstream_sources"):
                if not string_list(candidate.get(key)):
                    self.error("EVIDENCE-CAPABILITY-009", f"candidate {identifier} has invalid {key}", path)
            for key in ("name", "residual_idle_cost_class", "adoption_trigger"):
                if not isinstance(candidate.get(key), str) or not candidate[key]:
                    self.error("EVIDENCE-CAPABILITY-010", f"candidate {identifier} requires {key}", path)
            decision = parse_iso_date(candidate.get("decision_date"))
            review = parse_iso_date(candidate.get("review_date"))
            if decision is None or review is None or decision > review or self.effective_date is None or review > self.effective_date:
                self.error("EVIDENCE-CAPABILITY-011", f"candidate {identifier} has invalid decision/review dates", path)
            if state == "supported" and (
                not candidate.get("implementation_paths")
                or not candidate.get("tests_evidence")
                or self.metrics.get("current_provider_profiles", 0) == 0
            ):
                self.error("EVIDENCE-CAPABILITY-012", f"supported candidate {identifier} lacks implementation, tests or live evidence", path)
            upstream_sources = candidate.get("upstream_sources")
            if string_list(upstream_sources) and any(
                not source.startswith("https://") for source in upstream_sources
            ):
                self.error("EVIDENCE-CAPABILITY-013", f"candidate {identifier} has a non-HTTPS upstream source", path)
        lesson_ids: set[str] = set()
        for lesson in lessons:
            if not isinstance(lesson, dict) or set(lesson) != LESSON_KEYS:
                self.error("EVIDENCE-CAPABILITY-014", "project lesson must use the closed schema", path)
                continue
            identifier = lesson.get("id")
            if not isinstance(identifier, str) or STABLE_ID.fullmatch(identifier) is None or identifier in lesson_ids:
                self.error("EVIDENCE-CAPABILITY-015", "lesson IDs must be unique and stable", path)
            else:
                lesson_ids.add(identifier)
            for key in ("project", "adopt", "reject", "zero_idle_effect", "ai_first_effect", "evidence_needed"):
                if not isinstance(lesson.get(key), str) or not lesson[key]:
                    self.error("EVIDENCE-CAPABILITY-016", f"lesson {identifier} requires {key}", path)
            if not string_list(lesson.get("upstream_sources"), non_empty=True):
                self.error("EVIDENCE-CAPABILITY-017", f"lesson {identifier} requires upstream sources", path)
        self.metrics["capability_candidates"] = len(candidates)
        self.metrics["project_lessons"] = len(lessons)

    def run(self) -> dict[str, Any]:
        try:
            truth = self.load_toml(TRUTH_RELATIVE)
            policy = self.load_toml(POLICY_RELATIVE)
            provider = self.load_toml(PROVIDER_RELATIVE)
            capabilities = self.load_toml(CAPABILITY_RELATIVE)
            baseline = self.load_json(BASELINE_RELATIVE)
            if truth is not None:
                self.validate_truth(truth, self.root / TRUTH_RELATIVE)
            self.validate_manifest()
            if policy is not None and truth is not None:
                self.validate_policy(policy, truth, self.root / POLICY_RELATIVE)
            if policy is not None and baseline is not None:
                self.validate_performance(policy, baseline, self.root / BASELINE_RELATIVE)
            if provider is not None:
                self.validate_provider(provider, self.root / PROVIDER_RELATIVE)
            if capabilities is not None:
                self.validate_capabilities(capabilities, self.root / CAPABILITY_RELATIVE)
        except Exception:
            self.error(
                "EVIDENCE-VALIDATOR-001",
                "operational evidence validation stopped on an unexpected malformed record",
            )
        return self.report()

    def report(self) -> dict[str, Any]:
        findings = sorted(self.findings, key=lambda item: (item.code, item.severity, item.path or "", item.message))
        errors = sum(item.severity == "error" for item in findings)
        warnings = sum(item.severity == "warning" for item in findings)
        bound_paths = [
            MANIFEST_RELATIVE,
            POLICY_RELATIVE,
            BASELINE_RELATIVE,
            PROVIDER_RELATIVE,
            CAPABILITY_RELATIVE,
            TRUTH_RELATIVE,
        ]
        receipt_root = self.root / PROVIDER_RECEIPT_PREFIX
        if receipt_root.is_dir() and not receipt_root.is_symlink():
            bound_paths.extend(
                path.relative_to(self.root)
                for path in sorted(receipt_root.glob("*.json"))
                if path.is_file() and not path.is_symlink()
            )
        inputs = {
            relative.as_posix(): sha256_bytes((self.root / relative).read_bytes())
            for relative in bound_paths
            if (self.root / relative).is_file()
        }
        report = {
            "schema_version": 1,
            "kind": "minco.operational-evidence-validation.v1",
            "status": "PASS" if errors == 0 else "FAIL",
            "source_tree_sha256": self.source_digest,
            "inputs": dict(sorted(inputs.items())),
            "effective_date": self.effective_date.isoformat() if self.effective_date else None,
            "effective_date_source": self.effective_date_source,
            "counts": {"errors": errors, "warnings": warnings},
            "metrics": dict(sorted(self.metrics.items())),
            "findings": [asdict(item) for item in findings],
        }
        report["receipt_digest"] = sha256_bytes(
            json.dumps(report, allow_nan=False, separators=(",", ":"), sort_keys=True).encode()
        )
        return report


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    parser.add_argument("--effective-date", type=str)
    parser.add_argument("--require-current-provider", action="store_true")
    outputs = parser.add_mutually_exclusive_group()
    outputs.add_argument("--output", type=Path)
    outputs.add_argument("--check-output", type=Path)
    return parser.parse_args()


def confined_output(root: Path, requested: Path) -> Path | None:
    output = requested if requested.is_absolute() else root / requested
    verification = (root / "verification").resolve()
    resolved = output.resolve(strict=False)
    if output.is_symlink() or not resolved.is_relative_to(verification):
        return None
    return resolved


def main() -> int:
    args = parse_arguments()
    override = None
    if args.effective_date is not None:
        override = parse_iso_date(args.effective_date)
        if override is None:
            print("--effective-date must be an exact YYYY-MM-DD date", file=sys.stderr)
            return 2
    report = Validator(
        args.root,
        effective_date_override=override,
        require_current_provider=args.require_current_provider,
    ).run()
    rendered = json.dumps(report, allow_nan=False, indent=2, sort_keys=True) + "\n"
    requested = args.output or args.check_output
    if requested is not None:
        resolved = confined_output(args.root, requested)
        if resolved is None:
            print("evidence output must be a non-symlink path under verification/", file=sys.stderr)
            return 2
        if args.check_output is not None:
            try:
                current = resolved.read_text()
            except OSError as error:
                print(f"cannot read checked evidence output: {error}", file=sys.stderr)
                return 1
            if current != rendered:
                print("checked operational-evidence receipt is stale", file=sys.stderr)
                return 1
        else:
            resolved.parent.mkdir(parents=True, exist_ok=True)
            resolved.write_text(rendered)
    print(rendered, end="")
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
