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
: "${MINCO_HOSTED_OBSERVATION:?set MINCO_HOSTED_OBSERVATION}"
: "${MINCO_SMOKE_JWT_TOKEN:?set MINCO_SMOKE_JWT_TOKEN}"
: "${MINCO_CANDIDATE_API_URL:?set MINCO_CANDIDATE_API_URL from the current stack output}"
: "${MINCO_FUNCTION_NAME:?set MINCO_FUNCTION_NAME from the current stack output}"
: "${MINCO_SMOKE_DATA_ID:=$MINCO_AWS_RUN_ID}"
require_safe_name MINCO_SMOKE_DATA_ID "$MINCO_SMOKE_DATA_ID"
initialize_cloud_journal

api_url="${MINCO_CANDIDATE_API_URL%/}"
function_name="$MINCO_FUNCTION_NAME"
authorization_header="$(mktemp /tmp/minco-smoke-authorization.XXXXXX)"
chmod 600 "$authorization_header"
printf 'Authorization: Bearer %s\n' "$MINCO_SMOKE_JWT_TOKEN" >"$authorization_header"
cleanup_authorization_header() {
  rm -f "$authorization_header"
}
trap cleanup_authorization_header EXIT INT TERM
artifact="$(
  jq -er '
    [.artifacts[] | select(.function_id == "api")]
    | if length == 1
      then .[0].file.path
      else error("release must contain exactly one api artifact")
      end
  ' "$MINCO_RELEASE_MANIFEST"
)"
artifact_digest="$(
  jq -er '
    [.artifacts[] | select(.function_id == "api")]
    | if length == 1
      then .[0].file.sha256
      else error("release must contain exactly one api artifact")
      end
  ' "$MINCO_RELEASE_MANIFEST"
)"
expected_code_sha="$(
  shasum -a 256 "$artifact" | awk '{print $1}' | xxd -r -p | base64
)"
aws_logged lambda get-function-configuration \
  "verify candidate function $function_name runtime, version, architecture and artifact digest" \
  --function-name "$function_name" \
  --qualifier candidate \
  --query '{FunctionName:FunctionName,Runtime:Runtime,Architectures:Architectures,MemorySize:MemorySize,Timeout:Timeout,CodeSha256:CodeSha256,LastUpdateStatus:LastUpdateStatus,Version:Version,RevisionId:RevisionId}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/lambda.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/lambda.json"
jq -e '
  .Runtime == "provided.al2023"
  and .Architectures == ["arm64"]
  and .LastUpdateStatus == "Successful"
  and (.Version | test("^[1-9][0-9]*$"))
' "$MINCO_AWS_EVIDENCE_DIR/lambda.json" >/dev/null
actual_code_sha="$(jq -er '.CodeSha256' "$MINCO_AWS_EVIDENCE_DIR/lambda.json")"
executed_version="$(jq -er '.Version' "$MINCO_AWS_EVIDENCE_DIR/lambda.json")"
[[ "$actual_code_sha" == "$expected_code_sha" ]] || {
  echo "deployed candidate Lambda digest does not match the release artifact" >&2
  exit 1
}

http_call() {
  local method="$1"
  local path="$2"
  local expected="$3"
  shift 3
  local response="$MINCO_AWS_EVIDENCE_DIR/http-response.json"
  local headers="$MINCO_AWS_EVIDENCE_DIR/http-response.headers"
  record_cloud_touch "aws:execute-api" "$method $path" "synthetic bounded smoke; credentials and token redacted"
  last_http_status="$(
    curl \
      --silent \
      --show-error \
      --max-time 30 \
      --dump-header "$headers" \
      --output "$response" \
      --write-out '%{http_code}' \
      --request "$method" \
      "$api_url$path" \
      "$@"
  )"
  [[ "$last_http_status" == "$expected" ]] || {
    printf '%s %s returned %s, expected %s\n' \
      "$method" "$path" "$last_http_status" "$expected" >&2
    return 1
  }
  last_request_id="$(http_response_request_id "$headers")"
  [[ -n "$last_request_id" ]] || {
    echo "$method $path did not return a request ID" >&2
    return 1
  }
}

http_call GET /health/live 200
jq -e '.live == true and .service == "minco-orders"' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
contract_request_id="$last_request_id"
http_call GET /health/ready 200
jq -e '.ready == true' "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
readiness_request_id="$last_request_id"
http_call GET /orders/00000000-0000-0000-0000-000000000000 401
authentication_request_id="$last_request_id"

idempotency_key="minco-smoke-$MINCO_SMOKE_DATA_ID"
customer_reference="MINCO-SMOKE-$MINCO_SMOKE_DATA_ID"
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
created_order_document="$(
  jq -cer '.data' "$MINCO_AWS_EVIDENCE_DIR/http-response.json"
)"
order_id="$(jq -er '.data.id' "$MINCO_AWS_EVIDENCE_DIR/http-response.json")"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/order-id.txt" "$order_id"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/idempotency-key.txt" "$idempotency_key"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/customer-reference.txt" "$customer_reference"

http_call GET "/orders/$order_id" 200 \
  --header "@$authorization_header"
jq -e \
  --arg id "$order_id" \
  --arg customer_reference "$customer_reference" \
  '.data.id == $id and .data.customerReference == $customer_reference' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
http_call POST /orders 200 \
  --header "@$authorization_header" \
  --header "Content-Type: application/json" \
  --header "Idempotency-Key: $idempotency_key" \
  --data-binary "@$body"
jq -e \
  --argjson created_order_document "$created_order_document" \
  '.data == $created_order_document' \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" >/dev/null
smoke_request_id="$last_request_id"

jq -n \
  --arg endpoint "$api_url" \
  --arg artifact_digest "$artifact_digest" \
  --arg executed_version "$executed_version" \
  --arg contract_request_id "$contract_request_id" \
  --arg readiness_request_id "$readiness_request_id" \
  --arg authentication_request_id "$authentication_request_id" \
  --arg smoke_request_id "$smoke_request_id" \
  '{
    endpoint: $endpoint,
    executed_artifact_digest: $artifact_digest,
    executed_version: $executed_version,
    checks: [
      {
        kind: "contract",
        passed: true,
        request_id: $contract_request_id,
        status_code: 200
      },
      {
        kind: "readiness",
        passed: true,
        request_id: $readiness_request_id,
        status_code: 200
      },
      {
        kind: "authentication",
        passed: true,
        request_id: $authentication_request_id,
        status_code: 401
      },
      {
        kind: "smoke",
        passed: true,
        request_id: $smoke_request_id,
        status_code: 200
      },
      {
        kind: "artifact_identity",
        passed: true,
        request_id: null,
        status_code: null
      }
    ]
  }' >"$MINCO_HOSTED_OBSERVATION"
chmod 600 "$MINCO_HOSTED_OBSERVATION"
rm -f \
  "$body" \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.json" \
  "$MINCO_AWS_EVIDENCE_DIR/http-response.headers"
unset MINCO_SMOKE_JWT_TOKEN
printf 'AWS smoke passed: live, ready, auth rejection, place, get and idempotent replay\n'
