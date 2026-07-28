#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

topology="$(python3 scripts/dev/topology.py)"
while IFS= read -r assignment; do
  [[ -n "$assignment" ]] || continue
  export "${assignment?}"
done < <(
  printf '%s' "$topology" |
    python3 -c 'import json,sys; topology=json.load(sys.stdin); [print(f"{key}={value}") for key,value in sorted(topology["environment"].items())]'
)

set -a
if [[ -f .env ]]; then
  # shellcheck source=/dev/null
  source .env
fi
set +a

command -v jq >/dev/null || {
  echo "jq is required to bind the reviewed local migration plan" >&2
  exit 1
}
: "${MIGRATION_DATABASE_URL:?MIGRATION_DATABASE_URL is required}"

migration_plan="target/minco/dev/orders-postgres-plan.json"
mkdir -p "$(dirname "$migration_plan")"
cargo minco db plan --set orders-postgres --json >"$migration_plan"
migration_digest="$(jq -er '.digest' "$migration_plan")"
migration_receipt="target/minco/dev/orders-postgres-$(date -u +%Y%m%dt%H%M%Sz)-$$.json"

exec cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MIGRATION_DATABASE_URL \
  --expected-plan-digest "$migration_digest" \
  --receipt "$migration_receipt" \
  --json
