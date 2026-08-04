#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fixture_dir="$(mktemp -d)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

fake_bin="$fixture_dir/bin"
evidence_dir="$fixture_dir/evidence"
mkdir -p "$fake_bin" "$evidence_dir"

artifact="$fixture_dir/bootstrap.zip"
printf 'bounded-smoke-artifact\n' >"$artifact"
artifact_digest="$(shasum -a 256 "$artifact" | awk '{print $1}')"
code_sha="$({ printf '%s' "$artifact_digest" | xxd -r -p | base64; } | tr -d '\n')"
jq -n \
  --arg artifact "$artifact" \
  --arg digest "$artifact_digest" \
  '{
    artifacts: [{
      function_id: "api",
      file: {path: $artifact, sha256: $digest}
    }]
  }' >"$fixture_dir/release.json"

cat >"$fake_bin/aws" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
jq -n \
  --arg code_sha "$MINCO_FAKE_CODE_SHA" \
  '{
    FunctionName: "minco-contract-test",
    Runtime: "provided.al2023",
    Architectures: ["arm64"],
    MemorySize: 512,
    Timeout: 30,
    CodeSha256: $code_sha,
    LastUpdateStatus: "Successful",
    Version: "1",
    RevisionId: "synthetic-revision"
  }'
EOF
chmod 755 "$fake_bin/aws"

cat >"$fake_bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=
headers=
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    --dump-header)
      headers="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
[[ -n "$output" && -n "$headers" ]]

counter=0
[[ ! -f "$MINCO_FAKE_CURL_COUNTER" ]] || counter="$(<"$MINCO_FAKE_CURL_COUNTER")"
counter=$((counter + 1))
printf '%s\n' "$counter" >"$MINCO_FAKE_CURL_COUNTER"
printf 'HTTP/2 200\r\nx-amzn-requestid: synthetic-request-%s\r\n\r\n' "$counter" >"$headers"

case "$counter" in
  1)
    printf '{"live":true,"service":"minco-orders"}\n' >"$output"
    printf 200
    ;;
  2)
    printf '{"ready":true}\n' >"$output"
    printf 200
    ;;
  3)
    printf '{"code":"unauthorized"}\n' >"$output"
    printf 401
    ;;
  4|5)
    jq -n '{data:{createdAt:"2026-08-04T00:00:00Z",customerReference:"MINCO-SMOKE-phase-test",id:"00000000-0000-0000-0000-000000000001",lines:[{quantity:1,sku:"MINCO-SMOKE-SKU"}],revision:1,status:"accepted",updatedAt:"2026-08-04T00:00:00Z"}}' >"$output"
    [[ "$counter" -eq 4 ]] && printf 201 || printf 200
    ;;
  6)
    customer_reference=MINCO-SMOKE-phase-test
    if [[ "${MINCO_FAKE_REPLAY_MISMATCH:-false}" == true ]]; then
      customer_reference=MINCO-SMOKE-wrong-phase
    fi
    jq -n --arg reference "$customer_reference" '{data:{createdAt:"2026-08-04T00:00:00Z",customerReference:$reference,id:"00000000-0000-0000-0000-000000000001",lines:[{quantity:1,sku:"MINCO-SMOKE-SKU"}],revision:1,status:"accepted",updatedAt:"2026-08-04T00:00:00Z"}}' >"$output"
    printf 200
    ;;
  *)
    exit 99
    ;;
esac
EOF
chmod 755 "$fake_bin/curl"

run_smoke() {
  local observation="$1"
  local replay_mismatch="$2"
  rm -f "$fixture_dir/curl-counter" "$observation"
  PATH="$fake_bin:$PATH" \
    AWS_REGION=ap-southeast-2 \
    MINCO_AWS_EVIDENCE_DIR="$evidence_dir" \
    MINCO_AWS_RUN_ID=contract-test \
    MINCO_CANDIDATE_API_URL=https://example.invalid/candidate \
    MINCO_FAKE_CODE_SHA="$code_sha" \
    MINCO_FAKE_CURL_COUNTER="$fixture_dir/curl-counter" \
    MINCO_FAKE_REPLAY_MISMATCH="$replay_mismatch" \
    MINCO_FUNCTION_NAME=minco-contract-test \
    MINCO_HOSTED_OBSERVATION="$observation" \
    MINCO_RELEASE_MANIFEST="$fixture_dir/release.json" \
    MINCO_SMOKE_DATA_ID=phase-test \
    MINCO_SMOKE_JWT_TOKEN=synthetic-token \
    bash scripts/aws/smoke.sh >/dev/null
}

observation="$fixture_dir/hosted-observation.json"
run_smoke "$observation" false
jq -e '
  .checks | length == 5
  and all(.passed == true)
  and ([.[].kind] | sort) ==
    (["artifact_identity", "authentication", "contract", "readiness", "smoke"] | sort)
' "$observation" >/dev/null

if run_smoke "$fixture_dir/mismatched-observation.json" true 2>"$fixture_dir/mismatch.err"; then
  echo "hosted smoke accepted a replay response that differed from the created document" >&2
  exit 1
fi
[[ ! -e "$fixture_dir/mismatched-observation.json" ]] || {
  echo "failed hosted smoke published an observation" >&2
  exit 1
}

printf 'Hosted smoke response contract checks passed.\n'
