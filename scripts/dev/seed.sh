#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

[[ "$#" -eq 1 ]] || {
  echo "usage: $0 <reference|demo|test|bootstrap>" >&2
  exit 2
}
profile="$1"
case "$profile" in
  reference|demo|test|bootstrap) ;;
  *)
    echo "unsupported seed profile: $profile" >&2
    exit 2
    ;;
esac

case "${DATABASE_KIND:-}" in
  postgres)
    selected_set="orders-postgres-seeds"
    : "${DATABASE_URL:?DATABASE_URL is required for PostgreSQL seeds}"
    seed_database_url="$DATABASE_URL"
    ;;
  sqlite)
    selected_set="orders-sqlite-seeds"
    : "${SQLITE_PATH:?SQLITE_PATH is required for SQLite seeds}"
    seed_database_url="sqlite://${SQLITE_PATH}"
    ;;
  *)
    echo "DATABASE_KIND must be postgres or sqlite for seeds" >&2
    exit 2
    ;;
esac

environment="${MINCO_DEV_ENVIRONMENT_CLASS:-local}"
export MINCO_DEV_SEED_DATABASE_URL="$seed_database_url"
mkdir -p target/minco/dev
seed_plan="$(mktemp target/minco/dev/seed-plan.XXXXXX)"
trap 'rm -f "$seed_plan"' EXIT

cargo minco --json db seed \
  --profile "$profile" \
  --environment "$environment" \
  --set "$selected_set" \
  --dry-run >"$seed_plan"
plan_digest="$(
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["digest"])' \
    "$seed_plan"
)"
receipt="target/minco/dev/seed-${profile}-$(date -u +%Y%m%dt%H%M%Sz)-$$.json"

cargo minco --json db seed \
  --profile "$profile" \
  --environment "$environment" \
  --set "$selected_set" \
  --database-url-env MINCO_DEV_SEED_DATABASE_URL \
  --expected-plan-digest "$plan_digest" \
  --receipt "$receipt"
