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
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert set(body) == {"data"} and body["data"]["customerReference"] == "E2E-1" and body["data"]["status"] == "accepted", body' "$response"
order_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["data"]["id"])' "$response")"
created_etag="$(python3 -c 'import sys; headers=(line.split(":", 1) for line in open(sys.argv[1]) if ":" in line); values={name.lower(): value.strip() for name,value in headers}; print(values["etag"])' "$response_headers")"
grep -Fqi "location: /orders/$order_id" "$response_headers"
retrieved="$tmp/retrieved.json"
retrieved_headers="$tmp/retrieved.headers"
curl --silent --show-error --fail --dump-header "$retrieved_headers" --output "$retrieved" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.read' \
  "$base_url/orders/$order_id"
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert set(body) == {"data"} and body["data"]["id"] == sys.argv[2] and body["data"]["customerReference"] == "E2E-1", body' "$retrieved" "$order_id"
grep -Fqi "etag: $created_etag" "$retrieved_headers"

collection="$tmp/collection.json"
curl --silent --show-error --fail --globoff --output "$collection" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.read' \
  "$base_url/orders?page%5Blimit%5D=1&sort=-createdAt,-id&filter%5Bstatus%5D=accepted"
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert set(body) == {"data", "page"} and len(body["data"]) == 1 and body["data"][0]["id"] == sys.argv[2] and body["page"] == {"hasMore": False, "nextCursor": None}, body' "$collection" "$order_id"

replay_headers="$tmp/replay.headers"
replay_status="$(curl --silent --show-error --dump-header "$replay_headers" --output "$tmp/replay.json" --write-out '%{http_code}' \
  -X POST "$base_url/orders" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: e2e-order-1' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.create,orders.read' \
  --data '{"customerReference":"E2E-1","lines":[{"sku":"SKU-1","quantity":2}]}')"
[[ "$replay_status" == 200 ]] || { cat "$tmp/replay.json" >&2; exit 1; }
python3 -c 'import json,sys; replay=json.load(open(sys.argv[1])); original=json.load(open(sys.argv[2])); assert replay == original, (replay, original)' "$tmp/replay.json" "$response"
grep -Fqi "etag: $created_etag" "$replay_headers"
grep -Fqi "location: /orders/$order_id" "$replay_headers"

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

update_headers="$tmp/update.headers"
update_status="$(curl --silent --show-error --dump-header "$update_headers" --output "$tmp/update.json" --write-out '%{http_code}' \
  -X PATCH "$base_url/orders/$order_id" \
  -H 'Content-Type: application/json' \
  -H "If-Match: $created_etag" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.update' \
  --data '{"customerReference":"E2E-UPDATED"}')"
[[ "$update_status" == 200 ]] || { cat "$tmp/update.json" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert set(body) == {"data"} and body["data"]["id"] == sys.argv[2] and body["data"]["customerReference"] == "E2E-UPDATED" and body["data"]["revision"] == 2, body' "$tmp/update.json" "$order_id"
updated_etag="$(python3 -c 'import sys; headers=(line.split(":", 1) for line in open(sys.argv[1]) if ":" in line); values={name.lower(): value.strip() for name,value in headers}; print(values["etag"])' "$update_headers")"
[[ "$updated_etag" != "$created_etag" ]]

stale_status="$(curl --silent --show-error --output "$tmp/stale.json" --write-out '%{http_code}' \
  -X PATCH "$base_url/orders/$order_id" \
  -H 'Content-Type: application/json' \
  -H "If-Match: $created_etag" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.update' \
  --data '{"customerReference":"E2E-STALE"}')"
[[ "$stale_status" == 412 ]] || { cat "$tmp/stale.json" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["code"] == "precondition_failed", body' "$tmp/stale.json"

delete_status="$(curl --silent --show-error --output "$tmp/delete.body" --write-out '%{http_code}' \
  -X DELETE "$base_url/orders/$order_id" \
  -H "If-Match: $updated_etag" \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.delete')"
[[ "$delete_status" == 204 ]] || { cat "$tmp/delete.body" >&2; exit 1; }
[[ ! -s "$tmp/delete.body" ]]

gone_status="$(curl --silent --show-error --output "$tmp/gone.json" --write-out '%{http_code}' \
  -H 'X-Minco-Subject: e2e-user' \
  -H 'X-Minco-Permissions: orders.read' \
  "$base_url/orders/$order_id")"
[[ "$gone_status" == 404 ]] || { cat "$tmp/gone.json" >&2; exit 1; }
python3 -c 'import json,sys; body=json.load(open(sys.argv[1])); assert body["code"] == "not_found", body' "$tmp/gone.json"

printf 'Orders E2E passed on port %s.\n' "$port"
