#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fixture_dir="$(mktemp -d)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

authority_file="$fixture_dir/authority.json"
database_boundary='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}'
approved_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expires_at="$(jq -nr 'now + 3600 | todateiso8601')"

jq -n \
  --arg approved_at "$approved_at" \
  --arg expires_at "$expires_at" \
  --argjson database_boundary "$database_boundary" \
  '{
    schema_version: 1,
    authority_kind: "minco.aws-controller-rehearsal.v1",
    run_id: "reviewed-run",
    source_revision: "0123456789abcdef0123456789abcdef01234567",
    expected_account_id: "123456789012",
    expected_region: "ap-southeast-2",
    expected_role_arn: "arn:aws:iam::123456789012:role/minco-rehearsal",
    aws_profile: "minco-rehearsal",
    environment: "dev",
    database_boundary: $database_boundary,
    resource_allowlist: "bounded-direct-smoke-v1",
    cleanup_blast_radius: "cleanup-bounded-direct-smoke-v1",
    max_duration_minutes: 60,
    max_spend_usd: 25,
    approved_by: "release-owner",
    approved_at: $approved_at,
    expires_at: $expires_at
  }' >"$authority_file"
approval_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"

scripts/aws/validate-rehearsal-authority.sh \
  "$authority_file" \
  "$approval_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-direct-smoke-v1 \
  cleanup-bounded-direct-smoke-v1

(
  cd "$fixture_dir"
  "$repo_root/scripts/aws/validate-rehearsal-authority.sh" \
    "$authority_file" \
    "$approval_digest" \
    reviewed-run \
    0123456789abcdef0123456789abcdef01234567 \
    ap-southeast-2 \
    minco-rehearsal \
    dev \
    "$database_boundary" \
    bounded-direct-smoke-v1 \
    cleanup-bounded-direct-smoke-v1
)

# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh
authority_receipt="$fixture_dir/authority-receipt.json"
write_rehearsal_authority_receipt \
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
    "resource_allowlist",
    "run_id",
    "schema_version",
    "source_revision"
  ]
  and ([tostring] | all(
    contains("123456789012") | not
  ))
  and ([tostring] | all(
    contains("arn:aws") | not
  ))
  and ([tostring] | all(
    contains("/minco/rehearsal/database-url") | not
  ))
' "$authority_receipt" >/dev/null || {
  echo "authority receipt omitted its bounds or retained sensitive identity" >&2
  exit 1
}

unsupported_authority="$fixture_dir/unsupported-authority.json"
jq '.resource_allowlist = "operator-defined-broad-scope"' \
  "$authority_file" >"$unsupported_authority"
unsupported_digest="$(shasum -a 256 "$unsupported_authority" | awk '{print $1}')"
if scripts/aws/validate-rehearsal-authority.sh \
  "$unsupported_authority" \
  "$unsupported_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  operator-defined-broad-scope \
  cleanup-bounded-direct-smoke-v1 2>/dev/null; then
  echo "authority validator accepted an unsupported resource allowlist" >&2
  exit 1
fi

reversed_window_authority="$fixture_dir/reversed-window-authority.json"
jq \
  --arg approved_at "$(jq -nr 'now + 240 | todateiso8601')" \
  --arg expires_at "$(jq -nr 'now + 60 | todateiso8601')" \
  '.approved_at = $approved_at | .expires_at = $expires_at' \
  "$authority_file" >"$reversed_window_authority"
reversed_window_digest="$(shasum -a 256 "$reversed_window_authority" | awk '{print $1}')"
if scripts/aws/validate-rehearsal-authority.sh \
  "$reversed_window_authority" \
  "$reversed_window_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$database_boundary" \
  bounded-direct-smoke-v1 \
  cleanup-bounded-direct-smoke-v1 2>/dev/null; then
  echo "authority validator accepted a reversed approval window" >&2
  exit 1
fi

root_copy_boundary='{"mode":"run-owned-ssm-copy","source_kind":"process-environment","source_environment_variable":"MINCO_DATABASE_URL","parameter_name":"/minco/smoke/reviewed/database-url"}'
root_copy_authority="$fixture_dir/root-copy-authority.json"
jq \
  --argjson boundary "$root_copy_boundary" \
  '.database_boundary = $boundary
   | .resource_allowlist = "bounded-root-bootstrap-v1"
   | .cleanup_blast_radius = "cleanup-bounded-root-bootstrap-v1"' \
  "$authority_file" >"$root_copy_authority"
