#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws base64 curl jq shasum xxd; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_RELEASE_MANIFEST:?set MINCO_RELEASE_MANIFEST}"
: "${MINCO_SMOKE_JWT_TOKEN:?set MINCO_SMOKE_JWT_TOKEN}"
initialize_cloud_journal

api_url="$(<"$MINCO_AWS_EVIDENCE_DIR/api-url.txt")"
function_name="$(<"$MINCO_AWS_EVIDENCE_DIR/function-name.txt")"
authorization_header="$(mktemp /tmp/minco-smoke-authorization.XXXXXX)"
chmod 600 "$authorization_header"
printf 'Authorization: Bearer %s\n' "$MINCO_SMOKE_JWT_TOKEN" >"$authorization_header"
cleanup_authorization_header() {
  rm -f "$authorization_header"
}
trap cleanup_authorization_header EXIT INT TERM
artifact="$(jq -er '.artifact.path' "$MINCO_RELEASE_MANIFEST")"
expected_code_sha="$(
  shasum -a 256 "$artifact" | awk '{print $1}' | xxd -r -p | base64
)"
actual_code_sha="$(
  aws_logged lambda get-function-configuration \
    "verify deployed function $function_name runtime, architecture and artifact digest" \
    --function-name "$function_name" \
    --query CodeSha256 \
    --output text
)"
[[ "$actual_code_sha" == "$expected_code_sha" ]] || {
  echo "deployed Lambda digest does not match the release artifact" >&2
  exit 1
}
aws_logged lambda get-function-configuration \
  "retain runtime configuration evidence for $function_name" \
  --function-name "$function_name" \
  --query '{FunctionName:FunctionName,Runtime:Runtime,Architectures:Architectures,MemorySize:MemorySize,Timeout:Timeout,CodeSha256:CodeSha256,LastUpdateStatus:LastUpdateStatus}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/lambda.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/lambda.json"
jq -e '
  .Runtime == "provided.al2023"
  and .Architectures == ["arm64"]
  and .LastUpdateStatus == "Successful"
' "$MINCO_AWS_EVIDENCE_DIR/lambda.json" >/dev/null

http_call() {
  local method="$1"
  local path="$2"
  local expected="$3"
  shift 3
  local response="$MINCO_AWS_EVIDENCE_DIR/http-response.json"
  record_cloud_touch "aws:execute-api" "$method $path" "synthetic bounded smoke; credentials and token redacted"
  local status
  status="$(
    curl \
      --silent \
      --show-error \
      --max-time 30 \
      --output "$response" \
      --write-out '%{http_code}' \
      --request "$method" \
      "$api_url$path" \
      "$@"
  )"
  [[ "$status" == "$expected" ]] || {
    printf '%s %s returned %s, expected %s\n' "$method" "$path" "$status" "$expected" >&2
    jq -c . "$response" >&2 2>/dev/null || true
    return 1
  }
}

http_call GET /health/live 200
jq -e '.live == true and .service == "minco-orders"' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
http_call GET /health/ready 200
jq -e '.ready == true' "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
http_call GET /orders/00000000-0000-0000-0000-000000000000 401

idempotency_key="minco-smoke-$MINCO_AWS_RUN_ID"
customer_reference="MINCO-SMOKE-$MINCO_AWS_RUN_ID"
body="$MINCO_AWS_EVIDENCE_DIR/place-order.json"
jq -n \
  --arg customer_reference "$customer_reference" \
  '{
    customerReference: $customer_reference,
    lines: [{sku: "MINCO-SMOKE-SKU", quantity: 1}]
  }' >"$body"
chmod 600 "$body"
http_call POST /orders 201 \
  --header "@$authorization_header" \
  --header "Content-Type: application/json" \
  --header "Idempotency-Key: $idempotency_key" \
  --data-binary "@$body"
order_id="$(jq -er '.order.id' "$MINCO_AWS_EVIDENCE_DIR/http-response.json")"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/order-id.txt" "$order_id"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/idempotency-key.txt" "$idempotency_key"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/customer-reference.txt" "$customer_reference"

http_call GET "/orders/$order_id" 200 \
  --header "@$authorization_header"
jq -e \
  --arg id "$order_id" \
  --arg customer_reference "$customer_reference" \
  '.id == $id and .customerReference == $customer_reference' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
http_call POST /orders 200 \
  --header "@$authorization_header" \
  --header "Content-Type: application/json" \
  --header "Idempotency-Key: $idempotency_key" \
  --data-binary "@$body"
jq -e \
  --arg id "$order_id" \
  '.replayed == true and .order.id == $id' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null

rm -f "$body" "$MINCO_AWS_EVIDENCE_DIR/http-response.json"
unset MINCO_SMOKE_JWT_TOKEN
printf 'AWS smoke passed: live, ready, auth rejection, place, get and idempotent replay\n'
