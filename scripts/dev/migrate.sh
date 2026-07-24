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

exec cargo minco db migrate
