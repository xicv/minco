#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v docker >/dev/null || { echo 'Docker is required' >&2; exit 1; }

selection_root="${MINCO_PROJECT_ROOT:-$PWD}"
manifest="$selection_root/minco.toml"
[[ -f "$manifest" ]] || { echo "Minco graph not found: $manifest" >&2; exit 1; }

compose_services="$(
  python3 scripts/dev/local_services.py "$manifest" --field compose
)"
export MINCO_RUSTACK_SERVICES="$(
  python3 scripts/dev/local_services.py "$manifest" --field aws
)"

selected=()
[[ -z "$compose_services" ]] || read -r -a selected <<<"$compose_services"
unselected=()
for service in postgres rustack; do
  if [[ " ${selected[*]} " != *" $service "* ]]; then
    unselected+=("$service")
  fi
done
if [[ ${#unselected[@]} -gt 0 ]]; then
  docker compose -f infra/local/compose.yaml stop "${unselected[@]}" >/dev/null
fi

if [[ ${#selected[@]} -eq 0 ]]; then
  printf 'No local services are required by enabled plugins in %s\n' "$manifest"
  exit 0
fi

docker compose -f infra/local/compose.yaml up -d --wait "${selected[@]}"
if [[ " ${selected[*]} " == *" postgres "* ]]; then
  printf '%s\n' 'PostgreSQL: postgres://minco:minco@127.0.0.1:55432/minco_orders'
fi
if [[ " ${selected[*]} " == *" rustack "* ]]; then
  printf 'Rustack:    http://127.0.0.1:%s (services: %s)\n' \
    "${MINCO_RUSTACK_PORT:-4566}" "$MINCO_RUSTACK_SERVICES"
fi