root_copy_digest="$(shasum -a 256 "$root_copy_authority" | awk '{print $1}')"
scripts/aws/validate-rehearsal-authority.sh \
  "$root_copy_authority" \
  "$root_copy_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$root_copy_boundary" \
  bounded-root-bootstrap-v1 \
  cleanup-bounded-root-bootstrap-v1

unsupported_copy_boundary='{"mode":"run-owned-ssm-copy","source_kind":"unbounded-provider","parameter_name":"/minco/smoke/reviewed/database-url"}'
unsupported_copy_authority="$fixture_dir/unsupported-copy-authority.json"
jq --argjson boundary "$unsupported_copy_boundary" \
  '.database_boundary = $boundary' \
  "$root_copy_authority" >"$unsupported_copy_authority"
unsupported_copy_digest="$(shasum -a 256 "$unsupported_copy_authority" | awk '{print $1}')"
if scripts/aws/validate-rehearsal-authority.sh \
  "$unsupported_copy_authority" \
  "$unsupported_copy_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$unsupported_copy_boundary" \
  bounded-root-bootstrap-v1 \
  cleanup-bounded-root-bootstrap-v1 2>/dev/null; then
  echo "authority validator accepted an unsupported database source" >&2
  exit 1
fi

root_temp_boundary='{"mode":"disposable-rds","rds_stack_name":"minco-rds-reviewed","instance_id":"minco-reviewed","parameter_name":"/minco/smoke/reviewed/database-url"}'
root_temp_authority="$fixture_dir/root-temp-authority.json"
jq \
  --argjson boundary "$root_temp_boundary" \
  '.database_boundary = $boundary
   | .resource_allowlist = "bounded-root-temp-rds-v1"
   | .cleanup_blast_radius = "cleanup-bounded-root-temp-rds-v1"' \
  "$authority_file" >"$root_temp_authority"
root_temp_digest="$(shasum -a 256 "$root_temp_authority" | awk '{print $1}')"
scripts/aws/validate-rehearsal-authority.sh \
  "$root_temp_authority" \
  "$root_temp_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$root_temp_boundary" \
  bounded-root-temp-rds-v1 \
  cleanup-bounded-root-temp-rds-v1

incomplete_root_temp_boundary='{"mode":"disposable-rds"}'
incomplete_root_temp_authority="$fixture_dir/incomplete-root-temp-authority.json"
jq --argjson boundary "$incomplete_root_temp_boundary" \
  '.database_boundary = $boundary' \
  "$root_temp_authority" >"$incomplete_root_temp_authority"
incomplete_root_temp_digest="$(shasum -a 256 "$incomplete_root_temp_authority" | awk '{print $1}')"
if scripts/aws/validate-rehearsal-authority.sh \
  "$incomplete_root_temp_authority" \
  "$incomplete_root_temp_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$incomplete_root_temp_boundary" \
  bounded-root-temp-rds-v1 \
  cleanup-bounded-root-temp-rds-v1 2>/dev/null; then
  echo "authority validator accepted an incomplete disposable database boundary" >&2
  exit 1
fi

incomplete_boundary='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url"}'
incomplete_authority="$fixture_dir/incomplete-authority.json"
jq --argjson boundary "$incomplete_boundary" '.database_boundary = $boundary' \
  "$authority_file" >"$incomplete_authority"
incomplete_digest="$(shasum -a 256 "$incomplete_authority" | awk '{print $1}')"
if scripts/aws/validate-rehearsal-authority.sh \
  "$incomplete_authority" \
  "$incomplete_digest" \
  reviewed-run \
  0123456789abcdef0123456789abcdef01234567 \
  ap-southeast-2 \
  minco-rehearsal \
  dev \
  "$incomplete_boundary" \
  bounded-direct-smoke-v1 \
  cleanup-bounded-direct-smoke-v1 2>/dev/null; then
  echo "authority validator accepted an incomplete database boundary" >&2
  exit 1
fi

