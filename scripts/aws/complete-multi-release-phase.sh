#!/usr/bin/env bash
set -euo pipefail
umask 077

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in awk cp git jq mktemp mv shasum stat; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_EVIDENCE_ROOT:?set MINCO_MULTI_RELEASE_EVIDENCE_ROOT to the initialized whole-run evidence directory}"
: "${MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST to the initialized controller receipt digest}"
: "${MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST to the exact phase-start receipt digest}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_MULTI_RELEASE_PHASE_ID:?set MINCO_MULTI_RELEASE_PHASE_ID to the exact phase ID}"
: "${MINCO_MULTI_RELEASE_PHASE_RESULT_FILE:?set MINCO_MULTI_RELEASE_PHASE_RESULT_FILE to the exact provider result}"
: "${MINCO_APPROVE_MULTI_RELEASE_PHASE_RESULT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PHASE_RESULT_DIGEST to the exact provider-result SHA-256}"

case "$MINCO_MULTI_RELEASE_PHASE_ID" in
  01-prior-initial)
    previous_phase_id=
    next_phase_id=02-current
    ;;
  02-current)
    previous_phase_id=01-prior-initial
    next_phase_id=03-prior-rollback
    ;;
  03-prior-rollback)
    previous_phase_id=02-current
    next_phase_id=
    ;;
  *)
    echo "multi-release phase ID is outside the fixed sequence" >&2
    exit 1
    ;;
esac

require_exact_entries() {
  local directory="$1"
  shift
  local entry
  local expected_entry
  local found
  local -a actual=()
  local -a expected=("$@")

  shopt -s dotglob nullglob
  for entry in "$directory"/*; do
    actual+=("${entry##*/}")
  done
  shopt -u dotglob nullglob
  [[ "${#actual[@]}" -eq "${#expected[@]}" ]] || return 1
  for expected_entry in "${expected[@]}"; do
    found=false
    for entry in "${actual[@]}"; do
      if [[ "$entry" == "$expected_entry" ]]; then
        found=true
        break
      fi
    done
    [[ "$found" == true ]] || return 1
  done
}

