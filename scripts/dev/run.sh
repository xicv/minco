#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

print_env=false
if [[ "${1:-}" == "--print-env" ]]; then
  print_env=true
  shift
fi
[[ "$#" -eq 0 ]] || { echo "usage: $0 [--print-env]" >&2; exit 2; }

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

if [[ "$print_env" == true ]]; then
  printf '%s' "$topology" |
    python3 -c 'import json,sys; topology=json.load(sys.stdin); [print(f"{key}={value}") for key,value in sorted(topology["environment"].items())]' |
    while IFS='=' read -r key _; do
      printf '%s=%s\n' "$key" "${!key}"
    done
  exit 0
fi

exec cargo run -p orders-service --bin orders-local --features all-runtimes
