#!/usr/bin/env bash
set -euo pipefail

appsync_proof_bucket_versions_are_empty() {
  local payload="${1:-}"
  [[ -n "$payload" ]] || payload='{}'
  jq -e \
    '((.Versions // []) | length == 0) and ((.DeleteMarkers // []) | length == 0)' \
    <<<"$payload" >/dev/null
}
