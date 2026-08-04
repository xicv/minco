#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
repository_root="$(cd "$proof_root/../.." && pwd)"
# shellcheck source=/dev/null
source "$repository_root/scripts/aws/lib/common.sh"

region="${AWS_REGION:-}"
profile="${MINCO_REALTIME_PROOF_PROFILE:-}"
allowed_account="${MINCO_REALTIME_PROOF_ALLOW_ACCOUNT:-}"
expected_role_arn="${MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN:-}"
stack="${MINCO_REALTIME_PROOF_STACK:-}"
approved_source="${MINCO_REALTIME_PROOF_SOURCE_SHA:-}"
max_duration="${MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES:-}"
max_spend="${MINCO_REALTIME_PROOF_MAX_SPEND_USD:-}"
cleanup_authority="${MINCO_REALTIME_PROOF_CLEANUP:-}"

require_authority() {
  local name="$1"
  local value="$2"
  if [[ -z "$value" ]]; then
    printf '%s is required before contacting AWS\n' "$name" >&2
    exit 64
  fi
}

require_authority AWS_REGION "$region"
require_authority MINCO_REALTIME_PROOF_PROFILE "$profile"
require_authority MINCO_REALTIME_PROOF_ALLOW_ACCOUNT "$allowed_account"
require_authority MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN "$expected_role_arn"
require_authority MINCO_REALTIME_PROOF_STACK "$stack"
require_authority MINCO_REALTIME_PROOF_SOURCE_SHA "$approved_source"
require_authority MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES "$max_duration"
require_authority MINCO_REALTIME_PROOF_MAX_SPEND_USD "$max_spend"
require_authority MINCO_REALTIME_PROOF_CLEANUP "$cleanup_authority"

