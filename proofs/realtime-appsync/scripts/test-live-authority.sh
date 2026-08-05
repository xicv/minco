#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=/dev/null
source "$proof_root/scripts/live-cleanup.sh"
fixture="$(mktemp -d)"
cleanup() {
  rm -r -- "$fixture"
}
trap cleanup EXIT

mkdir -p "$fixture/bin"
aws_calls="$fixture/aws-calls"
cat >"$fixture/bin/aws" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${MINCO_TEST_AWS_CALLS:?}"
exit 97
EOF
chmod 0755 "$fixture/bin/aws"

authority=(
  AWS_REGION=ap-southeast-2
  MINCO_APPSYNC_PROOF_PROFILE=minco-proof
  MINCO_APPSYNC_PROOF_ALLOW_ACCOUNT=123456789012
  MINCO_APPSYNC_PROOF_EXPECTED_ROLE_ARN=arn:aws:iam::123456789012:role/minco-realtime-proof
  MINCO_APPSYNC_PROOF_STACK=minco-appsync-proof-test
  MINCO_APPSYNC_PROOF_SOURCE_SHA=0123456789012345678901234567890123456789
  MINCO_APPSYNC_PROOF_MAX_DURATION_MINUTES=30
  MINCO_APPSYNC_PROOF_MAX_SPEND_USD=5
  MINCO_APPSYNC_PROOF_RESOURCES='AWS::AppSync::Api,AWS::AppSync::ChannelNamespace,AWS::Cognito::UserPool,AWS::Cognito::UserPoolClient,AWS::IAM::Role,AWS::Lambda::Function,AWS::Logs::LogGroup,S3ArtifactBucket'
  MINCO_APPSYNC_PROOF_CLEANUP='delete-stack:minco-appsync-proof-test;delete-bucket:minco-appsync-proof-test-artifacts-123456789012'
)

fields=(
  AWS_REGION
  MINCO_APPSYNC_PROOF_PROFILE
  MINCO_APPSYNC_PROOF_ALLOW_ACCOUNT
  MINCO_APPSYNC_PROOF_EXPECTED_ROLE_ARN
  MINCO_APPSYNC_PROOF_STACK
  MINCO_APPSYNC_PROOF_SOURCE_SHA
  MINCO_APPSYNC_PROOF_MAX_DURATION_MINUTES
  MINCO_APPSYNC_PROOF_MAX_SPEND_USD
  MINCO_APPSYNC_PROOF_RESOURCES
  MINCO_APPSYNC_PROOF_CLEANUP
)

for index in "${!fields[@]}"; do
  : >"$aws_calls"
  if env -i \
    HOME="$HOME" \
    PATH="$fixture/bin:$PATH" \
    MINCO_TEST_AWS_CALLS="$aws_calls" \
    "${authority[@]:0:index}" \
    bash "$proof_root/scripts/run-live-aws.sh" >"$fixture/stdout" 2>"$fixture/stderr"; then
    echo "live runner accepted missing authority field ${fields[$index]}" >&2
    exit 1
  fi

  if [[ -s "$aws_calls" ]]; then
    echo "live runner contacted AWS before validating ${fields[$index]}" >&2
    exit 1
  fi

  grep -q "${fields[$index]}" "$fixture/stderr" || {
    echo "live runner did not name missing authority field ${fields[$index]}" >&2
    exit 1
  }
done

invalid_authority=(
  'AWS_REGION=bad/region'
  'MINCO_APPSYNC_PROOF_PROFILE=bad profile'
  'MINCO_APPSYNC_PROOF_ALLOW_ACCOUNT=123'
  'MINCO_APPSYNC_PROOF_EXPECTED_ROLE_ARN=arn:aws:iam::999999999999:role/wrong-account'
  'MINCO_APPSYNC_PROOF_STACK=bad_stack_name'
  'MINCO_APPSYNC_PROOF_SOURCE_SHA=not-a-commit'
  'MINCO_APPSYNC_PROOF_MAX_DURATION_MINUTES=31'
  'MINCO_APPSYNC_PROOF_MAX_SPEND_USD=5.01'
  'MINCO_APPSYNC_PROOF_RESOURCES=all-resources'
  'MINCO_APPSYNC_PROOF_CLEANUP=delete-everything'
)

