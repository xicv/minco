#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo >/dev/null || { echo 'Cargo is required' >&2; exit 1; }
command -v curl >/dev/null || { echo 'curl is required' >&2; exit 1; }
command -v python3 >/dev/null || { echo 'Python 3 is required' >&2; exit 1; }

port="${MINCO_E2E_PORT:-$(python3 -c 'import socket; listener = socket.socket(); listener.bind(("127.0.0.1", 0)); print(listener.getsockname()[1]); listener.close()')}"
tmp="$(mktemp -d)"
log="$tmp/api.log"
cleanup() {
  if [[ -n "${api_pid:-}" ]]; then
    kill "$api_pid" 2>/dev/null || true
    wait "$api_pid" 2>/dev/null || true
  fi
  rm -rf -- "$tmp"
}
trap cleanup EXIT

target_dir="${CARGO_TARGET_DIR:-target}"
cargo build -p orders-service --bin orders-local --features sqlite

APP_ENV=local \
API_HOST=127.0.0.1 \
API_PORT="$port" \
DATABASE_KIND=sqlite \
SQLITE_PATH="$tmp/orders.db" \
DATABASE_MAX_CONNECTIONS=1 \
ALLOW_DEVELOPMENT_HEADERS=true \
ALLOWED_ORIGINS=http://127.0.0.1:5173 \
"$target_dir/debug/orders-local" >"$log" 2>&1 &
api_pid=$!

for _ in $(seq 1 120); do
  if curl --silent --fail "http://127.0.0.1:$port/health/live" >/dev/null; then break; fi
  if ! kill -0 "$api_pid" 2>/dev/null; then cat "$log" >&2; exit 1; fi
  sleep 0.25
done
base_url="http://127.0.0.1:$port"
live="$tmp/live.json"
ready="$tmp/ready.json"
curl --silent --show-error --fail "$base_url/health/live" --output "$live" ||
  { cat "$log" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body == {"live": True, "service": "minco-orders"}, body' "$live"
curl --silent --show-error --fail "$base_url/health/ready" --output "$ready" ||
  { cat "$log" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["ready"] is True and body["dependencies"]["orders-store"]["ready"] is True, body' "$ready"

response="$tmp/order.json"
response_headers="$tmp/order.headers"
status="$(curl --silent --show-error --dump-header "$response_headers" --output "$response" --write-out '%{http_code}' \
  -X POST "$base_url/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-1","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$status" == 201 ]] || { cat "$response" >&2; exit 1; }
tr -d '\r' <"$response_headers" | grep -Eiq '^content-type: application/json([;]|$)'
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["replayed"] is False and body["order"]["customerReference"] == "E2E-1" and body["order"]["status"] == "accepted", body' "$response"
order_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["order"]["id"])' "$response")"
retrieved="$tmp/retrieved.json"
curl --silent --show-error --fail --output "$retrieved" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.read' \
  "$base_url/orders/$order_id"
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["id"] == sys.argv[2] and body["customerReference"] == "E2E-1", body' "$retrieved" "$order_id"

replay_status="$(curl --silent --output "$tmp/replay.json" --write-out '%{http_code}' \
  -X POST "$base_url/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-1","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$replay_status" == 200 ]] || { cat "$tmp/replay.json" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["replayed"] is True and body["order"]["id"] == sys.argv[2], body' "$tmp/replay.json" "$order_id"

conflict_headers="$tmp/conflict.headers"
conflict_status="$(curl --silent --show-error --dump-header "$conflict_headers" --output "$tmp/conflict.json" --write-out '%{http_code}' \
  -X POST "$base_url/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-CHANGED","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$conflict_status" == 409 ]] || { cat "$tmp/conflict.json" >&2; exit 1; }
tr -d '\r' <"$conflict_headers" | grep -Eiq '^content-type: application/problem\+json([;]|$)'
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["code"] == "idempotency_conflict", body' "$tmp/conflict.json"

unauthorized_status="$(curl --silent --show-error --output "$tmp/unauthorized.json" --write-out '%{http_code}' \
  "$base_url/orders/$order_id")"
[[ "$unauthorized_status" == 401 ]] || { cat "$tmp/unauthorized.json" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["code"] == "authentication_required", body' "$tmp/unauthorized.json"

printf 'Orders E2E passed on port %s.\n' "$port"
