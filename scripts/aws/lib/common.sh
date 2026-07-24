#!/usr/bin/env bash
set -euo pipefail

minco_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 || return
  pwd
}

require_command() {
  command -v "$1" >/dev/null || {
    printf '%s is required\n' "$1" >&2
    return 1
  }
}

require_safe_name() {
  local label="$1"
  local value="$2"
  [[ ${#value} -le 48 && "$value" =~ ^[a-zA-Z0-9][a-zA-Z0-9._-]*$ ]] || {
    printf '%s contains unsupported characters: %s\n' "$label" "$value" >&2
    return 1
  }
}

s3_tagged_create_configuration() {
  local region="$1"
  local run_id="$2"
  require_command jq
  jq -cn \
    --arg region "$region" \
    --arg run_id "$run_id" \
    '{
      Tags: [
        {Key: "minco:managed", Value: "true"},
        {Key: "minco:purpose", Value: "bounded-smoke"},
        {Key: "minco:run-id", Value: $run_id}
      ]
    }
    | if $region == "us-east-1"
      then .
      else . + {LocationConstraint: $region}
      end'
}

postgres_url_to_conninfo() {
  local database_url="$1"
  require_command python3
  printf '%s' "$database_url" | python3 -c '
import re
import sys
from urllib.parse import parse_qsl, unquote, urlsplit

url = sys.stdin.read()
parsed = urlsplit(url)
if parsed.scheme not in {"postgres", "postgresql"}:
    raise SystemExit("PostgreSQL URL must use postgres or postgresql")
if parsed.fragment:
    raise SystemExit("PostgreSQL URL fragments are unsupported")
if not parsed.hostname:
    raise SystemExit("PostgreSQL URL must include a host")

def quote(value: str) -> str:
    single_quote = chr(39)
    return (
        single_quote
        + value.replace("\\", "\\\\").replace(single_quote, "\\" + single_quote)
        + single_quote
    )

values = [
    ("host", parsed.hostname),
]
if parsed.port is not None:
    values.append(("port", str(parsed.port)))
if parsed.username is not None:
    values.append(("user", unquote(parsed.username)))
if parsed.password is not None:
    values.append(("password", unquote(parsed.password)))
if parsed.path and parsed.path != "/":
    values.append(("dbname", unquote(parsed.path.removeprefix("/"))))

structural = {"host", "hostaddr", "port", "user", "password", "dbname"}
seen = set()
for key, value in parse_qsl(parsed.query, keep_blank_values=True):
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
        raise SystemExit("PostgreSQL URL contains an invalid query key")
    if key in structural or key in seen:
        raise SystemExit("PostgreSQL URL contains a duplicate connection key")
    seen.add(key)
    values.append((key, value))

sys.stdout.write(" ".join(f"{key}={quote(value)}" for key, value in values))
'
}

psql_with_url() {
  local database_url="$1"
  local conninfo
  shift
  conninfo="$(postgres_url_to_conninfo "$database_url")"
  PGCONNECT_TIMEOUT="${PGCONNECT_TIMEOUT:-10}" \
    PGDATABASE="$conninfo" \
    command psql "$@"
}

initialize_cloud_journal() {
  : "${MINCO_AWS_RUN_ID:?set MINCO_AWS_RUN_ID}"
  require_safe_name "MINCO_AWS_RUN_ID" "$MINCO_AWS_RUN_ID"
  MINCO_AWS_EVIDENCE_DIR="$(minco_repo_root)/target/minco/aws/$MINCO_AWS_RUN_ID"
  mkdir -p "$MINCO_AWS_EVIDENCE_DIR"
  chmod 700 "$MINCO_AWS_EVIDENCE_DIR"
  MINCO_AWS_TOUCH_LOG="$MINCO_AWS_EVIDENCE_DIR/cloud-touches.jsonl"
  touch "$MINCO_AWS_TOUCH_LOG"
  chmod 600 "$MINCO_AWS_TOUCH_LOG"
  export MINCO_AWS_EVIDENCE_DIR MINCO_AWS_TOUCH_LOG
}

record_cloud_touch() {
  local service="$1"
  local action="$2"
  local detail="$3"
  jq -cn \
    --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    --arg service "$service" \
    --arg action "$action" \
    --arg detail "$detail" \
    '{at: $at, run_id: $run_id, service: $service, action: $action, detail: $detail}' \
    >>"$MINCO_AWS_TOUCH_LOG"
}

aws_logged() {
  local service="$1"
  local operation="$2"
  local detail="$3"
  shift 3
  record_cloud_touch "aws:$service" "$operation" "$detail" || return
  AWS_PAGER="" command aws \
    --no-cli-pager \
    --region "$AWS_REGION" \
    "$service" "$operation" "$@"
}

sam_logged() {
  local action="$1"
  local detail="$2"
  shift 2
  record_cloud_touch "aws:sam" "$action" "$detail" || return
  AWS_PAGER="" SAM_CLI_TELEMETRY=0 command sam "$@"
}

record_external_database_touch() {
  record_cloud_touch "external:postgresql" "$1" "$2"
}

write_evidence_value() {
  local path="$1"
  local value="$2"
  printf '%s\n' "$value" >"$path"
  chmod 600 "$path"
}
