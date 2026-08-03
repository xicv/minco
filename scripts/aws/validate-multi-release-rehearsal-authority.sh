#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"

if (( $# != 11 )); then
  echo "usage: validate-multi-release-rehearsal-authority.sh AUTHORITY APPROVAL RUN_ID PRIOR_SOURCE CURRENT_SOURCE REGION PROFILE ENVIRONMENT DATABASE_JSON RESOURCE_SCOPE CLEANUP_SCOPE" >&2
  exit 2
fi

authority_file="$1"
approval_digest="$2"
run_id="$3"
prior_source_revision="$4"
current_source_revision="$5"
region="$6"
profile="$7"
environment="$8"
database_boundary="$9"
resource_allowlist="${10}"
cleanup_blast_radius="${11}"

for command in jq shasum; do
  command -v "$command" >/dev/null || {
    printf '%s is required\n' "$command" >&2
    exit 1
  }
done

[[ -f "$authority_file" && ! -L "$authority_file" ]] || {
  echo "multi-release rehearsal authority must be a regular non-symlink file" >&2
  exit 1
}
[[ "$approval_digest" =~ ^[0-9a-f]{64}$ ]] || {
  echo "multi-release rehearsal authority approval must be a SHA-256 digest" >&2
  exit 1
}
actual_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"
[[ "$actual_digest" == "$approval_digest" ]] || {
  echo "multi-release rehearsal authority approval does not match the exact document digest" >&2
  exit 1
}
jq -e . <<<"$database_boundary" >/dev/null || {
  echo "expected database boundary is not valid JSON" >&2
  exit 1
}

jq -e \
  --arg run_id "$run_id" \
  --arg prior_source_revision "$prior_source_revision" \
  --arg current_source_revision "$current_source_revision" \
  --arg region "$region" \
  --arg profile "$profile" \
  --arg environment "$environment" \
  --argjson database_boundary "$database_boundary" \
  --arg resource_allowlist "$resource_allowlist" \
  --arg cleanup_blast_radius "$cleanup_blast_radius" \
  '
    keys == [
      "approved_at",
      "approved_by",
      "authority_kind",
      "aws_profile",
      "cleanup_blast_radius",
      "database_boundary",
      "environment",
      "expected_account_id",
      "expected_region",
      "expected_role_arn",
      "expires_at",
      "max_duration_minutes",
      "max_spend_usd",
      "release_sequence",
      "resource_allowlist",
      "run_id",
      "schema_version",
      "source_revisions"
    ]
    and .schema_version == 1
    and .authority_kind == "minco.aws-multi-release-controller-rehearsal.v1"
    and .run_id == $run_id
    and (.source_revisions | keys) == ["current", "prior"]
    and .source_revisions.prior == $prior_source_revision
    and .source_revisions.current == $current_source_revision
    and .source_revisions.prior != .source_revisions.current
    and (.source_revisions.prior | test("^[0-9a-f]{40}([0-9a-f]{24})?$"))
    and (.source_revisions.current | test("^[0-9a-f]{40}([0-9a-f]{24})?$"))
    and .release_sequence == ["prior", "current", "prior"]
    and .expected_region == $region
    and .aws_profile == $profile
    and .environment == $environment
    and .database_boundary == $database_boundary
    and .resource_allowlist == $resource_allowlist
    and .cleanup_blast_radius == $cleanup_blast_radius
    and (
      (
        $resource_allowlist == "bounded-multi-release-smoke-v1"
        and $cleanup_blast_radius == "cleanup-bounded-multi-release-smoke-v1"
      )
      or (
        $resource_allowlist == "bounded-root-multi-release-smoke-v1"
        and $cleanup_blast_radius == "cleanup-bounded-root-multi-release-smoke-v1"
      )
      or (
        $resource_allowlist == "bounded-root-temp-rds-multi-release-v1"
        and $cleanup_blast_radius == "cleanup-bounded-root-temp-rds-multi-release-v1"
      )
    )
  ' "$authority_file" >/dev/null || {
  echo "multi-release rehearsal authority is missing, broader than policy, or does not match this exact ordered run" >&2
  exit 1
}

case "$resource_allowlist:$cleanup_blast_radius" in
  bounded-multi-release-smoke-v1:cleanup-bounded-multi-release-smoke-v1)
    single_resource_allowlist="bounded-direct-smoke-v1"
    single_cleanup_blast_radius="cleanup-bounded-direct-smoke-v1"
    ;;
  bounded-root-multi-release-smoke-v1:cleanup-bounded-root-multi-release-smoke-v1)
    single_resource_allowlist="bounded-root-bootstrap-v1"
    single_cleanup_blast_radius="cleanup-bounded-root-bootstrap-v1"
    ;;
  bounded-root-temp-rds-multi-release-v1:cleanup-bounded-root-temp-rds-multi-release-v1)
    single_resource_allowlist="bounded-root-temp-rds-v1"
    single_cleanup_blast_radius="cleanup-bounded-root-temp-rds-v1"
    ;;
  *)
    echo "multi-release rehearsal resource and cleanup scopes are unsupported" >&2
    exit 1
    ;;
esac

now_epoch="$(date -u +%s)"
jq -e \
  --arg resource_allowlist "$single_resource_allowlist" \
  --arg cleanup_blast_radius "$single_cleanup_blast_radius" \
  --argjson now_epoch "$now_epoch" \
  -f "$repo_root/scripts/aws/lib/validate-rehearsal-authority-common.jq" \
  "$authority_file" >/dev/null || {
  echo "multi-release rehearsal authority has an invalid account, time, spend, or database boundary" >&2
  exit 1
}
