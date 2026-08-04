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
: "${MINCO_MULTI_RELEASE_EXECUTION_MODE:=validation_only}"

[[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "01-prior-initial" ]] || {
  echo "only the exact started first phase may enter a parent session" >&2
  exit 1
}
case "$MINCO_MULTI_RELEASE_EXECUTION_MODE" in
  validation_only)
    [[ -z "${MINCO_MULTI_RELEASE_PROVIDER_ACTION:-}" ]] || {
      echo "validation-only parent sessions do not accept a provider action" >&2
      exit 1
    }
    ;;
  provider_identity_preflight)
    [[ "${MINCO_MULTI_RELEASE_PROVIDER_ACTION:-}" == "plan" ||
      "${MINCO_MULTI_RELEASE_PROVIDER_ACTION:-}" == "execute" ]] || {
      echo "provider identity preflight accepts only plan or execute" >&2
      exit 1
    }
    ;;
  provider_resource_preflight)
    [[ "${MINCO_MULTI_RELEASE_PROVIDER_ACTION:-}" == "plan" ||
      "${MINCO_MULTI_RELEASE_PROVIDER_ACTION:-}" == "execute" ]] || {
      echo "provider resource preflight accepts only plan or execute" >&2
      exit 1
    }
    ;;
  *)
    echo "multi-release execution mode is outside the fixed parent policy" >&2
    exit 1
    ;;
esac

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
provider_entry_attempted=false
provider_entry_plan_digest=
provider_identity_verified=false
provider_resources_absent=false

build_parent_session_payload() {
  local state="$1"
  local output_path="$2"
  local cleanup_action
  local cleanup_state
  local execution_provider_state
  local external_aws_contact
  local authority_json
  local controller_json
  local phase_json

  case "$state" in
    started)
      cleanup_action="none_before_provider_boundary"
      cleanup_state="installed"
      execution_provider_state="not_entered"
      external_aws_contact=false
      ;;
    validated)
      cleanup_action="none_provider_boundary_not_entered"
      cleanup_state="disarmed"
      execution_provider_state="not_entered"
      external_aws_contact=false
      ;;
    provider_identity_verified)
      cleanup_action="none_read_only_identity_preflight"
      cleanup_state="disarmed"
      execution_provider_state="identity_verified"
      external_aws_contact=true
      ;;
    provider_resources_absent)
      cleanup_action="none_read_only_resource_preflight"
      cleanup_state="disarmed"
      execution_provider_state="resources_absent"
      external_aws_contact=true
      ;;
    failed)
      if [[ "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == \
        "provider_resource_preflight" ]]; then
        cleanup_action="none_read_only_resource_preflight"
        execution_provider_state="resource_state_unverified"
      else
        cleanup_action="none_read_only_identity_preflight"
        execution_provider_state="identity_unverified"
      fi
      cleanup_state="disarmed"
      external_aws_contact=true
      ;;
    *)
      echo "parent-session state is outside the fixed policy" >&2
      return 1
      ;;
  esac
  authority_json="$(jq -c '.authority' "$phase_start_receipt")"
  controller_json="$(jq -c '.controller' "$phase_start_receipt")"
  phase_json="$(jq -c \
    --arg start_receipt_digest "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
    '.phase + {start_receipt_digest: $start_receipt_digest}' \
    "$phase_start_receipt")"
  jq -n \
    --arg cleanup_action "$cleanup_action" \
    --arg cleanup_state "$cleanup_state" \
    --arg execution_mode "$MINCO_MULTI_RELEASE_EXECUTION_MODE" \
    --arg execution_provider_state "$execution_provider_state" \
    --arg provider_entry_plan_digest "$provider_entry_plan_digest" \
    --arg session_start_receipt_digest "$session_start_digest" \
    --arg state "$state" \
    --argjson external_aws_contact "$external_aws_contact" \
    --argjson authority "$authority_json" \
    --argjson controller "$controller_json" \
    --argjson phase "$phase_json" \
    '{
      schema_version: 1,
      operation: "multi_release_parent_session",
      state: $state,
      external_aws_contact: $external_aws_contact,
      controller: $controller,
      authority: $authority,
      phase: $phase,
      execution: {
        mode: $execution_mode,
        provider_entry_plan_digest: (
          if $provider_entry_plan_digest == ""
          then null
          else $provider_entry_plan_digest
          end
        ),
        provider_state: $execution_provider_state
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

build_provider_entry_plan() {
  local output_path="$1"
  local authority_json
  local controller_json
  local phase_json

  authority_json="$(jq -c '.authority' "$phase_start_receipt")"
  controller_json="$(jq -c '.controller' "$phase_start_receipt")"
  phase_json="$(jq -c \
    --arg start_receipt_digest "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
    '{
      id: .phase.id,
      projection_digest: .phase.projection_digest,
      source_revision: .phase.source_revision,
      start_receipt_digest: $start_receipt_digest
    }' "$phase_start_receipt")"
  jq -n \
    --arg expected_region "$authority_region" \
    --argjson authority "$authority_json" \
    --argjson controller "$controller_json" \
    --argjson phase "$phase_json" \
    '{
      schema_version: 1,
      operation: "multi_release_provider_entry",
      external_aws_contact: false,
      controller: $controller,
      authority: $authority,
      phase: $phase,
      provider: {
        action: "sts_get_caller_identity",
        expected_region: $expected_region,
        mutation: false,
        secrets_requested: false
      },
      cleanup: {
        owner: "parent_controller",
        required: true,
        trap_count: 1
      }
    }' >"$output_path"
}