require_exact_clean_checkout() {
  local label="$1"
  local root="$2"
  local expected_revision="$3"
  local canonical_root
  local status
  local revision

  [[ "$root" == /* && -d "$root" && ! -L "$root" &&
    -f "$root/minco.toml" && ! -L "$root/minco.toml" ]] || {
    printf '%s root must be an absolute existing checkout\n' "$label" >&2
    return 1
  }
  canonical_root="$(cd "$root" && pwd -P)"
  [[ "$canonical_root" == "$root" ]] || {
    printf '%s root must be canonical\n' "$label" >&2
    return 1
  }
  if [[ -d "$root/.jj" && ! -L "$root/.jj" ]] && command -v jj >/dev/null; then
    status="$(cd "$root" && jj diff --summary)"
  elif [[ (-d "$root/.git" || -f "$root/.git") && ! -L "$root/.git" ]] &&
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    status="$(git -C "$root" status --porcelain=v1 --untracked-files=normal)"
  else
    printf '%s root must own JJ or Git metadata\n' "$label" >&2
    return 1
  fi
  [[ -z "$status" ]] || {
    printf '%s root changed before phase completion\n' "$label" >&2
    return 1
  }
  revision="$(cd "$root" && current_source_revision)"
  [[ "$revision" == "$expected_revision" ]] || {
    printf '%s root revision changed before phase completion\n' "$label" >&2
    return 1
  }
}

[[ "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" == /* &&
  -d "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" &&
  ! -L "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" ]] || {
  echo "multi-release evidence root must be an absolute existing non-symlink directory" >&2
  exit 1
}
evidence_root="$(cd "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" && pwd -P)"
[[ "$evidence_root" == "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" &&
  "$(minco_file_mode "$evidence_root")" == "700" ]] || {
  echo "multi-release evidence root must remain canonical and private" >&2
  exit 1
}
for approval in \
  "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" \
  "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$MINCO_APPROVE_MULTI_RELEASE_PHASE_RESULT_DIGEST"; do
  [[ "$approval" =~ ^[0-9a-f]{64}$ ]] || {
    echo "multi-release phase completion approvals must be SHA-256 digests" >&2
    exit 1
  }
done
[[ "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE" == /* &&
  -f "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE" &&
  ! -L "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE" ]] || {
  echo "multi-release phase result must be an absolute regular non-symlink file" >&2
  exit 1
}
result_root="$(cd "$(dirname "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE")" && pwd -P)"
result_file="$result_root/$(basename "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE")"
[[ "$result_file" == "$MINCO_MULTI_RELEASE_PHASE_RESULT_FILE" &&
  "$(minco_file_mode "$result_file")" == "600" ]] || {
  echo "multi-release phase result must be canonical and private" >&2
  exit 1
}
validation_dir="$(mktemp -d)"
sealed_result="$validation_dir/phase-result.json"
completion_payload=
completion_tmp=
cleanup_local_state() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$completion_payload" ]]; then
    rm -f -- "$completion_payload"
  fi
  if [[ -n "$completion_tmp" ]]; then
    rm -f -- "$completion_tmp"
  fi
  if [[ -d "$validation_dir" && ! -L "$validation_dir" ]]; then
    rm -r -- "$validation_dir"
  fi
  exit "$status"
}
trap cleanup_local_state EXIT INT TERM
cp "$result_file" "$sealed_result"
chmod 600 "$sealed_result"
actual_result_digest="$(shasum -a 256 "$sealed_result" | awk '{print $1}')"
[[ "$actual_result_digest" == "$MINCO_APPROVE_MULTI_RELEASE_PHASE_RESULT_DIGEST" ]] || {
  echo "phase-result approval does not match the exact provider result" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-phase-result.jq \
  "$sealed_result" >/dev/null || {
  echo "multi-release phase result is outside the fixed policy" >&2
  exit 1
}

control_root="$evidence_root/control"
phases_root="$evidence_root/phases"
controller_receipt="$control_root/controller-receipt.json"
sealed_plan="$control_root/multi-release-plan.json"
phase_path="$phases_root/$MINCO_MULTI_RELEASE_PHASE_ID"
phase_projection="$phase_path/phase-projection.json"
phase_start_receipt="$phase_path/phase-start-receipt.json"
phase_completion_receipt="$phase_path/phase-completion-receipt.json"
[[ -f "$controller_receipt" && ! -L "$controller_receipt" &&
  -f "$sealed_plan" && ! -L "$sealed_plan" &&
  -d "$phase_path" && ! -L "$phase_path" &&
  "$(minco_file_mode "$phase_path")" == "700" ]] || {
  echo "multi-release phase completion boundary is missing or unsafe" >&2
  exit 1
}
if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "01-prior-initial" ]]; then
  require_exact_entries "$phase_path" \
    parent-session-completion-receipt.json \
    parent-session-start-receipt.json \
    phase-projection.json phase-start-receipt.json || {
    echo "initial phase provider preflight evidence is incomplete or unsealed" >&2
    exit 1
  }
else
  require_exact_entries "$phase_path" phase-projection.json phase-start-receipt.json || {
    echo "multi-release phase start evidence is incomplete or unsealed" >&2
    exit 1
  }
fi
[[ ! -e "$phase_completion_receipt" && ! -L "$phase_completion_receipt" ]] || {
  echo "multi-release phase completion is create-only" >&2
  exit 1
}

jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null || exit 1
controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
[[ "$controller_receipt_digest" == \
    "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" &&
  "$(
    jq -cS 'del(.receipt_digest)' "$controller_receipt" |
      shasum -a 256 | awk '{print $1}'
  )" == "$controller_receipt_digest" ]] || {
  echo "controller receipt approval is invalid during phase completion" >&2
  exit 1
}
plan_digest="$(shasum -a 256 "$sealed_plan" | awk '{print $1}')"
[[ "$plan_digest" == "$(jq -er '.plan_digest' "$controller_receipt")" ]] || {
  echo "sealed plan changed before phase completion" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq "$sealed_plan" >/dev/null || {
  echo "sealed plan is outside the fixed policy during phase completion" >&2
  exit 1
}
[[ -f "$MINCO_REHEARSAL_AUTHORITY_FILE" &&
  ! -L "$MINCO_REHEARSAL_AUTHORITY_FILE" &&
  "$(shasum -a 256 "$MINCO_REHEARSAL_AUTHORITY_FILE" | awk '{print $1}')" == \
    "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" &&
  "$(jq -er '.authority.approval_digest' "$controller_receipt")" == \
    "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" ]] || {
  echo "authority changed before phase completion" >&2
  exit 1
}
prior_root="$(jq -er '.phases[0].source.root' "$sealed_plan")"
current_root="$(jq -er '.phases[1].source.root' "$sealed_plan")"
prior_revision="$(jq -er '.source_revisions.prior' "$controller_receipt")"
current_revision="$(jq -er '.source_revisions.current' "$controller_receipt")"
require_exact_clean_checkout prior "$prior_root" "$prior_revision"
require_exact_clean_checkout current "$current_root" "$current_revision"
jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_start_receipt" >/dev/null || exit 1
phase_start_digest="$(jq -er '.receipt_digest' "$phase_start_receipt")"
projection_digest="$(shasum -a 256 "$phase_projection" | awk '{print $1}')"
[[ "$phase_start_digest" == \
    "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" &&
  "$(
    jq -cS 'del(.receipt_digest)' "$phase_start_receipt" |
      shasum -a 256 | awk '{print $1}'
  )" == "$phase_start_digest" &&
  "$projection_digest" == "$(jq -er '.phase.projection_digest' "$phase_start_receipt")" ]] || {
  echo "phase-start approval is invalid during completion" >&2
  exit 1
}
jq -e \
  --arg phase_id "$MINCO_MULTI_RELEASE_PHASE_ID" \
  --arg source_revision "$(jq -er '.phase.source_revision' "$phase_start_receipt")" \
  '.phase == {
    evidence_id: $phase_id,
    id: $phase_id,
    source_revision: $source_revision
  }' "$sealed_result" >/dev/null || {
  echo "provider result does not bind the exact started phase" >&2
  exit 1
}

previous_completion_digest=
if [[ -n "$previous_phase_id" ]]; then
  previous_completion="$phases_root/$previous_phase_id/phase-completion-receipt.json"
  jq -e -f scripts/aws/lib/validate-multi-release-phase-completion-receipt.jq \
    "$previous_completion" >/dev/null || exit 1
  previous_completion_digest="$(jq -er '.receipt_digest' "$previous_completion")"
  [[ "$(
    jq -cS 'del(.receipt_digest)' "$previous_completion" |
      shasum -a 256 | awk '{print $1}'
  )" == "$previous_completion_digest" &&
    "$(jq -er '.transition.next_phase' "$previous_completion")" == \
      "$MINCO_MULTI_RELEASE_PHASE_ID" ]] || {
    echo "previous phase completion no longer authorizes this phase" >&2
    exit 1
  }
fi
if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "02-current" ]]; then
  phase_one_release_digest="$(jq -er \
    '.result.artifacts.release_manifest_digest' \
    "$phases_root/01-prior-initial/phase-completion-receipt.json")"
  [[ "$(jq -er '.artifacts.release_manifest_digest' "$sealed_result")" != \
    "$phase_one_release_digest" ]] || {
    echo "current phase did not produce a distinct exact release" >&2
    exit 1
  }
elif [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "03-prior-rollback" ]]; then
  phase_one_completion="$phases_root/01-prior-initial/phase-completion-receipt.json"
  jq -e -f scripts/aws/lib/validate-multi-release-phase-completion-receipt.jq \
    "$phase_one_completion" >/dev/null || {
    echo "initial phase completion is outside the fixed policy during rollback" >&2
    exit 1
  }
  phase_one_completion_digest="$(jq -er '.receipt_digest' "$phase_one_completion")"
  [[ "$(
    jq -cS 'del(.receipt_digest)' "$phase_one_completion" |
      shasum -a 256 | awk '{print $1}'
  )" == "$phase_one_completion_digest" &&
    "$(jq -er '.transition.previous_phase_completion_digest' "$previous_completion")" == \
      "$phase_one_completion_digest" &&
    "$(jq -er '.result.artifacts.release_manifest_digest' "$previous_completion")" != \
      "$(jq -er '.result.artifacts.release_manifest_digest' "$phase_one_completion")" ]] || {
    echo "rollback predecessor chain no longer binds the initial phase" >&2
    exit 1
  }
  phase_one_release_digest="$(jq -er \
    '.result.artifacts.release_manifest_digest' \
    "$phase_one_completion")"
  jq -e --arg release_digest "$phase_one_release_digest" '
    .artifacts.release_manifest_digest == $release_digest
    and .rollback.reused_release_manifest_digest == $release_digest
    and .rollback.exact_initial_release_reused == true
    and (.rollback.assessment_digest | type == "string")
  ' "$sealed_result" >/dev/null || {
    echo "rollback phase did not reuse the exact initial release" >&2
    exit 1
  }
fi

authority_json="$(jq -c '.authority' "$controller_receipt")"
result_json="$(jq -c . "$sealed_result")"
completion_payload="$(mktemp "$phase_path/.phase-completion-payload.XXXXXX")"
completion_tmp="$(mktemp "$phase_path/.phase-completion-receipt.XXXXXX")"
jq -n \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg next_phase "$next_phase_id" \
  --arg plan_digest "$plan_digest" \
  --arg previous_completion_digest "$previous_completion_digest" \
  --arg projection_digest "$projection_digest" \
  --arg result_digest "$actual_result_digest" \
  --arg start_receipt_digest "$phase_start_digest" \
  --argjson authority "$authority_json" \
  --argjson result "$result_json" \
  '{
    schema_version: 1,
    operation: "multi_release_phase_completion",
    state: "succeeded",
    external_aws_contact: true,
    controller: {
      plan_digest: $plan_digest,
      receipt_digest: $controller_receipt_digest
    },
    authority: $authority,
    phase: {
      id: $result.phase.id,
      source_revision: $result.phase.source_revision,
      evidence_id: $result.phase.evidence_id,
      projection_digest: $projection_digest,
      start_receipt_digest: $start_receipt_digest
    },
    result: ($result + {receipt_digest: $result_digest}),
    transition: {
      previous_phase_completion_digest: (
        if $previous_completion_digest == "" then null
        else $previous_completion_digest end
      ),
      next_phase: (if $next_phase == "" then null else $next_phase end)
    },
    cleanup: {
      owner: "parent_controller",
      deferred: true
    }
  }' >"$completion_payload"
receipt_digest="$(jq -cS . "$completion_payload" | shasum -a 256 | awk '{print $1}')"
jq --arg receipt_digest "$receipt_digest" \
  '. + {receipt_digest: $receipt_digest}' \
  "$completion_payload" >"$completion_tmp"
chmod 600 "$completion_tmp"
jq -e -f scripts/aws/lib/validate-multi-release-phase-completion-receipt.jq \
  "$completion_tmp" >/dev/null || {
  echo "multi-release phase completion receipt is outside the fixed policy" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$completion_tmp" |
    shasum -a 256 | awk '{print $1}'
)" == "$receipt_digest" ]] || exit 1
if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "01-prior-initial" ]]; then
  require_exact_entries "$phase_path" \
    "${completion_payload##*/}" "${completion_tmp##*/}" \
    parent-session-completion-receipt.json \
    parent-session-start-receipt.json \
    phase-projection.json phase-start-receipt.json || exit 1
else
  require_exact_entries "$phase_path" \
    "${completion_payload##*/}" "${completion_tmp##*/}" \
    phase-projection.json phase-start-receipt.json || exit 1
fi
rm -f -- "$completion_payload"
completion_payload=
mv -n "$completion_tmp" "$phase_completion_receipt"
[[ ! -e "$completion_tmp" &&
  -f "$phase_completion_receipt" && ! -L "$phase_completion_receipt" &&
  "$(minco_file_mode "$phase_completion_receipt")" == "600" ]] || {
  echo "could not atomically complete the multi-release phase" >&2
  exit 1
}
completion_tmp=
rm -r -- "$validation_dir"
validation_dir=
trap - EXIT INT TERM
cat "$phase_completion_receipt"
