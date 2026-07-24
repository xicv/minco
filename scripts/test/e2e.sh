#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo >/dev/null || { echo 'Cargo is required' >&2; exit 1; }
command -v curl >/dev/null || { echo 'curl is required' >&2; exit 1; }

port="${MINCO_E2E_PORT:-39090}"
tmp="$(mktemp -d)"
log="$tmp/api.log"
cleanup() {
  if [[ -n "${api_pid:-}" ]]; then kill "$api_pid" 2>/dev/null || true; wait "$api_pid" 2>/dev/null || true; fi
  rm -rf "$tmp"
}
trap cleanup EXIT

APP_ENV=local \
API_HOST=127.0.0.1 \
API_PORT="$port" \
DATABASE_KIND=sqlite \
SQLITE_PATH="$tmp/orders.db" \
DATABASE_MAX_CONNECTIONS=1 \
ALLOW_DEVELOPMENT_HEADERS=true \
ALLOWED_ORIGINS=http://127.0.0.1:5173 \
cargo run -p orders-service --bin orders-local --features sqlite >"$log" 2>&1 &
api_pid=$!

for _ in $(seq 1 120); do
  if curl --silent --fail "http://127.0.0.1:$port/health/live" >/dev/null; then break; fi
  if ! kill -0 "$api_pid" 2>/dev/null; then cat "$log" >&2; exit 1; fi
  sleep 0.25
done
curl --silent --fail "http://127.0.0.1:$port/health/live" >/dev/null || { cat "$log" >&2; exit 1; }

response="$tmp/order.json"
status="$(curl --silent --output "$response" --write-out '%{http_code}' \
  -X POST "http://127.0.0.1:$port/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-1","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$status" == 201 ]] || { cat "$response" >&2; exit 1; }
order_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["order"]["id"])' "$response")"
curl --silent --fail \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.read' \
  "http://127.0.0.1:$port/orders/$order_id" >/dev/null

replay_status="$(curl --silent --output "$tmp/replay.json" --write-out '%{http_code}' \
  -X POST "http://127.0.0.1:$port/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-1","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$replay_status" == 200 ]] || { cat "$tmp/replay.json" >&2; exit 1; }
printf 'Orders E2E passed on port %s.\n' "$port"
