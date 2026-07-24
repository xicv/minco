#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

while IFS= read -r assignment; do
  [[ -n "$assignment" ]] || continue
  export "${assignment?}"
done < <(python3 scripts/dev/topology.py --format env)

set -a
if [[ -f .env ]]; then
  # shellcheck source=/dev/null
  source .env
fi
set +a

exec cargo minco db migrate
