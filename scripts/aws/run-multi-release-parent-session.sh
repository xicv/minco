#!/usr/bin/env bash
set -euo pipefail
umask 077

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in awk cat cmp git jq mktemp mv rm shasum stat; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_EVIDENCE_ROOT:?set MINCO_MULTI_RELEASE_EVIDENCE_ROOT to the initialized whole-run evidence directory}"
: "${MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST to the initialized controller receipt digest}"
: "${MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST to the exact phase-start receipt digest}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_MULTI_RELEASE_PHASE_ID:?set MINCO_MULTI_RELEASE_PHASE_ID to the exact started phase ID}"

[[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "01-prior-initial" ]] || {
  echo "only the exact started first phase may enter a parent session" >&2
  exit 1
}

file_mode() {
  if stat -f '%Lp' "$1" 2>/dev/null; then
    return
  fi
  stat -c '%a' "$1"
}

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

require_private_directory() {
  [[ -d "$1" && ! -L "$1" && "$(file_mode "$1")" == "700" ]]
}

require_private_file() {
  [[ -f "$1" && ! -L "$1" && "$(file_mode "$1")" == "600" ]]
}

canonical_checkout_root() {
  local label="$1"
  local root="$2"

  [[ "$root" == /* && -d "$root" && ! -L "$root" ]] || {
    printf '%s root must be an absolute existing non-symlink directory\n' "$label" >&2
    return 1
  }
  [[ -f "$root/minco.toml" && ! -L "$root/minco.toml" ]] || {
    printf '%s root must contain a regular non-symlink minco.toml\n' "$label" >&2
    return 1
  }
  (
    cd "$root"
    pwd -P
  )
}

require_exact_clean_checkout() {
  local label="$1"
  local root="$2"
  local expected_revision="$3"
  local status
  local revision

  if [[ -d "$root/.jj" && ! -L "$root/.jj" ]] && command -v jj >/dev/null; then
    status="$(cd "$root" && jj diff --summary)"
  elif [[ (-d "$root/.git" || -f "$root/.git") && ! -L "$root/.git" ]] &&
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    status="$(git -C "$root" status --porcelain=v1 --untracked-files=normal)"
  else
    printf '%s root must own JJ or Git checkout metadata\n' "$label" >&2
    return 1
  fi
  [[ -z "$status" ]] || {
    printf '%s root must be clean at parent-session handoff\n' "$label" >&2
    return 1
  }
  revision="$(cd "$root" && current_source_revision)"
  [[ "$revision" == "$expected_revision" ]] || {
    printf '%s root revision does not match the initialized controller\n' "$label" >&2
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
[[ "$evidence_root" == "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT" ]] || {
  echo "multi-release evidence root must be canonical" >&2
  exit 1
}
require_private_directory "$evidence_root" || {
  echo "multi-release evidence root must remain mode 0700" >&2
  exit 1
}
[[ "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" =~ ^[0-9a-f]{64}$ &&
  "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" =~ ^[0-9a-f]{64}$ &&
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
  echo "multi-release parent-session approvals must be SHA-256 digests" >&2
  exit 1
}
[[ "$MINCO_REHEARSAL_AUTHORITY_FILE" == /* &&
  -f "$MINCO_REHEARSAL_AUTHORITY_FILE" &&
  ! -L "$MINCO_REHEARSAL_AUTHORITY_FILE" ]] || {
  echo "multi-release authority must be an absolute regular non-symlink file" >&2
  exit 1
}
authority_root="$(cd "$(dirname "$MINCO_REHEARSAL_AUTHORITY_FILE")" && pwd -P)"
authority_file="$authority_root/$(basename "$MINCO_REHEARSAL_AUTHORITY_FILE")"
[[ "$authority_file" == "$MINCO_REHEARSAL_AUTHORITY_FILE" ]] || {
  echo "multi-release authority path must be canonical" >&2
  exit 1
}
actual_authority_digest="$(
  shasum -a 256 "$authority_file" | awk '{print $1}'
)"
[[ "$actual_authority_digest" == "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" ]] || {
  echo "multi-release authority approval does not match the exact document digest" >&2
  exit 1
}

control_root="$evidence_root/control"
control_phases="$control_root/phases"
phase_path="$evidence_root/phases/$MINCO_MULTI_RELEASE_PHASE_ID"
controller_receipt="$control_root/controller-receipt.json"
sealed_plan="$control_root/multi-release-plan.json"
authority_receipt="$control_root/authority-receipt.json"
sealed_projection="$control_phases/$MINCO_MULTI_RELEASE_PHASE_ID.json"
phase_projection="$phase_path/phase-projection.json"
phase_start_receipt="$phase_path/phase-start-receipt.json"
parent_session_start="$phase_path/parent-session-start-receipt.json"
parent_session_completion="$phase_path/parent-session-completion-receipt.json"

require_exact_entries "$evidence_root" control phases || {
  echo "multi-release evidence root contains unsealed state" >&2
  exit 1
}
require_exact_entries "$control_root" \
  authority-receipt.json controller-receipt.json multi-release-plan.json phases || {
  echo "multi-release control directory contains unsealed state" >&2
  exit 1
}
require_exact_entries "$control_phases" \
  01-prior-initial.json 02-current.json 03-prior-rollback.json || {
  echo "multi-release phase projections contain unsealed state" >&2
  exit 1
}
require_exact_entries "$evidence_root/phases" 01-prior-initial || {
  echo "multi-release phase boundary contains an unexpected phase" >&2
  exit 1
}
require_exact_entries "$phase_path" phase-projection.json phase-start-receipt.json || {
  echo "started phase contains unsealed state" >&2
  exit 1
}
for directory in "$control_root" "$control_phases" "$evidence_root/phases" "$phase_path"; do
  require_private_directory "$directory" || {
    echo "multi-release parent-session directories must remain private" >&2
    exit 1
  }
done
for control_file in \
  "$authority_receipt" \
  "$controller_receipt" \
  "$sealed_plan" \
  "$control_phases/01-prior-initial.json" \
  "$control_phases/02-current.json" \
  "$control_phases/03-prior-rollback.json" \
  "$phase_projection" \
  "$phase_start_receipt"; do
  require_private_file "$control_file" || {
    echo "multi-release parent-session evidence must be regular mode-0600 files" >&2
    exit 1
  }
done
[[ ! -e "$parent_session_start" && ! -L "$parent_session_start" &&
  ! -e "$parent_session_completion" && ! -L "$parent_session_completion" ]] || {
  echo "multi-release parent session is create-only" >&2
  exit 1
}

validation_dir="$(mktemp -d)"
start_tmp=
completion_tmp=
session_started=false
session_start_digest=

build_parent_session_payload() {
  local state="$1"
  local output_path="$2"
  local cleanup_action
  local cleanup_state
  local authority_json
  local controller_json
  local phase_json

  if [[ "$state" == "started" ]]; then
    cleanup_action="none_before_provider_boundary"
    cleanup_state="installed"
  else
    cleanup_action="none_provider_boundary_not_entered"
    cleanup_state="disarmed"
  fi
  authority_json="$(jq -c '.authority' "$phase_start_receipt")"
  controller_json="$(jq -c '.controller' "$phase_start_receipt")"
  phase_json="$(jq -c \
    --arg start_receipt_digest "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
    '.phase + {start_receipt_digest: $start_receipt_digest}' \
    "$phase_start_receipt")"
  jq -n \
    --arg cleanup_action "$cleanup_action" \
    --arg cleanup_state "$cleanup_state" \
    --arg session_start_receipt_digest "$session_start_digest" \
    --arg state "$state" \
    --argjson authority "$authority_json" \
    --argjson controller "$controller_json" \
    --argjson phase "$phase_json" \
    '{
      schema_version: 1,
      operation: "multi_release_parent_session",
      state: $state,
      external_aws_contact: false,
      controller: $controller,
      authority: $authority,
      phase: $phase,
      execution: {
        mode: "validation_only",
        provider_state: "not_entered"
      },
      session: {
        start_receipt_digest: (
          if $state == "started"
          then null
          else $session_start_receipt_digest
          end
        )
      },
      cleanup: {
        owner: "parent_controller",
        required: true,
        trap_count: 1,
        state: $cleanup_state,
        action: $cleanup_action
      }
    }' >"$output_path"
}

seal_parent_session_receipt() {
  local payload_path="$1"
  local output_path="$2"
  local receipt_digest

  receipt_digest="$(
    jq -cS . "$payload_path" | shasum -a 256 | awk '{print $1}'
  )"
  jq --arg receipt_digest "$receipt_digest" \
    '. + {receipt_digest: $receipt_digest}' \
    "$payload_path" >"$output_path"
  chmod 600 "$output_path"
  jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
    "$output_path" >/dev/null
  [[ "$(
    jq -cS 'del(.receipt_digest)' "$output_path" |
      shasum -a 256 | awk '{print $1}'
  )" == "$receipt_digest" ]]
}

# Invoked by the EXIT/interrupt trap below.
# shellcheck disable=SC2329
finalize_parent_session() {
  local status=$?
  local completion_payload

  trap - EXIT INT TERM
  if [[ "$session_started" == true && "$status" -eq 0 ]]; then
    if ! require_exact_entries "$phase_path" \
      parent-session-start-receipt.json \
      phase-projection.json phase-start-receipt.json; then
      echo "started parent session contains unsealed state" >&2
      status=1
    elif ! require_private_file "$parent_session_start" ||
      [[ "$(jq -er '.receipt_digest' "$parent_session_start")" != \
        "$session_start_digest" ]] ||
      [[ "$(
        jq -cS 'del(.receipt_digest)' "$parent_session_start" |
          shasum -a 256 | awk '{print $1}'
      )" != "$session_start_digest" ]]; then
      echo "parent-session start receipt changed before completion" >&2
      status=1
    else
      completion_payload="$validation_dir/parent-session-completion-payload.json"
      build_parent_session_payload validated "$completion_payload"
      completion_tmp="$(mktemp "$phase_path/.parent-session-completion.XXXXXX")"
      if ! seal_parent_session_receipt "$completion_payload" "$completion_tmp" ||
        ! mv -n "$completion_tmp" "$parent_session_completion" ||
        [[ -e "$completion_tmp" ]] ||
        ! require_private_file "$parent_session_completion"; then
        echo "could not atomically complete the parent validation session" >&2
        status=1
      else
        completion_tmp=
      fi
    fi
  fi
  if [[ -n "$start_tmp" && -f "$start_tmp" && ! -L "$start_tmp" ]]; then
    rm -f -- "$start_tmp"
  fi
  if [[ -n "$completion_tmp" && -f "$completion_tmp" && ! -L "$completion_tmp" ]]; then
    rm -f -- "$completion_tmp"
  fi
  if [[ -d "$validation_dir" && ! -L "$validation_dir" ]]; then
    rm -r -- "$validation_dir"
  fi
  if [[ "$status" -eq 0 && -f "$parent_session_completion" ]]; then
    if ! cat "$parent_session_completion"; then
      status=1
    fi
  fi
  exit "$status"
}
trap finalize_parent_session EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null || {
  echo "initialized controller receipt is outside the fixed policy" >&2
  exit 1
}
controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
[[ "$controller_receipt_digest" == \
  "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" ]] || {
  echo "controller approval does not match the initialized receipt" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$controller_receipt" |
    shasum -a 256 | awk '{print $1}'
)" == "$controller_receipt_digest" ]] || {
  echo "initialized controller receipt digest is invalid" >&2
  exit 1
}

plan_digest="$(shasum -a 256 "$sealed_plan" | awk '{print $1}')"
[[ "$plan_digest" == "$(jq -er '.plan_digest' "$controller_receipt")" ]] || {
  echo "sealed plan does not match the initialized controller" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq "$sealed_plan" >/dev/null || {
  echo "sealed plan is outside the fixed controller policy" >&2
  exit 1
}
[[ "$(jq -er '.authority.approval_digest' "$controller_receipt")" == \
  "$actual_authority_digest" ]] || {
  echo "authority does not match the initialized controller" >&2
  exit 1
}
for sealed_phase_id in 01-prior-initial 02-current 03-prior-rollback; do
  sealed_phase_projection="$control_phases/$sealed_phase_id.json"
  expected_projection_digest="$(
    jq -er \
      --arg phase_id "$sealed_phase_id" \
      '.execution.phases[] | select(.id == $phase_id) | .projection_digest' \
      "$controller_receipt"
  )"
  [[ "$(shasum -a 256 "$sealed_phase_projection" | awk '{print $1}')" == \
    "$expected_projection_digest" ]] || {
    echo "sealed phase projection does not match the initialized controller" >&2
    exit 1
  }
done
cmp -s "$sealed_projection" "$phase_projection" || {
  echo "started phase projection differs from sealed control evidence" >&2
  exit 1
}

jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_start_receipt" >/dev/null || {
  echo "phase-start receipt is outside the fixed policy" >&2
  exit 1
}
phase_start_receipt_digest="$(jq -er '.receipt_digest' "$phase_start_receipt")"
[[ "$phase_start_receipt_digest" == \
  "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" ]] || {
  echo "phase-start approval does not match the exact receipt" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$phase_start_receipt" |
    shasum -a 256 | awk '{print $1}'
)" == "$phase_start_receipt_digest" ]] || {
  echo "phase-start receipt digest is invalid" >&2
  exit 1
}
projection_digest="$(shasum -a 256 "$phase_projection" | awk '{print $1}')"
jq -e \
  --arg authority_digest "$actual_authority_digest" \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg plan_digest "$plan_digest" \
  --arg projection_digest "$projection_digest" \
  --slurpfile projection "$phase_projection" \
  '
    .controller == {
      plan_digest: $plan_digest,
      receipt_digest: $controller_receipt_digest
    }
    and .authority.approval_digest == $authority_digest
    and .phase.id == $projection[0].phase.id
    and .phase.release == $projection[0].phase.release
    and .phase.source_revision == $projection[0].phase.source.revision
    and .phase.evidence_namespace == $projection[0].phase.evidence_namespace
    and .phase.projection_digest == $projection_digest
    and .phase.stack_action == $projection[0].phase.stack_action
    and .phase.change_set_review_policy
      == $projection[0].phase.change_set_review_policy
  ' "$phase_start_receipt" >/dev/null || {
  echo "phase-start receipt does not bind the exact sealed phase" >&2
  exit 1
}

prior_revision="$(jq -er '.source_revisions.prior' "$controller_receipt")"
current_revision="$(jq -er '.source_revisions.current' "$controller_receipt")"
authority_region="$(jq -er '.expected_region' "$authority_file")"
authority_profile="$(jq -er '.aws_profile' "$authority_file")"
authority_environment="$(jq -er '.environment' "$authority_file")"
authority_database_boundary="$(jq -cer '.database_boundary' "$authority_file")"
authority_resource_allowlist="$(jq -er '.resource_allowlist' "$authority_file")"
authority_cleanup_blast_radius="$(jq -er '.cleanup_blast_radius' "$authority_file")"
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$authority_file" "$actual_authority_digest" \
  "$(jq -er '.authority.run_id' "$controller_receipt")" \
  "$prior_revision" "$current_revision" \
  "$authority_region" "$authority_profile" "$authority_environment" \
  "$authority_database_boundary" "$authority_resource_allowlist" \
  "$authority_cleanup_blast_radius"

regenerated_authority_receipt="$validation_dir/authority-receipt.json"
write_multi_release_rehearsal_authority_receipt \
  "$authority_file" "$actual_authority_digest" \
  "$regenerated_authority_receipt"
cmp -s "$authority_receipt" "$regenerated_authority_receipt" || {
  echo "sealed authority receipt does not match the approved authority" >&2
  exit 1
}

prior_root="$(canonical_checkout_root prior "$(jq -er '.phases[0].source.root' "$sealed_plan")")"
current_root="$(canonical_checkout_root current "$(jq -er '.phases[1].source.root' "$sealed_plan")")"
[[ "$prior_root" != "$current_root" && "$current_root" == "$repo_root" ]] || {
  echo "parent session must run from the exact distinct current controller checkout" >&2
  exit 1
}
require_exact_clean_checkout prior "$prior_root" "$prior_revision"
require_exact_clean_checkout current "$current_root" "$current_revision"

start_payload="$validation_dir/parent-session-start-payload.json"
build_parent_session_payload started "$start_payload"
start_tmp="$(mktemp "$phase_path/.parent-session-start.XXXXXX")"
seal_parent_session_receipt "$start_payload" "$start_tmp" || {
  echo "parent-session start receipt is outside the fixed policy" >&2
  exit 1
}
require_exact_entries "$phase_path" \
  "${start_tmp##*/}" phase-projection.json phase-start-receipt.json || {
  echo "started phase changed before parent-session claim" >&2
  exit 1
}
mv -n "$start_tmp" "$parent_session_start"
[[ ! -e "$start_tmp" ]] || {
  echo "could not atomically start the parent validation session" >&2
  exit 1
}
start_tmp=
require_private_file "$parent_session_start" || {
  echo "parent-session start receipt is missing or unsafe" >&2
  exit 1
}
session_start_digest="$(jq -er '.receipt_digest' "$parent_session_start")"
session_started=true

exit 0
