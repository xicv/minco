#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

for command in aws cargo cmp docker python3; do
  command -v "$command" >/dev/null || {
    printf '%s is required for Rustack conformance\n' "$command" >&2
    exit 1
  }
done

smoke_port="$(
  python3 - <<'PY'
import socket
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
)"
smoke_suffix="$(date -u +%Y%m%d%H%M%S)-$$"
smoke_directory="$(mktemp -d /tmp/minco-rustack-smoke.XXXXXX)"
body_path="$smoke_directory/body.txt"
download_path="$smoke_directory/download.txt"
bucket_name="minco-rustack-$smoke_suffix"
parameter_name="/minco/rustack/$smoke_suffix"
queue_name="minco-rustack-$smoke_suffix"
queue_url=""

export COMPOSE_PROJECT_NAME="minco-rustack-smoke-$$"
export MINCO_RUSTACK_PORT="$smoke_port"
export MINCO_RUSTACK_SERVICES="s3,sqs,ssm,sts"
export AWS_ACCESS_KEY_ID="test"
export AWS_SECRET_ACCESS_KEY="test"
export AWS_DEFAULT_REGION="ap-southeast-2"
export AWS_EC2_METADATA_DISABLED="true"
export AWS_ENDPOINT_URL="http://127.0.0.1:$smoke_port"
export AWS_MAX_ATTEMPTS="1"

aws_local() {
  aws --cli-connect-timeout 1 --cli-read-timeout 2 "$@"
}

cleanup() {
  set +e
  if [[ -n "$queue_url" ]]; then
    aws_local sqs delete-queue --queue-url "$queue_url" >/dev/null 2>&1
  fi
  aws_local ssm delete-parameter --name "$parameter_name" >/dev/null 2>&1
  aws_local s3api delete-object --bucket "$bucket_name" --key proof.txt >/dev/null 2>&1
  aws_local s3api delete-bucket --bucket "$bucket_name" >/dev/null 2>&1
  docker compose -f infra/local/compose.yaml down --remove-orphans >/dev/null 2>&1
  rm -f "$body_path" "$download_path"
  rmdir "$smoke_directory" >/dev/null 2>&1
}
trap cleanup EXIT

docker compose -f infra/local/compose.yaml up -d rustack

ready=false
for _attempt in {1..30}; do
  if aws_local sts get-caller-identity >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 0.2
done
[[ "$ready" == true ]] || {
  docker compose -f infra/local/compose.yaml logs rustack >&2
  echo "Rustack did not answer STS within 6 seconds" >&2
  exit 1
}

account_id="$(aws_local sts get-caller-identity --query Account --output text)"
[[ -n "$account_id" && "$account_id" != "None" ]]

aws_local ssm put-parameter \
  --name "$parameter_name" \
  --type SecureString \
  --value "rustack-ssm-proof" >/dev/null
parameter_value="$(
  aws_local ssm get-parameter \
    --name "$parameter_name" \
    --with-decryption \
    --query 'Parameter.Value' \
    --output text
)"
[[ "$parameter_value" == "rustack-ssm-proof" ]]

cargo test -p minco-aws-lambda --test rustack_ssm --locked \
  secure_parameter_round_trip_uses_the_standard_sdk_endpoint \
  -- --ignored --exact

queue_url="$(
  aws_local sqs create-queue \
    --queue-name "$queue_name" \
    --query QueueUrl \
    --output text
)"
aws_local sqs send-message \
  --queue-url "$queue_url" \
  --message-body "rustack-sqs-proof" >/dev/null
message_body="$(
  aws_local sqs receive-message \
    --queue-url "$queue_url" \
    --max-number-of-messages 1 \
    --wait-time-seconds 1 \
    --query 'Messages[0].Body' \
    --output text
)"
[[ "$message_body" == "rustack-sqs-proof" ]]

printf '%s\n' "rustack-s3-proof" >"$body_path"
aws_local s3api create-bucket \
  --bucket "$bucket_name" \
  --create-bucket-configuration "LocationConstraint=$AWS_DEFAULT_REGION" >/dev/null
aws_local s3api put-object \
  --bucket "$bucket_name" \
  --key proof.txt \
  --body "$body_path" >/dev/null
aws_local s3api get-object \
  --bucket "$bucket_name" \
  --key proof.txt \
  "$download_path" >/dev/null
cmp "$body_path" "$download_path"

printf 'Rustack conformance passed: s3 sqs ssm sts and Minco SSM adapter (account %s)\n' \
  "$account_id"