[[ "$region" =~ ^[a-z0-9-]{3,32}$ ]] || {
  echo "AWS_REGION has an unsupported shape" >&2
  exit 64
}
[[ "$profile" =~ ^[A-Za-z0-9_+=,.@-]{1,64}$ ]] || {
  echo "MINCO_REALTIME_PROOF_PROFILE has an unsupported shape" >&2
  exit 64
}
[[ "$allowed_account" =~ ^[0-9]{12}$ ]] || {
  echo "MINCO_REALTIME_PROOF_ALLOW_ACCOUNT must be a 12-digit account ID" >&2
  exit 64
}
[[ ${#expected_role_arn} -le 300 && "$expected_role_arn" =~ ^arn:(aws|aws-cn|aws-us-gov):iam::${allowed_account}:role/[A-Za-z0-9_+=,.@/-]+$ ]] || {
  echo "MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN must name an exact role in the approved account" >&2
  exit 64
}
[[ ${#stack} -le 39 && "$stack" =~ ^[A-Za-z][A-Za-z0-9-]*$ ]] || {
  echo "MINCO_REALTIME_PROOF_STACK must be an explicit CloudFormation-safe name of at most 39 characters" >&2
  exit 64
}
[[ "$approved_source" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]] || {
  echo "MINCO_REALTIME_PROOF_SOURCE_SHA must be an exact Git or JJ commit ID" >&2
  exit 64
}
if [[ ! "$max_duration" =~ ^[0-9]+$ ]] ||
  ((max_duration < 1 || max_duration > 30)); then
  echo "MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES must be between 1 and 30" >&2
  exit 64
fi
if [[ ! "$max_spend" =~ ^[0-9]+([.][0-9]{1,2})?$ ]] ||
  ! awk -v spend="$max_spend" 'BEGIN { exit !(spend > 0 && spend <= 5) }'; then
  echo "MINCO_REALTIME_PROOF_MAX_SPEND_USD must be greater than 0 and at most 5" >&2
  exit 64
fi

bucket="${stack}-artifacts-${allowed_account}"
expected_cleanup="delete-stack:${stack};delete-bucket:${bucket}"
[[ "$cleanup_authority" == "$expected_cleanup" ]] || {
  echo "MINCO_REALTIME_PROOF_CLEANUP must equal: $expected_cleanup" >&2
  exit 64
}

for command in aws base64 cargo jq npm openssl rg sam shasum uv; do
  require_command "$command"
done
cargo lambda --version >/dev/null

cd "$repository_root"
source_revision="$(current_source_revision)"
[[ "$source_revision" == "$approved_source" ]] || {
  echo "MINCO_REALTIME_PROOF_SOURCE_SHA does not match the current checkout" >&2
  exit 64
}
if [[ -d .jj ]]; then
  [[ -z "$(jj diff --summary)" ]] || {
    echo "the approved JJ source checkout must be clean" >&2
    exit 64
  }
elif [[ -n "$(git status --short)" ]]; then
  echo "the approved Git source checkout must be clean" >&2
  exit 64
fi

started_epoch="$(date -u +%s)"
deadline_epoch="$((started_epoch + max_duration * 60))"
enforce_deadline() {
  if (( $(date -u +%s) > deadline_epoch )); then
    echo "live proof duration authority expired; only cleanup may continue" >&2
    return 1
  fi
}

# Qualify and build the exact source before the first provider call.
bash "$proof_root/scripts/test-local.sh"
enforce_deadline
cargo lambda build \
  --manifest-path "$proof_root/aws-handler/Cargo.toml" \
  --release \
  --arm64 \
  --output-format zip
enforce_deadline

artifact="$proof_root/aws-handler/target/lambda/minco-realtime-pusher-aws-proof/bootstrap.zip"
test -f "$artifact"
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
artifact_checksum="$(openssl dgst -sha256 -binary "$artifact" | base64 | tr -d '\n')"

aws_command=(
  aws
  --profile "$profile"
  --region "$region"
  --cli-connect-timeout 5
  --cli-read-timeout 20
)
identity="$("${aws_command[@]}" sts get-caller-identity --output json)"
account="$(jq -er '.Account' <<<"$identity")"
caller_arn="$(jq -er '.Arn' <<<"$identity")"

case "$caller_arn" in
  arn:*:iam::"$account":role/*)
    caller_role_arn="$caller_arn"
    ;;
  arn:*:sts::"$account":assumed-role/*/*)
    partition="${caller_arn%%:sts::*}"
    role_session="${caller_arn#*:assumed-role/}"
    role_name="${role_session%/*}"
    caller_role_arn="${partition}:iam::${account}:role/${role_name}"
    ;;
  *)
    echo "live proof requires an IAM role or assumed-role caller" >&2
    exit 64
    ;;
esac

[[ "$account" == "$allowed_account" ]] || {
  echo "caller account does not match MINCO_REALTIME_PROOF_ALLOW_ACCOUNT" >&2
  exit 64
}
[[ "$caller_role_arn" == "$expected_role_arn" ]] || {
  echo "caller role does not match MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN" >&2
  exit 64
}
unset identity caller_arn caller_role_arn role_session role_name partition

stack_probe="$(mktemp)"
if "${aws_command[@]}" cloudformation describe-stacks \
  --stack-name "$stack" >"$stack_probe" 2>&1; then
  echo "refusing to adopt or delete pre-existing stack: $stack" >&2
  rm -f -- "$stack_probe"
  exit 64
elif ! rg -q 'does not exist' "$stack_probe"; then
  echo "could not prove the exact stack name is absent" >&2
  sed -n '1,8p' "$stack_probe" >&2
  rm -f -- "$stack_probe"
  exit 1
fi
rm -f -- "$stack_probe"

if "${aws_command[@]}" s3api head-bucket --bucket "$bucket" >/dev/null 2>&1; then
  echo "refusing to adopt or delete pre-existing artifact bucket: $bucket" >&2
  exit 64
fi

code_key="handler-${source_revision}.zip"
code_version=""
stack_id=""
bucket_created=0

cleanup() {
  local cleanup_status=0
  local remaining="{}"
  local stack_delete_error=""
  if [[ -n "$stack_id" ]]; then
    stack_delete_error="$(mktemp)"
    if "${aws_command[@]}" cloudformation delete-stack \
      --stack-name "$stack_id" 2>"$stack_delete_error"; then
      "${aws_command[@]}" cloudformation wait stack-delete-complete \
        --stack-name "$stack_id" || cleanup_status=1
    elif ! rg -q 'does not exist' "$stack_delete_error"; then
      cleanup_status=1
    fi
    rm -f -- "$stack_delete_error"
  fi
  if [[ -n "$code_version" ]]; then
    "${aws_command[@]}" s3api delete-object \
      --bucket "$bucket" \
      --key "$code_key" \
      --version-id "$code_version" >/dev/null || cleanup_status=1
  fi
  if [[ "$bucket_created" -eq 1 ]]; then
    remaining="$("${aws_command[@]}" s3api list-object-versions --bucket "$bucket" --output json)" || cleanup_status=1
    if ! jq -e '((.Versions // []) | length == 0) and ((.DeleteMarkers // []) | length == 0)' \
      <<<"${remaining:-{}}" >/dev/null; then
      echo "refusing broad cleanup because the proof bucket contains unexpected objects" >&2
      cleanup_status=1
    elif ! "${aws_command[@]}" s3api delete-bucket --bucket "$bucket"; then
      cleanup_status=1
    fi
  fi
  if [[ "$cleanup_status" -ne 0 ]]; then
    echo "cleanup failed for exact stack $stack or exact bucket $bucket" >&2
    return 1
  fi
  echo "cleanup=passed"
}
on_exit() {
  local exit_code=$?
  trap - EXIT
  cleanup || exit_code=70
  exit "$exit_code"
}
trap on_exit EXIT

enforce_deadline
if [[ "$region" == "us-east-1" ]]; then
  "${aws_command[@]}" s3api create-bucket --bucket "$bucket" >/dev/null
else
  "${aws_command[@]}" s3api create-bucket \
    --bucket "$bucket" \
    --create-bucket-configuration "LocationConstraint=$region" >/dev/null
fi
bucket_created=1
"${aws_command[@]}" s3api put-bucket-versioning \
  --bucket "$bucket" \
  --versioning-configuration Status=Enabled

upload="$("${aws_command[@]}" s3api put-object \
  --bucket "$bucket" \
  --key "$code_key" \
  --body "$artifact" \
  --checksum-algorithm SHA256 \
  --output json)"
code_version="$(jq -r '.VersionId // ""' <<<"$upload")"
uploaded_checksum="$(jq -r '.ChecksumSHA256 // ""' <<<"$upload")"
[[ -n "$code_version" && "$code_version" != "null" ]] || {
  echo "versioned upload did not return an exact S3 object version" >&2
  exit 1
}
[[ "$uploaded_checksum" == "$artifact_checksum" ]] || {
  echo "uploaded artifact checksum does not match the exact local artifact" >&2
  exit 1
}

enforce_deadline
stack_id="$("${aws_command[@]}" cloudformation create-stack \
  --stack-name "$stack" \
  --template-body "file://$proof_root/aws/template.yaml" \
  --capabilities CAPABILITY_IAM \
  --on-failure DELETE \
  --parameters \
    "ParameterKey=CodeBucket,ParameterValue=$bucket" \
    "ParameterKey=CodeKey,ParameterValue=$code_key" \
    "ParameterKey=CodeObjectVersion,ParameterValue=$code_version" \
  --tags Key=minco:purpose,Value=realtime-proof \
  --query StackId \
  --output text)"
[[ "$stack_id" == arn:*:cloudformation:*:*:stack/"$stack"/* ]] || {
  echo "CloudFormation did not return the exact created stack identity" >&2
  exit 1
}

while true; do
  enforce_deadline
  stack_status="$("${aws_command[@]}" cloudformation describe-stacks \
    --stack-name "$stack_id" \
    --query 'Stacks[0].StackStatus' \
    --output text)"
  case "$stack_status" in
    CREATE_COMPLETE)
      break
      ;;
    *_FAILED | ROLLBACK_* | DELETE_*)
      echo "proof stack did not reach CREATE_COMPLETE: $stack_status" >&2
      exit 1
      ;;
  esac
  sleep 5
done

ws_host="$("${aws_command[@]}" cloudformation describe-stacks \
  --stack-name "$stack_id" \
  --query "Stacks[0].Outputs[?OutputKey=='PusherHost'].OutputValue" \
  --output text)"
enforce_deadline

MINCO_REALTIME_PROOF_WS_HOST="$ws_host" \
  npm --prefix "$proof_root/browser" exec -- \
  playwright test --config playwright.aws.config.mjs
enforce_deadline

echo "authority=account-role-region-profile-source-duration-spend-cleanup-verified"
echo "source_revision=$source_revision"
echo "artifact_sha256=$artifact_sha256"
echo "artifact_version=$code_version"
echo "provider_deployment=passed"
echo "browser_runtime=passed"
echo "cleanup=scheduled_by_exit_trap"
