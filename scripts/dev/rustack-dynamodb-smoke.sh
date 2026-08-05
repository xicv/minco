#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

for command in aws cargo docker python3; do
  command -v "$command" >/dev/null || {
    printf '%s is required for Rustack DynamoDB conformance\n' "$command" >&2
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
table_name="minco-orders-$smoke_suffix"
table_created=false

export COMPOSE_PROJECT_NAME="minco-rustack-dynamodb-$$"
export MINCO_RUSTACK_PORT="$smoke_port"
export MINCO_RUSTACK_SERVICES="dynamodb,sts"
export AWS_ACCESS_KEY_ID="test"
export AWS_SECRET_ACCESS_KEY="test"
export AWS_DEFAULT_REGION="ap-southeast-2"
export AWS_EC2_METADATA_DISABLED="true"
export AWS_ENDPOINT_URL="http://127.0.0.1:$smoke_port"
export AWS_MAX_ATTEMPTS="1"

aws_local() {
  aws --cli-connect-timeout 1 --cli-read-timeout 3 "$@"
}

table_absent() {
  ! aws_local dynamodb describe-table --table-name "$table_name" >/dev/null 2>&1
}

delete_table() {
  if [[ "$table_created" == true ]]; then
    aws_local dynamodb delete-table --table-name "$table_name" >/dev/null 2>&1 || true
    for _attempt in {1..30}; do
      if table_absent; then
        table_created=false
        return 0
      fi
      sleep 0.2
    done
    return 1
  fi
}

cleanup() {
  set +e
  delete_table
  docker compose -f infra/local/compose.yaml down --remove-orphans >/dev/null 2>&1
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

aws_local dynamodb create-table \
  --table-name "$table_name" \
  --billing-mode PAY_PER_REQUEST \
  --attribute-definitions \
    AttributeName=pk,AttributeType=S \
    AttributeName=sk,AttributeType=S \
    AttributeName=gsi1pk,AttributeType=S \
    AttributeName=gsi1sk,AttributeType=S \
    AttributeName=gsi2pk,AttributeType=S \
    AttributeName=gsi2sk,AttributeType=S \
    AttributeName=gsi3pk,AttributeType=S \
    AttributeName=gsi3sk,AttributeType=S \
  --key-schema \
    AttributeName=pk,KeyType=HASH \
    AttributeName=sk,KeyType=RANGE \
  --global-secondary-indexes '[
    {
      "IndexName": "orders-by-created-at",
      "KeySchema": [
        {"AttributeName": "gsi1pk", "KeyType": "HASH"},
        {"AttributeName": "gsi1sk", "KeyType": "RANGE"}
      ],
      "Projection": {"ProjectionType": "ALL"}
    },
    {
      "IndexName": "orders-by-created-at-inverted-id",
      "KeySchema": [
        {"AttributeName": "gsi2pk", "KeyType": "HASH"},
        {"AttributeName": "gsi2sk", "KeyType": "RANGE"}
      ],
      "Projection": {"ProjectionType": "ALL"}
    },
    {
      "IndexName": "orders-by-id",
      "KeySchema": [
        {"AttributeName": "gsi3pk", "KeyType": "HASH"},
        {"AttributeName": "gsi3sk", "KeyType": "RANGE"}
      ],
      "Projection": {"ProjectionType": "ALL"}
    }
  ]' >/dev/null
table_created=true

active=false
for _attempt in {1..30}; do
  status="$(
    aws_local dynamodb describe-table \
      --table-name "$table_name" \
      --query 'Table.TableStatus' \
      --output text 2>/dev/null || true
  )"
  if [[ "$status" == "ACTIVE" ]]; then
    active=true
    break
  fi
  sleep 0.2
done
[[ "$active" == true ]] || {
  echo "Rustack DynamoDB table did not become ACTIVE within 6 seconds" >&2
  exit 1
}

export MINCO_ORDERS_TEST_DYNAMODB_TABLE="$table_name"
export MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT="$AWS_ENDPOINT_URL"

cargo test -p orders-adapters --features dynamodb --test dynamodb --locked \
  -- --ignored --exact

delete_table
table_absent

printf '%s\n' \
  "Rustack DynamoDB conformance passed: transactions, strong reads, sharded index queries, conditional updates, soft delete, and absence-verified cleanup"
