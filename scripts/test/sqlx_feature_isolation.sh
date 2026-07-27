#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repository_root}"

dependency_tree() {
  cargo tree --locked -e normal,build "$@"
}

require_package_contains() {
  local tree=$1
  local package=$2
  local description=$3
  if ! grep -Eq -- "(^|[^[:alnum:]_-])${package} v[0-9]" <<<"${tree}"; then
    printf 'SQLx feature isolation failed: %s did not contain %s.\n' \
      "${description}" "${package}" >&2
    exit 1
  fi
}

require_package_absent() {
  local tree=$1
  local package=$2
  local description=$3
  if grep -Eq -- "(^|[^[:alnum:]_-])${package} v[0-9]" <<<"${tree}"; then
    printf 'SQLx feature isolation failed: %s unexpectedly contained %s.\n' \
      "${description}" "${package}" >&2
    exit 1
  fi
}

require_postgres_only() {
  local tree=$1
  local description=$2
  require_package_contains "${tree}" "sqlx-postgres" "${description}"
  require_package_absent "${tree}" "sqlx-sqlite" "${description}"
  require_package_absent "${tree}" "libsqlite3-sys" "${description}"
}

require_sqlite_only() {
  local tree=$1
  local description=$2
  require_package_contains "${tree}" "sqlx-sqlite" "${description}"
  require_package_contains "${tree}" "libsqlite3-sys" "${description}"
  require_package_absent "${tree}" "sqlx-postgres" "${description}"
}

require_no_sqlx() {
  local tree=$1
  local description=$2
  require_package_absent "${tree}" "sqlx" "${description}"
  require_package_absent "${tree}" "sqlx-postgres" "${description}"
  require_package_absent "${tree}" "sqlx-sqlite" "${description}"
  require_package_absent "${tree}" "libsqlite3-sys" "${description}"
}

require_package_absent \
  "minco-sqlx-postgres v0.3.0" \
  "sqlx-postgres" \
  "package matcher wrapper-name self-check"

feedback_postgres="$(dependency_tree \
  -p minco-plugin-feedback \
  --no-default-features \
  --features postgres)"
require_postgres_only "${feedback_postgres}" "Feedback PostgreSQL graph"

feedback_sqlite="$(dependency_tree \
  -p minco-plugin-feedback \
  --no-default-features \
  --features sqlite)"
require_sqlite_only "${feedback_sqlite}" "Feedback SQLite graph"

postgres_extension="$(dependency_tree -p minco-sqlx-postgres)"
require_postgres_only "${postgres_extension}" "PostgreSQL extension graph"

sqlite_extension="$(dependency_tree -p minco-sqlx-sqlite)"
require_sqlite_only "${sqlite_extension}" "SQLite extension graph"

orders_memory="$(dependency_tree \
  -p orders-adapters \
  --no-default-features \
  --features memory)"
require_no_sqlx "${orders_memory}" "Orders memory adapter graph"

orders_postgres="$(dependency_tree \
  -p orders-adapters \
  --no-default-features \
  --features postgres)"
require_postgres_only "${orders_postgres}" "Orders PostgreSQL adapter graph"

orders_sqlite="$(dependency_tree \
  -p orders-adapters \
  --no-default-features \
  --features sqlite)"
require_sqlite_only "${orders_sqlite}" "Orders SQLite adapter graph"

service_without_database="$(dependency_tree \
  -p orders-service \
  --no-default-features)"
require_no_sqlx "${service_without_database}" "Orders no-database service graph"

service_postgres="$(dependency_tree \
  -p orders-service \
  --no-default-features \
  --features postgres)"
require_postgres_only "${service_postgres}" "Orders PostgreSQL service graph"

service_sqlite="$(dependency_tree \
  -p orders-service \
  --no-default-features \
  --features sqlite)"
require_sqlite_only "${service_sqlite}" "Orders SQLite service graph"

minco_no_default="$(dependency_tree -p minco --no-default-features)"
require_no_sqlx "${minco_no_default}" "Minco no-default facade graph"

all_backends="$(dependency_tree --workspace --all-features)"
require_package_contains "${all_backends}" "sqlx-postgres" "all-feature workspace graph"
require_package_contains "${all_backends}" "sqlx-sqlite" "all-feature workspace graph"
require_package_contains "${all_backends}" "libsqlite3-sys" "all-feature workspace graph"

printf '%s\n' 'SQLx PostgreSQL and SQLite feature graphs are isolated.'
