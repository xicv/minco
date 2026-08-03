#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fixture_dir="$(mktemp -d)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

prior_revision="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
current_revision="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
database_boundary='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}'
approved_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expires_at="$(jq -nr 'now + 3600 | todateiso8601')"
authority_file="$fixture_dir/multi-release-authority.json"

jq -n \
  --arg approved_at "$approved_at" \
  --arg expires_at "$expires_at" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  --argjson database_boundary "$database_boundary" \
  '{
    schema_version: 1,
    authority_kind: "minco.aws-multi-release-controller-rehearsal.v1",
    run_id: "reviewed-multi-release-run",
    source_revisions: {
      current: $current_revision,
      prior: $prior_revision
    },
    release_sequence: ["prior", "current", "prior"],
    expected_account_id: "123456789012",
    expected_region: "ap-southeast-2",
    expected_role_arn: "arn:aws:iam::123456789012:role/minco-rehearsal",
    aws_profile: "minco-rehearsal",
    environment: "dev",
    database_boundary: $database_boundary,
    resource_allowlist: "bounded-multi-release-smoke-v1",
    cleanup_blast_radius: "cleanup-bounded-multi-release-smoke-v1",
    max_duration_minutes: 60,
    max_spend_usd: 25,
    approved_by: "release-owner",
    approved_at: $approved_at,
    expires_at: $expires_at
  }' >"$authority_file"
approval_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"

scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$authority_file" \
  "$approval_digest" \
  reviewed-multi-release-run \
  "$prior_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1

(
  cd "$fixture_dir"
  "$repo_root/scripts/aws/validate-multi-release-rehearsal-authority.sh" \
    "$authority_file" \
    "$approval_digest" \
    reviewed-multi-release-run \
    "$prior_revision" \
    "$current_revision" \
    ap-southeast-2 \
    minco-rehearsal \
    dev \
    "$database_boundary" \
    bounded-multi-release-smoke-v1 \
    cleanup-bounded-multi-release-smoke-v1
)

# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh
authority_receipt="$fixture_dir/multi-release-authority-receipt.json"
write_multi_release_rehearsal_authority_receipt \
  "$authority_file" \
  "$approval_digest" \
  "$authority_receipt"
jq -e '
  keys == [
    "approval_digest",
    "approved_at",
    "authority_kind",
    "cleanup_blast_radius",
    "database_boundary_mode",
    "environment",
    "expires_at",
    "max_duration_minutes",
    "max_spend_usd",
    "release_sequence",
    "resource_allowlist",
    "run_id",
    "schema_version",
    "source_revisions"
  ]
  and .source_revisions == {
    current: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    prior: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  }
  and .release_sequence == ["prior", "current", "prior"]
  and ([tostring] | all(contains("123456789012") | not))
  and ([tostring] | all(contains("arn:aws") | not))
  and ([tostring] | all(contains("/minco/rehearsal/database-url") | not))
' "$authority_receipt" >/dev/null || {
  echo "multi-release authority receipt omitted its bounds or retained sensitive identity" >&2
  exit 1
}

swapped_error="$fixture_dir/swapped-error.txt"
if scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$authority_file" \
  "$approval_digest" \
  reviewed-multi-release-run \
  "$current_revision" \
  "$prior_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1 2>"$swapped_error"; then
  echo "multi-release authority accepted swapped source revisions" >&2
  exit 1
fi

invalid_sequence="$fixture_dir/invalid-sequence.json"
jq '.release_sequence = ["prior", "current"]' \
  "$authority_file" >"$invalid_sequence"
invalid_sequence_digest="$(shasum -a 256 "$invalid_sequence" | awk '{print $1}')"
if scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$invalid_sequence" \
  "$invalid_sequence_digest" \
  reviewed-multi-release-run \
  "$prior_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1 2>/dev/null; then
  echo "multi-release authority accepted an incomplete release sequence" >&2
  exit 1
fi

same_revision="$fixture_dir/same-revision.json"
jq --arg revision "$current_revision" \
  '.source_revisions.prior = $revision' \
  "$authority_file" >"$same_revision"
same_revision_digest="$(shasum -a 256 "$same_revision" | awk '{print $1}')"
if scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$same_revision" \
  "$same_revision_digest" \
  reviewed-multi-release-run \
  "$current_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1 2>/dev/null; then
  echo "multi-release authority accepted identical prior and current revisions" >&2
  exit 1
fi

root_copy_boundary='{"mode":"run-owned-ssm-copy","source_kind":"process-environment","source_environment_variable":"MINCO_DATABASE_URL","parameter_name":"/minco/smoke/reviewed/database-url"}'
root_copy_authority="$fixture_dir/root-copy-multi-release-authority.json"
jq \
  --argjson boundary "$root_copy_boundary" \
  '.database_boundary = $boundary
   | .resource_allowlist = "bounded-root-multi-release-smoke-v1"
   | .cleanup_blast_radius = "cleanup-bounded-root-multi-release-smoke-v1"' \
  "$authority_file" >"$root_copy_authority"
root_copy_digest="$(shasum -a 256 "$root_copy_authority" | awk '{print $1}')"
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$root_copy_authority" \
  "$root_copy_digest" \
  reviewed-multi-release-run \
  "$prior_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$root_copy_boundary" \
  bounded-root-multi-release-smoke-v1 \
  cleanup-bounded-root-multi-release-smoke-v1

if scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$root_copy_authority" \
  "$root_copy_digest" \
  reviewed-multi-release-run \
  "$prior_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$root_copy_boundary" \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1 2>/dev/null; then
  echo "multi-release authority accepted a database and resource-scope mismatch" >&2
  exit 1
fi

root_temp_boundary='{"mode":"disposable-rds","rds_stack_name":"minco-rds-reviewed","instance_id":"minco-reviewed","parameter_name":"/minco/smoke/reviewed/database-url"}'
root_temp_authority="$fixture_dir/root-temp-multi-release-authority.json"
jq \
  --argjson boundary "$root_temp_boundary" \
  '.database_boundary = $boundary
   | .resource_allowlist = "bounded-root-temp-rds-multi-release-v1"
   | .cleanup_blast_radius = "cleanup-bounded-root-temp-rds-multi-release-v1"' \
  "$authority_file" >"$root_temp_authority"
root_temp_digest="$(shasum -a 256 "$root_temp_authority" | awk '{print $1}')"
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$root_temp_authority" \
  "$root_temp_digest" \
  reviewed-multi-release-run \
  "$prior_revision" \
  "$current_revision" \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$root_temp_boundary" \
  bounded-root-temp-rds-multi-release-v1 \
  cleanup-bounded-root-temp-rds-multi-release-v1

printf 'Multi-release rehearsal authority checks passed.\n'