build_resource_preflight_plan() {
  local output_path="$1"
  local authority_json
  local controller_json
  local phase_json

  authority_json="$(jq -c '.authority' "$phase_start_receipt")"
  controller_json="$(jq -c '.controller' "$phase_start_receipt")"
  phase_json="$(jq -c \
    --arg start_receipt_digest "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
    '{
      id: .phase.id,
      projection_digest: .phase.projection_digest,
      source_revision: .phase.source_revision,
      start_receipt_digest: $start_receipt_digest
    }' "$phase_start_receipt")"
  jq -n \
    --arg expected_region "$authority_region" \
    --argjson authority "$authority_json" \
    --argjson controller "$controller_json" \
    --argjson phase "$phase_json" \
    '{
      schema_version: 1,
      operation: "multi_release_resource_preflight",
      external_aws_contact: false,
      controller: $controller,
      authority: $authority,
      phase: $phase,
      provider: {
        actions: [
          "sts_get_caller_identity",
          "cloudformation_describe_application_stack_absence",
          "s3_head_artifact_bucket_absence",
          "cloudformation_describe_database_stack_absence",
          "rds_describe_database_instance_absence"
        ],
        expected_region: $expected_region,
        mutation: false,
        secrets_requested: false
      },
      cleanup: {
        owner: "parent_controller",
        required: true,
        trap_count: 1
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
  local terminal_state

  trap - EXIT INT TERM
  terminal_state=
  if [[ "$session_started" == true ]]; then
    if [[ "$status" -eq 0 &&
      "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "validation_only" ]]; then
      terminal_state=validated
    elif [[ "$status" -eq 0 &&
      "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_identity_preflight" &&
      "$provider_identity_verified" == true ]]; then
      terminal_state=provider_identity_verified
    elif [[ "$status" -eq 0 &&
      "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_resource_preflight" &&
      "$provider_resources_absent" == true ]]; then
      terminal_state=provider_resources_absent
    elif [[ "$provider_entry_attempted" == true ]]; then
      terminal_state=failed
      status=1
    fi
  fi
  if [[ -n "$terminal_state" ]]; then
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
      if ! build_parent_session_payload "$terminal_state" "$completion_payload"; then
        echo "could not build the parent-session terminal receipt" >&2
        status=1
      elif ! completion_tmp="$(mktemp "$phase_path/.parent-session-completion.XXXXXX")"; then
        echo "could not allocate the parent-session terminal receipt" >&2
        status=1
      elif ! seal_parent_session_receipt "$completion_payload" "$completion_tmp" ||
        ! mv -n "$completion_tmp" "$parent_session_completion" ||
        [[ -e "$completion_tmp" ]] ||
        ! require_private_file "$parent_session_completion"; then
        echo "could not atomically complete the parent session" >&2
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
authority_account_id="$(jq -er '.expected_account_id' "$authority_file")"
authority_role_arn="$(jq -er '.expected_role_arn' "$authority_file")"
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
[[ "$(shasum -a 256 "$authority_file" | awk '{print $1}')" == \
  "$actual_authority_digest" ]] || {
  echo "multi-release authority changed after validation" >&2
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

if [[ "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_identity_preflight" ]]; then
  provider_entry_plan="$validation_dir/provider-entry-plan.json"
  build_provider_entry_plan "$provider_entry_plan"
  jq -e -f scripts/aws/lib/validate-multi-release-provider-entry-plan.jq \
    "$provider_entry_plan" >/dev/null || {
    echo "multi-release provider-entry plan is outside the fixed policy" >&2
    exit 1
  }
  provider_entry_plan_digest="$(
    shasum -a 256 "$provider_entry_plan" | awk '{print $1}'
  )"
  if [[ "$MINCO_MULTI_RELEASE_PROVIDER_ACTION" == "plan" ]]; then
    cat "$provider_entry_plan"
    exit 0
  fi
  : "${MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST to the exact provider-entry plan digest}"
  [[ "$MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
    echo "provider-entry approval must be a SHA-256 digest" >&2
    exit 1
  }
  [[ "$MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST" == \
    "$provider_entry_plan_digest" ]] || {
    echo "provider-entry approval does not match the exact deterministic plan" >&2
    exit 1
  }
  require_command aws
elif [[ "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_resource_preflight" ]]; then
  [[ "$(jq -er '.mode' <<<"$authority_database_boundary")" == \
    "disposable-rds" ]] || {
    echo "provider resource preflight requires the disposable-RDS authority profile" >&2
    exit 1
  }
  resource_preflight_plan="$validation_dir/resource-preflight-plan.json"
  build_resource_preflight_plan "$resource_preflight_plan"
  jq -e -f scripts/aws/lib/validate-multi-release-resource-preflight-plan.jq \
    "$resource_preflight_plan" >/dev/null || {
    echo "multi-release resource preflight plan is outside the fixed policy" >&2
    exit 1
  }
  resource_preflight_plan_digest="$(
    shasum -a 256 "$resource_preflight_plan" | awk '{print $1}'
  )"
  if [[ "$MINCO_MULTI_RELEASE_PROVIDER_ACTION" == "plan" ]]; then
    cat "$resource_preflight_plan"
    exit 0
  fi
  : "${MINCO_APPROVE_MULTI_RELEASE_RESOURCE_PREFLIGHT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_RESOURCE_PREFLIGHT_DIGEST to the exact resource-preflight plan digest}"
  [[ "$MINCO_APPROVE_MULTI_RELEASE_RESOURCE_PREFLIGHT_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
    echo "resource-preflight approval must be a SHA-256 digest" >&2
    exit 1
  }
  [[ "$MINCO_APPROVE_MULTI_RELEASE_RESOURCE_PREFLIGHT_DIGEST" == \
    "$resource_preflight_plan_digest" ]] || {
    echo "resource-preflight approval does not match the exact deterministic plan" >&2
    exit 1
  }
  provider_entry_plan_digest="$resource_preflight_plan_digest"
  require_command aws
fi

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

if [[ "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_identity_preflight" ||
  "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_resource_preflight" ]]; then
  provider_entry_attempted=true
  identity="$(
    AWS_PROFILE="$authority_profile" \
    AWS_REGION="$authority_region" \
    AWS_PAGER="" \
      command aws --no-cli-pager --region "$authority_region" \
      sts get-caller-identity \
      --query '{Account:Account,Arn:Arn,UserId:UserId}' \
      --output json
  )"
  jq -e -s '
    length == 1
    and (.[0] | keys) == ["Account", "Arn", "UserId"]
    and (.[0].Account | type == "string" and test("^[0-9]{12}$"))
    and (.[0].Arn | type == "string" and length > 0)
    and (.[0].UserId | type == "string" and length > 0)
  ' <<<"$identity" >/dev/null || {
    echo "provider identity response is outside the fixed shape" >&2
    exit 1
  }
  account_id="$(jq -er '.Account' <<<"$identity")"
  caller_arn="$(jq -er '.Arn' <<<"$identity")"
  case "$caller_arn" in
    arn:aws*:iam::"$account_id":role/*)
      caller_role_arn="$caller_arn"
      ;;
    arn:aws*:sts::"$account_id":assumed-role/*/*)
      partition="${caller_arn%%:sts::*}"
      role_session="${caller_arn#*:assumed-role/}"
      role_name="${role_session%%/*}"
      caller_role_arn="${partition}:iam::${account_id}:role/${role_name}"
      ;;
    *)
      echo "provider identity preflight requires an IAM role or assumed-role caller" >&2
      exit 1
      ;;
  esac
  [[ "$account_id" == "$authority_account_id" &&
    "$caller_role_arn" == "$authority_role_arn" ]] || {
    echo "provider identity does not match the exact account and role authority" >&2
    exit 1
  }
  unset \
    account_id authority_account_id authority_role_arn caller_arn \
    caller_role_arn identity partition role_name role_session
  provider_identity_verified=true
