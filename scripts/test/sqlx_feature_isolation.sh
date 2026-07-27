#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repository_root}"

require_contains() {
  local tree=$1
  local needle=$2
  local description=$3
  if ! grep -Fq -- "${needle}" <<<"${tree}"; then
    printf 'SQLx feature isolation failed: %s did not contain %s.\n' \
      "${description}" "${needle}" >&2
    exit 1
  fi
}

require_absent() {
  local tree=$1
  local needle=$2
  local description=$3
  if grep -Fq -- "${needle}" <<<"${tree}"; then
    printf 'SQLx feature isolation failed: %s unexpectedly contained %s.\n' \
      "${description}" "${needle}" >&2
    exit 1
  fi
}

feedback_postgres=$(cargo tree --locked \
  -p minco-plugin-feedback \
  --no-default-features \
  --features postgres)
require_contains "${feedback_postgres}" "sqlx-postgres" "Feedback PostgreSQL graph"
require_absent "${feedback_postgres}" "sqlx-sqlite" "Feedback PostgreSQL graph"
require_absent "${feedback_postgres}" "libsqlite3-sys" "Feedback PostgreSQL graph"

feedback_sqlite=$(cargo tree --locked \
  -p minco-plugin-feedback \
  --no-default-features \
  --features sqlite)
require_contains "${feedback_sqlite}" "sqlx-sqlite" "Feedback SQLite graph"
require_contains "${feedback_sqlite}" "libsqlite3-sys" "Feedback SQLite graph"
require_absent "${feedback_sqlite}" "sqlx-postgres" "Feedback SQLite graph"

postgres_extension=$(cargo tree --locked -p minco-sqlx-postgres)
require_contains "${postgres_extension}" "sqlx-postgres" "PostgreSQL extension graph"
require_absent "${postgres_extension}" "sqlx-sqlite" "PostgreSQL extension graph"
require_absent "${postgres_extension}" "libsqlite3-sys" "PostgreSQL extension graph"

sqlite_extension=$(cargo tree --locked -p minco-sqlx-sqlite)
require_contains "${sqlite_extension}" "sqlx-sqlite" "SQLite extension graph"
require_contains "${sqlite_extension}" "libsqlite3-sys" "SQLite extension graph"
require_absent "${sqlite_extension}" "sqlx-postgres" "SQLite extension graph"

orders_postgres=$(cargo tree --locked \
  -p orders-adapters \
  --no-default-features \
  --features postgres)
require_contains "${orders_postgres}" "sqlx-postgres" "Orders PostgreSQL graph"
require_absent "${orders_postgres}" "sqlx-sqlite" "Orders PostgreSQL graph"
require_absent "${orders_postgres}" "libsqlite3-sys" "Orders PostgreSQL graph"

orders_sqlite=$(cargo tree --locked \
  -p orders-adapters \
  --no-default-features \
  --features sqlite)
require_contains "${orders_sqlite}" "sqlx-sqlite" "Orders SQLite graph"
require_contains "${orders_sqlite}" "libsqlite3-sys" "Orders SQLite graph"
require_absent "${orders_sqlite}" "sqlx-postgres" "Orders SQLite graph"

printf '%s\n' 'SQLx PostgreSQL and SQLite feature graphs are isolated.'
