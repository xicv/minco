#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"

if (( $# != 10 )); then
  echo "usage: validate-rehearsal-authority.sh AUTHORITY APPROVAL RUN_ID SOURCE REGION PROFILE ENVIRONMENT DATABASE_JSON RESOURCE_SCOPE CLEANUP_SCOPE" >&2
  exit 2
fi

authority_file="$1"
approval_digest="$2"
run_id="$3"
source_revision="$4"
region="$5"
profile="$6"
environment="$7"
database_boundary="$8"
resource_allowlist="$9"
cleanup_blast_radius="${10}"

for command in jq shasum; do
  command -v "$command" >/dev/null || {
    printf '%s is required\n' "$command" >&2
    exit 1
  }
done

[[ -f "$authority_file" && ! -L "$authority_file" ]] || {
  echo "rehearsal authority must be a regular non-symlink file" >&2
  exit 1
}
[[ "$approval_digest" =~ ^[0-9a-f]{64}$ ]] || {
  echo "rehearsal authority approval must be a SHA-256 digest" >&2
  exit 1
}
actual_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"
[[ "$actual_digest" == "$approval_digest" ]] || {
  echo "rehearsal authority approval does not match the exact document digest" >&2
  exit 1
}
jq -e . <<<"$database_boundary" >/dev/null || {
  echo "expected database boundary is not valid JSON" >&2
  exit 1
}

jq -e \
  --arg run_id "$run_id" \
  --arg source_revision "$source_revision" \
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
      "resource_allowlist",
      "run_id",
      "schema_version",
      "source_revision"
    ]
    and .schema_version == 1
    and .authority_kind == "minco.aws-controller-rehearsal.v1"
    and .run_id == $run_id
    and .source_revision == $source_revision
    and (.source_revision | test("^[0-9a-f]{40}([0-9a-f]{24})?$"))
    and .expected_region == $region
    and .aws_profile == $profile
    and .environment == $environment
    and .database_boundary == $database_boundary
    and .resource_allowlist == $resource_allowlist
    and .cleanup_blast_radius == $cleanup_blast_radius
  ' "$authority_file" >/dev/null || {
  echo "rehearsal authority is missing, broader than policy, or does not match this exact run" >&2
  exit 1
}

now_epoch="$(date -u +%s)"
jq -e \
  --arg resource_allowlist "$resource_allowlist" \
  --arg cleanup_blast_radius "$cleanup_blast_radius" \
  --argjson now_epoch "$now_epoch" \
  -f "$repo_root/scripts/aws/lib/validate-rehearsal-authority-common.jq" \
  "$authority_file" >/dev/null || {
  echo "rehearsal authority is missing, stale, broader than policy, or does not match this exact run" >&2
  exit 1
}