for index in "${!invalid_authority[@]}"; do
  : >"$aws_calls"
  if env -i \
    HOME="$HOME" \
    PATH="$fixture/bin:$PATH" \
    MINCO_TEST_AWS_CALLS="$aws_calls" \
    "${authority[@]}" \
    "${invalid_authority[$index]}" \
    bash "$proof_root/scripts/run-live-aws.sh" >"$fixture/stdout" 2>"$fixture/stderr"; then
    echo "live runner accepted malformed authority field ${fields[$index]}" >&2
    exit 1
  fi

  if [[ -s "$aws_calls" ]]; then
    echo "live runner contacted AWS with malformed ${fields[$index]}" >&2
    exit 1
  fi

  grep -q "${fields[$index]}" "$fixture/stderr" || {
    echo "live runner did not name malformed authority field ${fields[$index]}" >&2
    exit 1
  }
done

rg -q 'refusing to adopt or delete pre-existing stack' "$proof_root/scripts/run-live-aws.sh"
rg -Uq 'delete-stack \\\n+[[:space:]]+--stack-name "\$stack_id"' "$proof_root/scripts/run-live-aws.sh"
if rg -q 'delete-stack --stack-name "\$stack"' "$proof_root/scripts/run-live-aws.sh"; then
  echo "live runner cleanup is bound to a mutable stack name instead of the created stack ID" >&2
  exit 1
fi
cleanup_body="$(sed -n '/^cleanup() {$/,/^on_exit() {$/p' "$proof_root/scripts/run-live-aws.sh")"
if rg -q 'describe-stacks' <<<"$cleanup_body"; then
  echo "live runner can skip exact stack deletion when a cleanup preflight fails" >&2
  exit 1
fi
rg -q 'does not exist' <<<"$cleanup_body" || {
  echo "live runner does not treat an already-absent exact stack as cleaned" >&2
  exit 1
}
rg -q 'head-bucket --bucket "\$bucket"' <<<"$cleanup_body" || {
  echo "live runner does not verify the exact proof bucket is absent after cleanup" >&2
  exit 1
}
if rg -q -- '--(temporary-)?password "\$proof_password"' \
  "$proof_root/scripts/run-live-aws.sh"; then
  echo "live runner exposes the temporary password in a process argument" >&2
  exit 1
fi
[[ "$(rg -c -- '--cli-input-json "file://\$auth_parameters"' \
  "$proof_root/scripts/run-live-aws.sh")" -eq 3 ]] || {
  echo "live runner must pass every password-bearing Cognito request through a protected file" >&2
  exit 1
}
rg -Fq "jj log -r '@ & conflicts()'" "$proof_root/scripts/run-live-aws.sh" || {
  echo "live runner must reject conflicts in the exact JJ working-copy commit" >&2
  exit 1
}
if rg -q 'jj diff --summary' "$proof_root/scripts/run-live-aws.sh"; then
  echo "live runner incorrectly treats a non-empty JJ commit as an uncommitted Git-style tree" >&2
  exit 1
fi

empty_bucket='{"RequestCharged":null,"Prefix":""}'
appsync_proof_bucket_versions_are_empty "$empty_bucket" || {
  echo "live cleanup rejected AWS's empty ListObjectVersions response" >&2
  exit 1
}
if appsync_proof_bucket_versions_are_empty \
  '{"Versions":[{"Key":"unexpected"}],"DeleteMarkers":[]}'; then
  echo "live cleanup accepted an unexpected object version" >&2
  exit 1
fi
if appsync_proof_bucket_versions_are_empty \
  '{"Versions":[],"DeleteMarkers":[{"Key":"unexpected"}]}'; then
  echo "live cleanup accepted an unexpected delete marker" >&2
  exit 1
fi

echo "Realtime AppSync live authority pre-contact gate passed."
