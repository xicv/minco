#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

for parameter_name in \
  /minco/smoke/abc123/database-url \
  /minco/service.name/database_url \
  /minco/service-name/database.url; do
  normalized_ssm_parameter_name "$parameter_name"
done

for parameter_name in \
  minco/smoke/abc123/database-url \
  /minco//database-url \
  /minco/database-url/ \
  "/minco/database url"; do
  if normalized_ssm_parameter_name "$parameter_name"; then
    printf 'accepted invalid SSM parameter name\n' >&2
    exit 1
  fi
done

review_fixture_dir="$(mktemp -d)"
cleanup_review_fixture() {
  rm -r -- "$review_fixture_dir"
}
trap cleanup_review_fixture EXIT
printf '%s\n' minco-smoke-test >"$review_fixture_dir/stack-preflight-absent.txt"
printf '%s\n' \
  '{"Stacks":[{"StackName":"minco-smoke-test","StackStatus":"REVIEW_IN_PROGRESS","Tags":[]}]}' \
  >"$review_fixture_dir/stack.json"
printf '%s\n' '{"StackResourceSummaries":[]}' >"$review_fixture_dir/resources.json"

if ! bounded_review_stack_cleanup_is_authorized \
  "$review_fixture_dir/stack.json" \
  "$review_fixture_dir/resources.json" \
  "$review_fixture_dir/stack-preflight-absent.txt" \
  minco-smoke-test; then
  printf 'rejected an exact empty run-created review stack\n' >&2
  exit 1
fi

printf '%s\n' \
  '{"StackResourceSummaries":[{"LogicalResourceId":"KeepMe"}]}' \
  >"$review_fixture_dir/resources.json"
if bounded_review_stack_cleanup_is_authorized \
  "$review_fixture_dir/stack.json" \
  "$review_fixture_dir/resources.json" \
  "$review_fixture_dir/stack-preflight-absent.txt" \
  minco-smoke-test; then
  printf 'authorized review-stack cleanup with a resource present\n' >&2
  exit 1
fi
printf '%s\n' '{"StackResourceSummaries":[]}' >"$review_fixture_dir/resources.json"
printf '%s\n' minco-smoke-other >"$review_fixture_dir/stack-preflight-absent.txt"
if bounded_review_stack_cleanup_is_authorized \
  "$review_fixture_dir/stack.json" \
  "$review_fixture_dir/resources.json" \
  "$review_fixture_dir/stack-preflight-absent.txt" \
  minco-smoke-test; then
  printf 'authorized review-stack cleanup with mismatched preflight evidence\n' >&2
  exit 1
fi
printf '%s\n' minco-smoke-test >"$review_fixture_dir/stack-preflight-absent.txt"
printf '%s\n' \
  '{"Stacks":[{"StackName":"minco-smoke-test","StackStatus":"CREATE_COMPLETE","Tags":[]}]}' \
  >"$review_fixture_dir/stack.json"
if bounded_review_stack_cleanup_is_authorized \
  "$review_fixture_dir/stack.json" \
  "$review_fixture_dir/resources.json" \
  "$review_fixture_dir/stack-preflight-absent.txt" \
  minco-smoke-test; then
  printf 'authorized cleanup of an untagged non-review stack\n' >&2
  exit 1
fi

python3 - <<'PY'
import json
from pathlib import Path
import subprocess

source = Path("scripts/aws/run-bounded-root-bootstrap.sh").read_text()
start_marker = '  --argjson create_temp_rds "$MINCO_CREATE_TEMP_RDS" \\\n  \''
end_marker = '\' >"$request_directory/role-policy.json"'
start = source.find(start_marker)
if start < 0:
    raise SystemExit("missing bootstrap role policy start")
start += len(start_marker)
end = source.find(end_marker, start)
if end < 0:
    raise SystemExit("missing bootstrap role policy end")

account_id = "123456789012"
region = "ap-southeast-2"
run_id = "test-run"
arguments = [
    "jq", "-n",
    "--arg", "stack_arn", "arn:aws:cloudformation:ap-southeast-2:123456789012:stack/test/*",
    "--arg", "rds_stack_arn", "arn:aws:cloudformation:ap-southeast-2:123456789012:stack/rds/*",
    "--arg", "bucket_arn", "arn:aws:s3:::test",
    "--arg", "parameter_arn", "arn:aws:ssm:ap-southeast-2:123456789012:parameter/minco/test",
    "--arg", "function_arn", "arn:aws:lambda:ap-southeast-2:123456789012:function:test",
    "--arg", "execution_role_arn", "arn:aws:iam::123456789012:role/test-*",
    "--arg", "log_group_arn", "arn:aws:logs:ap-southeast-2:123456789012:log-group:/aws/lambda/test",
    "--arg", "rds_instance_arn", "arn:aws:rds:ap-southeast-2:123456789012:db:test",
    "--arg", "rds_subnet_group_arn", "arn:aws:rds:ap-southeast-2:123456789012:subgrp:test-*",
    "--arg", "rds_secret_arn", "arn:aws:secretsmanager:ap-southeast-2:123456789012:secret:rds!db-*",
    "--arg", "region", region,
    "--arg", "account_id", account_id,
    "--arg", "run_id", run_id,
    "--argjson", "create_temp_rds", "true",
    source[start:end],
]
policy = json.loads(subprocess.run(
    arguments,
    check=True,
    capture_output=True,
    text=True,
).stdout)
statement = next(
    item for item in policy["Statement"]
    if item["Sid"] == "TagOwnedTemporaryCognitoHarness"
)
expected_tags = {
    "aws:RequestTag/minco:run-id": run_id,
    "aws:RequestTag/minco:managed": "true",
    "aws:RequestTag/minco:purpose": "bounded-smoke",
}
expected_keys = ["minco:run-id", "minco:managed", "minco:purpose"]
if statement != {
    "Sid": "TagOwnedTemporaryCognitoHarness",
    "Effect": "Allow",
    "Action": ["cognito-idp:TagResource"],
    "Resource": (
        f"arn:aws:cognito-idp:{region}:{account_id}:userpool/*"
    ),
    "Condition": {
        "StringEquals": expected_tags,
        "ForAllValues:StringEquals": {"aws:TagKeys": expected_keys},
    },
}:
    raise SystemExit("Cognito tagging policy exceeds or misses the run-owned boundary")
PY

printf 'AWS shell portability checks passed.\n'
