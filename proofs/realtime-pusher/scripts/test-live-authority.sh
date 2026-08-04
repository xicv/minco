#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
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
  MINCO_REALTIME_PROOF_PROFILE=minco-proof
  MINCO_REALTIME_PROOF_ALLOW_ACCOUNT=123456789012
  MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN=arn:aws:iam::123456789012:role/minco-realtime-proof
  MINCO_REALTIME_PROOF_STACK=minco-realtime-proof-test
  MINCO_REALTIME_PROOF_SOURCE_SHA=0123456789012345678901234567890123456789
  MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES=30
  MINCO_REALTIME_PROOF_MAX_SPEND_USD=5
  MINCO_REALTIME_PROOF_CLEANUP='delete-stack:minco-realtime-proof-test;delete-bucket:minco-realtime-proof-test-artifacts-123456789012'
)

fields=(
  AWS_REGION
  MINCO_REALTIME_PROOF_PROFILE
  MINCO_REALTIME_PROOF_ALLOW_ACCOUNT
  MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN
  MINCO_REALTIME_PROOF_STACK
  MINCO_REALTIME_PROOF_SOURCE_SHA
  MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES
  MINCO_REALTIME_PROOF_MAX_SPEND_USD
  MINCO_REALTIME_PROOF_CLEANUP
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
    cat "$aws_calls" >&2
    exit 1
  fi

  grep -q "${fields[$index]}" "$fixture/stderr" || {
    echo "live runner did not name missing authority field ${fields[$index]}" >&2
    cat "$fixture/stderr" >&2
    exit 1
  }
done

invalid_authority=(
  'AWS_REGION=bad/region'
  'MINCO_REALTIME_PROOF_PROFILE=bad profile'
  'MINCO_REALTIME_PROOF_ALLOW_ACCOUNT=123'
  'MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN=arn:aws:iam::999999999999:role/wrong-account'
  'MINCO_REALTIME_PROOF_STACK=bad_stack_name'
  'MINCO_REALTIME_PROOF_SOURCE_SHA=not-a-commit'
  'MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES=31'
  'MINCO_REALTIME_PROOF_MAX_SPEND_USD=5.01'
  'MINCO_REALTIME_PROOF_CLEANUP=delete-everything'
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
    cat "$aws_calls" >&2
    exit 1
  fi

  grep -q "${fields[$index]}" "$fixture/stderr" || {
    echo "live runner did not name malformed authority field ${fields[$index]}" >&2
    cat "$fixture/stderr" >&2
    exit 1
  }
done

rg -q 'refusing to adopt or delete pre-existing stack' "$proof_root/scripts/run-live-aws.sh"
rg -Uq 'delete-stack \\\n[[:space:]]+--stack-name "\$stack_id"' "$proof_root/scripts/run-live-aws.sh"
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

echo "Realtime Pusher live authority pre-contact gate passed."
