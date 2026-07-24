#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v docker >/dev/null || { echo 'Docker is required' >&2; exit 1; }

all_services="$(
  python3 scripts/dev/local_services.py \
    infra/local/fixtures/postgres-aws/minco.toml --field compose
)"
[[ "$all_services" == "postgres rustack" ]] || {
  printf 'unexpected combined graph resolution: %s\n' "$all_services" >&2
  exit 1
}

export COMPOSE_PROJECT_NAME="minco-rustack-test-${GITHUB_RUN_ID:-local}-$$"
export MINCO_PROJECT_ROOT="$PWD/infra/local/fixtures/aws-ssm"
export MINCO_RUSTACK_PORT="$(
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
)"

cleanup() {
  status=$?
  trap - EXIT
  if [[ $status -ne 0 ]]; then
    docker compose -f infra/local/compose.yaml logs rustack || true
  fi
  docker compose -f infra/local/compose.yaml down --volumes --remove-orphans >/dev/null
  exit "$status"
}
trap cleanup EXIT

./scripts/dev/up.sh

running="$(docker compose -f infra/local/compose.yaml ps --services --status running)"
[[ "$running" == "rustack" ]] || {
  printf 'expected only Rustack to run, got: %s\n' "$running" >&2
  exit 1
}
container_id="$(docker compose -f infra/local/compose.yaml ps -q rustack)"
docker inspect "$container_id" --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | grep -qx 'SERVICES=ssm'

export AWS_EC2_METADATA_DISABLED=true
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_REGION=ap-southeast-2
export AWS_DEFAULT_REGION=ap-southeast-2
export AWS_ENDPOINT_URL="http://127.0.0.1:$MINCO_RUSTACK_PORT"
cargo test -p minco-aws-lambda --test rustack_ssm --locked \
  secure_parameter_round_trip_uses_standard_endpoint_override -- --ignored --exact