fi

if [[ "$MINCO_MULTI_RELEASE_EXECUTION_MODE" == "provider_resource_preflight" ]]; then
  run_id="$(jq -er '.authority.run_id' "$controller_receipt")"
  run_suffix="$(
    printf '%s' "$run_id" | shasum -a 256 | awk '{print substr($1, 1, 12)}'
  )"
  application_stack_name="minco-smoke-$run_suffix"
  artifact_bucket_name="${application_stack_name,,}"
  database_stack_name="$(jq -er '.rds_stack_name' <<<"$authority_database_boundary")"
  database_instance_id="$(jq -er '.instance_id' <<<"$authority_database_boundary")"

  application_stack_error="$validation_dir/application-stack-error.txt"
  if AWS_PROFILE="$authority_profile" AWS_REGION="$authority_region" AWS_PAGER="" \
    command aws --no-cli-pager --cli-error-format json --region "$authority_region" \
      cloudformation describe-stacks \
      --stack-name "$application_stack_name" \
      >/dev/null 2>"$application_stack_error"; then
    echo "refusing to use a pre-existing application stack" >&2
    exit 1
  else
    application_stack_status=$?
  fi
  if [[ "${application_stack_status:-0}" -ne 254 ]] ||
    ! jq -e '
      keys == ["Code", "Message"]
      and .Code == "ValidationError"
      and (.Message | type == "string")
    ' "$application_stack_error" >/dev/null; then
    echo "could not prove the application stack is absent" >&2
    exit 1
  fi

  bucket_error="$validation_dir/artifact-bucket-error.txt"
  if AWS_PROFILE="$authority_profile" AWS_REGION="$authority_region" AWS_PAGER="" \
    command aws --no-cli-pager --cli-error-format json --region "$authority_region" \
      s3api head-bucket \
      --bucket "$artifact_bucket_name" \
      >/dev/null 2>"$bucket_error"; then
    echo "refusing to use a pre-existing artifact bucket" >&2
    exit 1
  else
    bucket_status=$?
  fi
  if [[ "${bucket_status:-0}" -ne 254 ]] ||
    ! jq -e '
      keys == ["Code", "Message"]
      and .Code == "404"
      and (.Message | type == "string")
    ' "$bucket_error" >/dev/null; then
    echo "could not prove the artifact bucket is absent" >&2
    exit 1
  fi

  database_stack_error="$validation_dir/database-stack-error.txt"
  if AWS_PROFILE="$authority_profile" AWS_REGION="$authority_region" AWS_PAGER="" \
    command aws --no-cli-pager --cli-error-format json --region "$authority_region" \
      cloudformation describe-stacks \
      --stack-name "$database_stack_name" \
      >/dev/null 2>"$database_stack_error"; then
    echo "refusing to use a pre-existing database stack" >&2
    exit 1
  else
    database_stack_status=$?
  fi
  if [[ "${database_stack_status:-0}" -ne 254 ]] ||
    ! jq -e '
      keys == ["Code", "Message"]
      and .Code == "ValidationError"
      and (.Message | type == "string")
    ' "$database_stack_error" >/dev/null; then
    echo "could not prove the database stack is absent" >&2
    exit 1
  fi

  database_instance_error="$validation_dir/database-instance-error.txt"
  if AWS_PROFILE="$authority_profile" AWS_REGION="$authority_region" AWS_PAGER="" \
    command aws --no-cli-pager --cli-error-format json --region "$authority_region" \
      rds describe-db-instances \
      --db-instance-identifier "$database_instance_id" \
      >/dev/null 2>"$database_instance_error"; then
    echo "refusing to use a pre-existing database instance" >&2
    exit 1
  else
    database_instance_status=$?
  fi
  if [[ "${database_instance_status:-0}" -ne 254 ]] ||
    ! jq -e '
      keys == ["Code", "Message"]
      and .Code == "DBInstanceNotFound"
      and (.Message | type == "string")
    ' "$database_instance_error" >/dev/null; then
    echo "could not prove the database instance is absent" >&2
    exit 1
  fi

  unset \
    application_stack_name application_stack_status artifact_bucket_name \
    bucket_status database_instance_id database_instance_status \
    database_stack_name database_stack_status run_id run_suffix
  provider_resources_absent=true
fi

exit 0