fake_bin="$fixture_dir/bin"
mkdir "$fake_bin"
for command in aws cargo; do
  printf '#!/usr/bin/env bash\nprintf invoked >%q\nexit 88\n' \
    "$fixture_dir/$command-invoked" >"$fake_bin/$command"
  chmod +x "$fake_bin/$command"
done
deadline_error="$fixture_dir/deadline-error.txt"
MINCO_AWS_RUN_ID=reviewed-run
MINCO_AWS_TOUCH_LOG="$fixture_dir/cloud-touches.jsonl"
AWS_REGION=ap-southeast-2
export AWS_REGION MINCO_AWS_RUN_ID MINCO_AWS_TOUCH_LOG
if PATH="$fake_bin:$PATH" \
  MINCO_REHEARSAL_DEADLINE_EPOCH="$(( $(date -u +%s) - 1 ))" \
  MINCO_REHEARSAL_CLEANUP_MODE=false \
  aws_logged sts get-caller-identity "expired rehearsal must fail locally" \
    --output json 2>"$deadline_error"; then
  echo "cloud helper accepted an expired rehearsal" >&2
  exit 1
fi
grep -q "duration authority expired" "$deadline_error" || {
  echo "cloud helper did not report the expired duration authority" >&2
  exit 1
}
[[ ! -e "$fixture_dir/aws-invoked" ]] || {
  echo "cloud helper contacted AWS after rehearsal duration expired" >&2
  exit 1
}
runner_error="$fixture_dir/runner-error.txt"
if PATH="$fake_bin:$PATH" \
  AWS_PROFILE=minco-rehearsal \
  AWS_REGION=ap-southeast-2 \
  MINCO_AWS_RUN_ID=reviewed-run \
  MINCO_DATABASE_URL_PARAMETER=/minco/rehearsal/database-url \
  bash scripts/aws/run-bounded-smoke.sh 2>"$runner_error"; then
  echo "bounded runner accepted missing rehearsal authority" >&2
  exit 1
fi
grep -q "MINCO_REHEARSAL_AUTHORITY_FILE" "$runner_error" || {
  echo "bounded runner did not fail on its missing authority input" >&2
  exit 1
}
[[ ! -e "$fixture_dir/aws-invoked" && ! -e "$fixture_dir/cargo-invoked" ]] || {
  echo "bounded runner invoked a build or AWS command before authority validation" >&2
  exit 1
}

rm -f "$fixture_dir/aws-invoked" "$fixture_dir/cargo-invoked"
root_runner_error="$fixture_dir/root-runner-error.txt"
if PATH="$fake_bin:$PATH" \
  MINCO_ROOT_PROFILE=minco-root-rehearsal \
  AWS_REGION=ap-southeast-2 \
  MINCO_AWS_RUN_ID=reviewed-run \
  MINCO_CREATE_TEMP_RDS=true \
  bash scripts/aws/run-bounded-root-bootstrap.sh 2>"$root_runner_error"; then
  echo "root bootstrap accepted missing rehearsal authority" >&2
  exit 1
fi
grep -q "MINCO_REHEARSAL_AUTHORITY_FILE" "$root_runner_error" || {
  echo "root bootstrap did not fail on its missing authority input" >&2
  exit 1
}
[[ ! -e "$fixture_dir/aws-invoked" ]] || {
  echo "root bootstrap invoked AWS before authority validation" >&2
  exit 1
}

rm -f "$fixture_dir/aws-invoked"
inspect_error="$fixture_dir/inspect-error.txt"
if PATH="$fake_bin:$PATH" \
  AWS_PROFILE=minco-rehearsal \
  AWS_REGION=ap-southeast-2 \
  MINCO_AWS_RUN_ID=reviewed-run \
  MINCO_DATABASE_URL_PARAMETER=/minco/rehearsal/database-url \
  bash scripts/aws/inspect-account.sh 2>"$inspect_error"; then
  echo "account inspection accepted missing rehearsal authority" >&2
  exit 1
fi
grep -q "MINCO_REHEARSAL_AUTHORITY_FILE" "$inspect_error" || {
  echo "account inspection did not fail on its missing authority input" >&2
  exit 1
}
[[ ! -e "$fixture_dir/aws-invoked" ]] || {
  echo "account inspection contacted AWS before authority validation" >&2
  exit 1
}
