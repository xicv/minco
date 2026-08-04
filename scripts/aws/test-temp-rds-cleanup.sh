#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fixture_dir="$(mktemp -d)"
run_id="cleanup-secret-test-$$"
evidence_dir="$repo_root/target/minco/aws/$run_id"
cleanup_fixture() {
  rm -r -- "$fixture_dir" "$evidence_dir"
}
trap cleanup_fixture EXIT

mkdir -p "$fixture_dir/bin" "$evidence_dir"
printf '%s\n' \
  'arn:aws:secretsmanager:ap-southeast-2:123456789012:secret:run-owned-test' \
  >"$evidence_dir/rds-master-secret-arn.txt"

cat >"$fixture_dir/bin/aws" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

while (($# > 0)); do
  case "$1" in
    --no-cli-pager)
      shift
      ;;
    --cli-error-format | --region)
      shift 2
      ;;
    *)
      break
      ;;
  esac
done

service="${1:-}"
operation="${2:-}"
case "$service:$operation" in
  cloudformation:describe-stacks)
    printf '{"Code":"ValidationError","Message":"stack is absent"}\n' >&2
    exit 254
    ;;
  rds:describe-db-instances)
    printf '{"Code":"DBInstanceNotFound","Message":"database is absent"}\n' >&2
    exit 254
    ;;
  secretsmanager:delete-secret)
    printf '{}\n'
    ;;
  secretsmanager:describe-secret)
    count=0
    if [[ -f "$MINCO_TEST_SECRET_DESCRIBE_COUNT" ]]; then
      count="$(<"$MINCO_TEST_SECRET_DESCRIBE_COUNT")"
    fi
    count=$((count + 1))
    printf '%s\n' "$count" >"$MINCO_TEST_SECRET_DESCRIBE_COUNT"
    if ((count >= 17)); then
      printf '{"Code":"ResourceNotFoundException","Message":"secret is absent"}\n' >&2
      exit 254
    fi
    printf '{"ARN":"run-owned-test"}\n'
    ;;
  *)
    printf 'unexpected fake AWS command: %s %s\n' "$service" "$operation" >&2
    exit 2
    ;;
esac
SH
chmod +x "$fixture_dir/bin/aws"

cat >"$fixture_dir/bin/sleep" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$fixture_dir/bin/sleep"

export MINCO_TEST_SECRET_DESCRIBE_COUNT="$fixture_dir/secret-describe-count.txt"
PATH="$fixture_dir/bin:$PATH" \
  MINCO_AWS_RUN_ID="$run_id" \
  MINCO_RDS_STACK_NAME="minco-test-stack" \
  MINCO_RDS_INSTANCE_ID="minco-test-instance" \
  scripts/aws/cleanup-temp-rds.sh

jq -e '
  .temporary_database_master_secret_absent == true
  and ([.[]] | all)
' "$evidence_dir/rds-cleanup.json" >/dev/null
[[ "$(<"$MINCO_TEST_SECRET_DESCRIBE_COUNT")" == 17 ]]

printf 'Temporary RDS cleanup regression checks passed.\n'
