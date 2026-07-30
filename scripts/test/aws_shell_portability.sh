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

request_id_headers="$review_fixture_dir/request-id.headers"
for header_name in x-request-id x-amzn-requestid apigw-requestid; do
  printf 'HTTP/2 401\r\n%s: request-123\r\n\r\n' \
    "$header_name" >"$request_id_headers"
  [[ "$(http_response_request_id "$request_id_headers")" == "request-123" ]] || {
    printf 'did not recognize %s as an HTTP request ID\n' "$header_name" >&2
    exit 1
  }
done
printf 'HTTP/2 401\r\ncontent-type: application/json\r\n\r\n' \
  >"$request_id_headers"
[[ -z "$(http_response_request_id "$request_id_headers")" ]] || {
  printf 'accepted an unrelated header as an HTTP request ID\n' >&2
  exit 1
}

bucket_visibility_error="$review_fixture_dir/bucket-visibility-error.txt"
bucket_visibility_calls=0
aws_logged() {
  bucket_visibility_calls=$((bucket_visibility_calls + 1))
  if ((bucket_visibility_calls < 3)); then
    printf 'An error occurred (404) when calling HeadBucket: Not Found\n' >&2
    return 1
  fi
}
wait_for_s3_bucket_visibility \
  minco-smoke-test \
  ap-southeast-2 \
  "$bucket_visibility_error" \
  3 \
  0
[[ "$bucket_visibility_calls" == "3" ]] || {
  printf 'artifact bucket visibility did not retry bounded 404 responses\n' >&2
  exit 1
}

bucket_visibility_calls=0
aws_logged() {
  bucket_visibility_calls=$((bucket_visibility_calls + 1))
  printf 'AccessDenied\n' >&2
  return 1
}
if wait_for_s3_bucket_visibility \
  minco-smoke-test \
  ap-southeast-2 \
  "$bucket_visibility_error" \
  3 \
  0 2>/dev/null; then
  printf 'artifact bucket visibility accepted a non-404 error\n' >&2
  exit 1
fi
[[ "$bucket_visibility_calls" == "1" ]] || {
  printf 'artifact bucket visibility retried a non-404 error\n' >&2
  exit 1
}

bucket_visibility_calls=0
aws_logged() {
  bucket_visibility_calls=$((bucket_visibility_calls + 1))
  printf 'NoSuchBucket\n' >&2
  return 1
}
if wait_for_s3_bucket_visibility \
  minco-smoke-test \
  ap-southeast-2 \
  "$bucket_visibility_error" \
  3 \
  0 2>/dev/null; then
  printf 'artifact bucket visibility accepted exhausted 404 retries\n' >&2
  exit 1
fi
[[ "$bucket_visibility_calls" == "3" ]] || {
  printf 'artifact bucket visibility exceeded its retry bound\n' >&2
  exit 1
}

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

target_fixture="$review_fixture_dir/deployment-targets.toml"
write_bounded_deployment_target_config \
  "$target_fixture" \
  123456789012 \
  ap-southeast-2 \
  arn:aws:iam::123456789012:role/minco-smoke-test \
  minco-smoke-test \
  minco-smoke-test \
  /minco/smoke/test/database-url \
  "" \
  subnet-a,subnet-b \
  sg-test \
  test-run
python3 - "$target_fixture" <<'PY'
from pathlib import Path
import sys
import tomllib

target = tomllib.loads(Path(sys.argv[1]).read_text())["environments"]["dev"]
if target["stack_tags"] != {
    "minco:managed": "true",
    "minco:purpose": "bounded-smoke",
    "minco:run-id": "test-run",
}:
    raise SystemExit("bounded target omitted exact run-ownership stack tags")
PY

python3 - "$review_fixture_dir" <<'PY'
import copy
import json
from pathlib import Path
import subprocess
import sys

source = Path("scripts/aws/run-bounded-root-bootstrap.sh").read_text()
if (
    "InvalidClientTokenId|AccessDenied|not authorized to perform: sts:AssumeRole"
    not in source
):
    raise SystemExit(
        "role assumption does not retry the fresh-key propagation failure"
    )
if (
    'if [[ "$application_runner_started" == false ]]; then\n'
    "    application_cleanup=true"
    not in source
):
    raise SystemExit(
        "bootstrap cleanup does not recognize a never-started application"
    )
if (
    "application_runner_started=true\n"
    "AWS_CONFIG_FILE=\"$profile_config\""
    not in source
):
    raise SystemExit(
        "bootstrap does not mark the application runner before invoking it"
    )
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

mutation_statement = next(
    item for item in policy["Statement"]
    if item["Sid"] == "MutateTemporaryHttpApiViaCloudFormation"
)
if mutation_statement != {
    "Sid": "MutateTemporaryHttpApiViaCloudFormation",
    "Effect": "Allow",
    "Action": [
        "apigateway:DELETE",
        "apigateway:PATCH",
        "apigateway:POST",
        "apigateway:PUT",
    ],
    "Resource": f"arn:aws:apigateway:{region}::/*",
    "Condition": {
        "ForAnyValue:StringEquals": {
            "aws:CalledVia": "cloudformation.amazonaws.com",
        },
    },
}:
    raise SystemExit("API Gateway mutation policy exceeds the CloudFormation boundary")

