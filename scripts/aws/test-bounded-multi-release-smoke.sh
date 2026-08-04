#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

canonical_path_fixture="$(mktemp /tmp/minco-canonical-path.XXXXXX)"
expected_canonical_path="$(
  cd "$(dirname "$canonical_path_fixture")"
  printf '%s/%s\n' "$(pwd -P)" "$(basename "$canonical_path_fixture")"
)"
actual_canonical_path="$(minco_canonical_existing_path "$canonical_path_fixture")"
[[ "$actual_canonical_path" == "$expected_canonical_path" ]] || {
  echo "existing temporary paths are not canonicalized physically" >&2
  exit 1
}
rm -- "$canonical_path_fixture"

fixture_dir="$(mktemp -d)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

fake_bin="$fixture_dir/bin"
mkdir -p "$fake_bin"
for command in aws cargo cargo-lambda curl psql sam uv; do
  # shellcheck disable=SC2016
  printf '#!/usr/bin/env bash\ntouch "$MINCO_FAKE_PROVIDER_CALLED"\nexit 99\n' \
    >"$fake_bin/$command"
  chmod 755 "$fake_bin/$command"
done

runner="scripts/aws/run-bounded-multi-release-smoke.sh"
[[ -x "$runner" ]] || {
  echo "bounded multi-release runner is missing" >&2
  exit 1
}

missing_authority_error="$fixture_dir/missing-authority-error.txt"
if PATH="$fake_bin:$PATH" \
  MINCO_FAKE_PROVIDER_CALLED="$fixture_dir/provider-called" \
  MINCO_MULTI_RELEASE_ACTION=plan \
  "$runner" 2>"$missing_authority_error"; then
  echo "bounded multi-release runner accepted missing authority" >&2
  exit 1
fi
grep -q 'MINCO_REHEARSAL_AUTHORITY_FILE' "$missing_authority_error" || {
  echo "bounded multi-release runner did not fail at the authority boundary" >&2
  exit 1
}
[[ ! -e "$fixture_dir/provider-called" ]] || {
  echo "bounded multi-release runner contacted a provider before authority validation" >&2
  exit 1
}

if ! rg -n 'MINCO_REHEARSAL_MODE.*multi-release' \
  scripts/aws/run-bounded-root-bootstrap.sh >/dev/null; then
  echo "root bootstrap does not expose the fixed multi-release mode" >&2
  exit 1
fi
if ! rg -n 'run-bounded-multi-release-smoke\.sh' \
  scripts/aws/run-bounded-root-bootstrap.sh >/dev/null; then
  echo "root bootstrap does not invoke the fixed multi-release runner" >&2
  exit 1
fi
if rg -n '^[[:space:]]*(scripts/aws/cleanup(-temp-rds)?\.sh|trap .*cleanup)' \
  "$runner" >/dev/null; then
  echo "bounded multi-release child claimed a resource cleanup trap" >&2
  exit 1
fi
if ! rg -n 'scripts/aws/cleanup\.sh' \
  scripts/aws/run-bounded-root-bootstrap.sh >/dev/null; then
  echo "root bootstrap does not retain application cleanup ownership" >&2
  exit 1
fi
if ! rg -F 'role-session-name "minco-cleanup-$run_suffix"' \
  scripts/aws/run-bounded-root-bootstrap.sh >/dev/null; then
  echo "multi-release cleanup does not refresh its exact bounded role session" >&2
  exit 1
fi
for temporary_path in \
  profile_config source_credentials role_credentials request_directory; do
  if ! rg -F "$temporary_path=\"\$(minco_canonical_existing_path \"\$$temporary_path\")\"" \
    scripts/aws/run-bounded-root-bootstrap.sh >/dev/null; then
    echo "root bootstrap does not canonicalize temporary $temporary_path" >&2
    exit 1
  fi
done
if ! rg -F '"$MINCO_PRIOR_ROOT" 01-prior-initial' \
  scripts/aws/create-temp-rds.sh >/dev/null ||
  ! rg -F '"$MINCO_CURRENT_ROOT" 02-current' \
    scripts/aws/create-temp-rds.sh >/dev/null; then
  echo "temporary RDS setup does not bind both exact-source migration receipts" >&2
  exit 1
fi
for approval in \
  MINCO_APPROVE_PRIOR_MIGRATION_PLAN_DIGEST \
  MINCO_APPROVE_CURRENT_MIGRATION_PLAN_DIGEST; do
  if ! rg -F ": \"\${$approval:?" \
    scripts/aws/run-bounded-root-bootstrap.sh >/dev/null ||
    ! rg -F "$approval" scripts/aws/create-temp-rds.sh >/dev/null; then
    echo "temporary RDS setup does not require $approval before creation" >&2
    exit 1
  fi
done
if ! rg -F -- '--allow-destructive' scripts/aws/create-temp-rds.sh >/dev/null; then
  echo "approved data-rewrite migration is not explicitly acknowledged" >&2
  exit 1
fi
if ! rg -n '01-prior-initial/database-migration-receipt\.json' \
  "$runner" >/dev/null; then
  echo "rollback phase does not reuse the exact prior migration receipt" >&2
  exit 1
fi
if [[ "$(rg -c -- '--id-token-validity 60' scripts/aws/create-smoke-identity.sh)" != "1" ]]; then
  echo "whole-run synthetic identity does not have the fixed 60-minute bound" >&2
  exit 1
fi
if ! rg -F ': "${MINCO_SMOKE_DATA_ID:=$MINCO_AWS_RUN_ID}"' \
  scripts/aws/smoke.sh >/dev/null ||
  ! rg -F 'MINCO_SMOKE_DATA_ID="$phase_id"' "$runner" >/dev/null; then
  echo "hosted phases do not bind distinct synthetic data identities" >&2
  exit 1
fi
for cleanup_identifier in \
  function-name.txt function-role-name.txt http-api-id.txt; do
  if ! rg -F "retain_cleanup_identifier $cleanup_identifier" "$runner" >/dev/null; then
    echo "phase execution does not retain $cleanup_identifier for root cleanup proof" >&2
    exit 1
  fi
done

printf 'Bounded multi-release runner checks passed.\n'
