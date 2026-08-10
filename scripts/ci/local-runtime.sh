#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

for command in cargo docker; do
  command -v "$command" >/dev/null || {
    printf '%s is required for local runtime qualification\n' "$command" >&2
    exit 1
  }
done

application="${MINCO_LOCAL_CI_APPLICATION:-minco-local-ci}"
postgres_port="${MINCO_LOCAL_CI_POSTGRES_PORT:-55432}"
rustack_port="${MINCO_LOCAL_CI_RUSTACK_PORT:-54566}"
compose_file="infra/local/compose.yaml"
target_directory="${CARGO_TARGET_DIR:-target}"
minco_binary="$target_directory/debug/cargo-minco"

cargo build --locked -p cargo-minco

stop_service() {
  local service="$1"
  local port="$2"
  shift 2
  "$minco_binary" __local-service stop "$service" \
    --application "$application" \
    --compose-file "$compose_file" \
    --port "$port" \
    "$@"
}

cleanup() {
  set +e
  stop_service rustack "$rustack_port" --aws-services sts >/dev/null 2>&1
  stop_service postgres "$postgres_port" >/dev/null 2>&1
}
trap cleanup EXIT

export MINCO_CONTAINER_RUNTIME=docker

"$minco_binary" __local-service start postgres \
  --application "$application" \
  --compose-file "$compose_file" \
  --port "$postgres_port"

postgres_id="$(
  docker ps -q \
    --filter label=dev.minco.managed=true \
    --filter "label=dev.minco.application=$application" \
    --filter label=dev.minco.service=postgres \
    --filter "publish=$postgres_port"
)"
[[ -n "$postgres_id" && "$postgres_id" != *$'\n'* ]] || {
  printf 'expected exactly one owned PostgreSQL container, got %q\n' "$postgres_id" >&2
  exit 1
}
docker exec "$postgres_id" \
  psql -U minco -d minco_orders -v ON_ERROR_STOP=1 -c 'SELECT 1'

stop_service postgres "$postgres_port"
"$minco_binary" __local-service start postgres \
  --application "$application" \
  --compose-file "$compose_file" \
  --port "$postgres_port"
stop_service postgres "$postgres_port"

"$minco_binary" __local-service start rustack \
  --application "$application" \
  --compose-file "$compose_file" \
  --port "$rustack_port" \
  --aws-services sts
stop_service rustack "$rustack_port" --aws-services sts

printf 'Owned local runtime qualification passed with PostgreSQL and Rustack.\n'