stage_create_statement = next(
    item for item in policy["Statement"]
    if item["Sid"] == "CreateRunOwnedTemporaryHttpApiStage"
)
allowed_stage_tag_keys = [
    "minco:run-id",
    "minco:managed",
    "minco:purpose",
    "MincoEnvironment",
    "MincoReleaseId",
    "MincoReleaseDigest",
    "httpapi:createdBy",
    "aws:cloudformation:stack-name",
    "aws:cloudformation:stack-id",
    "aws:cloudformation:logical-id",
]
if stage_create_statement != {
    "Sid": "CreateRunOwnedTemporaryHttpApiStage",
    "Effect": "Allow",
    "Action": "apigateway:POST",
    "Resource": f"arn:aws:apigateway:{region}::/apis/*/stages",
    "Condition": {
        "StringEquals": expected_tags,
        "ForAllValues:StringEquals": {
            "aws:TagKeys": allowed_stage_tag_keys,
        },
    },
}:
    raise SystemExit("API Gateway stage creation policy exceeds the run-owned boundary")
stage_tag_statement = next(
    item for item in policy["Statement"]
    if item["Sid"] == "TagRunOwnedTemporaryHttpApiStage"
)
if stage_tag_statement != {
    "Sid": "TagRunOwnedTemporaryHttpApiStage",
    "Effect": "Allow",
    "Action": "apigateway:TagResource",
    "Resource": f"arn:aws:apigateway:{region}::/apis/*/stages",
    "Condition": {
        "StringEquals": expected_tags,
        "ForAllValues:StringEquals": {
            "aws:TagKeys": allowed_stage_tag_keys,
        },
    },
}:
    raise SystemExit("API Gateway stage tagging policy exceeds the run-owned boundary")

fixture_dir = Path(sys.argv[1])
(fixture_dir / "role-policy.json").write_text(
    json.dumps(policy, separators=(",", ":"))
)
statement_index = policy["Statement"].index(stage_tag_statement)
stale_finding = {
    "findingDetails": "The action apigateway:TagResource does not exist.",
    "findingType": "ERROR",
    "issueCode": "INVALID_ACTION",
    "learnMoreLink": (
        "https://docs.aws.amazon.com/IAM/latest/UserGuide/"
        "access-analyzer-reference-policy-checks.html"
        "#access-analyzer-reference-policy-checks-error-invalid-action"
    ),
    "locations": [{
        "path": [
            {"value": "Statement"},
            {"index": statement_index},
            {"value": "Action"},
        ],
        "span": {
            "start": {"line": 1, "column": 1, "offset": 1},
            "end": {"line": 1, "column": 2, "offset": 2},
        },
    }],
}
(fixture_dir / "validation-clean.json").write_text(
    json.dumps({"findings": []}, separators=(",", ":"))
)
(fixture_dir / "validation-stale-stage-tag-action.json").write_text(
    json.dumps({"findings": [stale_finding]}, separators=(",", ":"))
)
unrelated_error = {
    "findingDetails": "A different validation error.",
    "findingType": "ERROR",
    "issueCode": "OTHER_ERROR",
    "locations": [],
}
(fixture_dir / "validation-stale-plus-error.json").write_text(
    json.dumps(
        {"findings": [stale_finding, unrelated_error]},
        separators=(",", ":"),
    )
)
wrong_location_finding = copy.deepcopy(stale_finding)
wrong_location_finding["locations"][0]["path"][1]["index"] += 1
(fixture_dir / "validation-stale-wrong-location.json").write_text(
    json.dumps(
        {"findings": [wrong_location_finding]},
        separators=(",", ":"),
    )
)
broader_policy = copy.deepcopy(policy)
broader_policy["Statement"][statement_index]["Resource"] = (
    f"arn:aws:apigateway:{region}::/*"
)
(fixture_dir / "role-policy-broader-stage-tag.json").write_text(
    json.dumps(broader_policy, separators=(",", ":"))
)
additional_broad_policy = copy.deepcopy(policy)
additional_broad_policy["Statement"].append({
    "Sid": "BroaderApiGatewayAccess",
    "Effect": "Allow",
    "Action": "apigateway:*",
    "Resource": "*",
})
(fixture_dir / "role-policy-additional-api-wildcard.json").write_text(
    json.dumps(additional_broad_policy, separators=(",", ":"))
)
PY

access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-clean.json" \
  "$review_fixture_dir/role-policy.json" \
  ap-southeast-2 \
  test-run
access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-stale-stage-tag-action.json" \
  "$review_fixture_dir/role-policy.json" \
  ap-southeast-2 \
  test-run
if access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-stale-plus-error.json" \
  "$review_fixture_dir/role-policy.json" \
  ap-southeast-2 \
  test-run; then
  printf 'accepted an unrelated Access Analyzer error\n' >&2
  exit 1
fi
if access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-stale-wrong-location.json" \
  "$review_fixture_dir/role-policy.json" \
  ap-southeast-2 \
  test-run; then
  printf 'accepted the stale finding at a different policy location\n' >&2
  exit 1
fi
if access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-stale-stage-tag-action.json" \
  "$review_fixture_dir/role-policy-broader-stage-tag.json" \
  ap-southeast-2 \
  test-run; then
  printf 'accepted the stale finding for a broader stage-tagging policy\n' >&2
  exit 1
fi
if access_analyzer_role_policy_is_accepted \
  "$review_fixture_dir/validation-stale-stage-tag-action.json" \
  "$review_fixture_dir/role-policy-additional-api-wildcard.json" \
  ap-southeast-2 \
  test-run; then
  printf 'accepted the stale finding alongside an API Gateway wildcard\n' >&2
  exit 1
fi

printf 'AWS shell portability checks passed.\n'
