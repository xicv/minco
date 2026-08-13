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
audit_table_name="minco-orders-audit-$smoke_suffix"
created_tables=()

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
  local target_table="$1"
  ! aws_local dynamodb describe-table --table-name "$target_table" >/dev/null 2>&1
}

delete_tables() {
  local target_table
  for target_table in "${created_tables[@]}"; do
    aws_local dynamodb delete-table --table-name "$target_table" >/dev/null 2>&1 || true
    for _attempt in {1..30}; do
      if table_absent "$target_table"; then
        break
      fi
      sleep 0.2
    done
    table_absent "$target_table" || return 1
  done
  created_tables=()
}

cleanup() {
  set +e
  delete_tables
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
created_tables+=("$table_name")

aws_local dynamodb create-table \
  --table-name "$audit_table_name" \
  --billing-mode PAY_PER_REQUEST \
  --attribute-definitions \
    AttributeName=pk,AttributeType=S \
    AttributeName=sk,AttributeType=S \
  --key-schema \
    AttributeName=pk,KeyType=HASH \
    AttributeName=sk,KeyType=RANGE >/dev/null
created_tables+=("$audit_table_name")

wait_active() {
  local target_table="$1"
  local active=false
  local status
  for _attempt in {1..30}; do
    status="$(
      aws_local dynamodb describe-table \
        --table-name "$target_table" \
        --query 'Table.TableStatus' \
        --output text 2>/dev/null || true
    )"
    if [[ "$status" == "ACTIVE" ]]; then
      active=true
      break
    fi
    sleep 0.2
  done
  [[ "$active" == true ]]
}

wait_active "$table_name" || {
  echo "Rustack Orders table did not become ACTIVE within 6 seconds" >&2
  exit 1
}
wait_active "$audit_table_name" || {
  echo "Rustack audit table did not become ACTIVE within 6 seconds" >&2
  exit 1
}

export MINCO_ORDERS_TEST_DYNAMODB_TABLE="$table_name"
export MINCO_ORDERS_TEST_DYNAMODB_AUDIT_TABLE="$audit_table_name"
export MINCO_ORDERS_TEST_DYNAMODB_ENDPOINT="$AWS_ENDPOINT_URL"

cargo test -p orders-adapters --features dynamodb --test dynamodb --locked \
  all_orders_ports_preserve_idempotency_sort_cursor_revision_and_soft_delete \
  -- --ignored --exact
cargo test -p orders-adapters --features dynamodb --test dynamodb --locked \
  audited_orders_actions_are_atomic_queryable_and_race_safe \
  -- --ignored --exact

delete_tables
table_absent "$table_name"
table_absent "$audit_table_name"

printf '%s\n' \
  "Rustack DynamoDB conformance passed: Orders plus audit cross-table transactions, strong reads, sharded queries, conditional races, soft delete, history, and absence-verified cleanup"
