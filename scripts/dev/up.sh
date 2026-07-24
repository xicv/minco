#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=true
  shift
fi
[[ "$#" -eq 0 ]] || { echo "usage: $0 [--dry-run]" >&2; exit 2; }

topology="$(python3 scripts/dev/topology.py)"
rustack_services="$(
  printf '%s' "$topology" |
    python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["aws_services"]))'
)"
compose_services="$(
  printf '%s' "$topology" |
    python3 -c 'import json,sys; print(" ".join(json.load(sys.stdin)["compose_services"]))'
)"
[[ -n "$compose_services" ]] || { echo 'The selected graph has no local services.' >&2; exit 1; }

if [[ "$dry_run" == true ]]; then
  printf 'MINCO_RUSTACK_SERVICES=%s\n' "$rustack_services"
  printf 'docker compose -f infra/local/compose.yaml up -d --wait %s\n' "$compose_services"
  exit 0
fi

command -v docker >/dev/null || { echo 'Docker is required' >&2; exit 1; }
if [[ "$compose_services" == *rustack* ]]; then
  command -v aws >/dev/null || { echo 'AWS CLI is required to verify Rustack' >&2; exit 1; }
fi
while IFS= read -r assignment; do
  [[ -n "$assignment" ]] || continue
  export "${assignment?}"
done < <(python3 scripts/dev/topology.py --format env)
export MINCO_RUSTACK_SERVICES="$rustack_services"
# The service list is generated from fixed Compose service identifiers.
# shellcheck disable=SC2086
docker compose -f infra/local/compose.yaml up -d --wait $compose_services
if [[ "$compose_services" == *postgres* ]]; then
  printf 'PostgreSQL: %s\n' "$DATABASE_URL"
fi
if [[ "$compose_services" == *rustack* ]]; then
  rustack_ready=false
  for _attempt in {1..30}; do
    if aws sts get-caller-identity >/dev/null 2>&1; then
      rustack_ready=true
      break
    fi
    sleep 0.2
  done
  [[ "$rustack_ready" == true ]] || {
    echo 'Rustack did not answer STS within 6 seconds' >&2
    exit 1
  }
  printf 'Rustack (%s): %s\n' "$rustack_services" "$AWS_ENDPOINT_URL"
fi
