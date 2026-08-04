#!/usr/bin/env bash
set -euo pipefail
umask 077

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in cmp cp git jq mkdir mktemp mv shasum stat; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_EVIDENCE_ROOT:?set MINCO_MULTI_RELEASE_EVIDENCE_ROOT to the initialized whole-run evidence directory}"
: "${MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST to the initialized controller receipt digest}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_MULTI_RELEASE_PHASE_ID:?set MINCO_MULTI_RELEASE_PHASE_ID to the exact next phase ID}"

case "$MINCO_MULTI_RELEASE_PHASE_ID" in
  01-prior-initial)
    previous_phase_id=
    ;;
  02-current)
    previous_phase_id=01-prior-initial
    ;;
  03-prior-rollback)
    previous_phase_id=02-current
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
[[ "$(minco_file_mode "$evidence_root")" == "700" ]] || {
  echo "multi-release evidence root must remain mode 0700" >&2
  exit 1
}
[[ "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
  echo "controller receipt approval must be a SHA-256 digest" >&2
  exit 1
}
[[ -f "$MINCO_REHEARSAL_AUTHORITY_FILE" &&
  ! -L "$MINCO_REHEARSAL_AUTHORITY_FILE" ]] || {
  echo "multi-release authority must be a regular non-symlink file" >&2
  exit 1
}
actual_authority_digest="$(
  shasum -a 256 "$MINCO_REHEARSAL_AUTHORITY_FILE" | awk '{print $1}'
)"
[[ "$actual_authority_digest" == "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" ]] || {
  echo "multi-release authority approval does not match the exact document digest" >&2
  exit 1
}

control_root="$evidence_root/control"
controller_receipt="$control_root/controller-receipt.json"
sealed_plan="$control_root/multi-release-plan.json"
sealed_projection="$control_root/phases/$MINCO_MULTI_RELEASE_PHASE_ID.json"
if [[ -z "$previous_phase_id" ]]; then
  require_exact_entries "$evidence_root" control || {
    echo "initialized multi-release evidence root contains unsealed state" >&2
    exit 1
  }
else
  require_exact_entries "$evidence_root" control phases || {
    echo "multi-release evidence root contains unsealed transition state" >&2
    exit 1
  }
fi
require_exact_entries "$control_root" \
  authority-receipt.json controller-receipt.json multi-release-plan.json phases || {
  echo "initialized multi-release control directory contains unsealed state" >&2
  exit 1
}
require_exact_entries "$control_root/phases" \
  01-prior-initial.json 02-current.json 03-prior-rollback.json || {
  echo "initialized phase projections contain unsealed state" >&2
  exit 1
}
for control_file in "$controller_receipt" "$sealed_plan" "$sealed_projection"; do
  [[ -f "$control_file" && ! -L "$control_file" ]] || {
    echo "initialized multi-release control evidence is missing or unsafe" >&2
    exit 1
  }
done
[[ -d "$control_root" && ! -L "$control_root" &&
  "$(minco_file_mode "$control_root")" == "700" ]] || {
  echo "initialized multi-release control directory must remain private" >&2
  exit 1
}
[[ -d "$control_root/phases" && ! -L "$control_root/phases" &&
  "$(minco_file_mode "$control_root/phases")" == "700" ]] || {
  echo "initialized phase-projection directory must remain private" >&2
  exit 1
}
for control_file in \
  "$control_root/authority-receipt.json" \
  "$controller_receipt" \
  "$sealed_plan" \
  "$control_root/phases/01-prior-initial.json" \
  "$control_root/phases/02-current.json" \
  "$control_root/phases/03-prior-rollback.json"; do
  [[ -f "$control_file" && ! -L "$control_file" &&
    "$(minco_file_mode "$control_file")" == "600" ]] || {
    echo "initialized multi-release control evidence must remain mode 0600" >&2
    exit 1
  }
done

jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null || {
  echo "initialized controller receipt is outside the fixed policy" >&2
  exit 1
}
controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
[[ "$controller_receipt_digest" == \
  "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" ]] || {
  echo "controller receipt approval does not match the initialized receipt" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$controller_receipt" |
    shasum -a 256 | awk '{print $1}'
)" == "$controller_receipt_digest" ]] || {
  echo "initialized controller receipt digest is invalid" >&2
  exit 1
}
for sealed_phase_id in \
  01-prior-initial 02-current 03-prior-rollback; do
  sealed_phase_projection="$control_root/phases/$sealed_phase_id.json"
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
[[ "$(jq -er '.state' "$controller_receipt")" == "initialized" ]] || {
  echo "controller receipt does not permit phase execution" >&2
  exit 1
}
if [[ -z "$previous_phase_id" ]]; then
  [[ "$(jq -er '.execution.next_phase' "$controller_receipt")" == \
    "$MINCO_MULTI_RELEASE_PHASE_ID" ]] || {
    echo "controller receipt does not permit the initial phase" >&2
    exit 1
  }
fi

plan_digest="$(shasum -a 256 "$sealed_plan" | awk '{print $1}')"
[[ "$plan_digest" == "$(jq -er '.plan_digest' "$controller_receipt")" ]] || {
  echo "sealed multi-release plan does not match the controller receipt" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq \
  "$sealed_plan" >/dev/null || {
  echo "sealed multi-release plan is outside the fixed controller policy" >&2
  exit 1
}
[[ "$repo_root" == "$(jq -er '.phases[1].source.root' "$sealed_plan")" ]] || {
  echo "multi-release phase must begin from the exact current controller checkout" >&2
  exit 1
}
[[ "$actual_authority_digest" == "$(jq -er '.authority.approval_digest' "$controller_receipt")" ]] || {
  echo "multi-release authority does not match the initialized controller" >&2
  exit 1
}

previous_completion_digest=
if [[ -n "$previous_phase_id" ]]; then
  : "${MINCO_APPROVE_PREVIOUS_PHASE_COMPLETION_DIGEST:?set MINCO_APPROVE_PREVIOUS_PHASE_COMPLETION_DIGEST to the exact predecessor completion receipt digest}"
  [[ "$MINCO_APPROVE_PREVIOUS_PHASE_COMPLETION_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
    echo "previous phase completion approval must be a SHA-256 digest" >&2
    exit 1
  }
  phases_root="$evidence_root/phases"
  [[ -d "$phases_root" && ! -L "$phases_root" &&
    "$(minco_file_mode "$phases_root")" == "700" ]] || {
    echo "multi-release phase boundary must remain private" >&2
    exit 1
  }
  if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "02-current" ]]; then
    require_exact_entries "$phases_root" 01-prior-initial || {
      echo "second phase requires only the completed first phase" >&2
      exit 1
    }
    require_exact_entries "$phases_root/01-prior-initial" \
      parent-session-completion-receipt.json \
      parent-session-start-receipt.json \
      phase-completion-receipt.json \
      phase-projection.json \
      phase-start-receipt.json || {
      echo "first phase evidence is incomplete or unsealed" >&2
      exit 1
    }
  else
    require_exact_entries "$phases_root" 01-prior-initial 02-current || {
      echo "rollback phase requires exactly two completed phases" >&2
      exit 1
    }
    require_exact_entries "$phases_root/02-current" \
      phase-completion-receipt.json \
      phase-projection.json \
      phase-start-receipt.json || {
      echo "current phase evidence is incomplete or unsealed" >&2
      exit 1
    }
  fi
  previous_completion="$phases_root/$previous_phase_id/phase-completion-receipt.json"
  [[ -f "$previous_completion" && ! -L "$previous_completion" &&
    "$(minco_file_mode "$previous_completion")" == "600" ]] || {
    echo "previous phase completion receipt is missing or unsafe" >&2
    exit 1
  }
  jq -e -f scripts/aws/lib/validate-multi-release-phase-completion-receipt.jq \
    "$previous_completion" >/dev/null || {
    echo "previous phase completion receipt is outside the fixed policy" >&2
    exit 1
  }
  previous_completion_digest="$(jq -er '.receipt_digest' "$previous_completion")"
  [[ "$previous_completion_digest" == \
      "$MINCO_APPROVE_PREVIOUS_PHASE_COMPLETION_DIGEST" &&
    "$(jq -er '.transition.next_phase' "$previous_completion")" == \
      "$MINCO_MULTI_RELEASE_PHASE_ID" &&
    "$(jq -er '.state' "$previous_completion")" == "succeeded" &&
    "$(
      jq -cS 'del(.receipt_digest)' "$previous_completion" |
        shasum -a 256 | awk '{print $1}'
    )" == "$previous_completion_digest" ]] || {
    echo "previous phase completion does not authorize the requested transition" >&2
    exit 1
  }
  if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "03-prior-rollback" ]]; then
    phase_one_completion="$phases_root/01-prior-initial/phase-completion-receipt.json"
    [[ -f "$phase_one_completion" && ! -L "$phase_one_completion" &&
      "$(minco_file_mode "$phase_one_completion")" == "600" ]] || {
      echo "initial phase completion receipt is missing or unsafe" >&2
      exit 1
    }
    jq -e -f scripts/aws/lib/validate-multi-release-phase-completion-receipt.jq \
      "$phase_one_completion" >/dev/null || {
      echo "initial phase completion receipt is outside the fixed policy" >&2
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
  fi
fi

validation_dir="$(mktemp -d)"
staging_path=
cleanup_local_state() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$staging_path" && -d "$staging_path" && ! -L "$staging_path" ]]; then
    rm -r -- "$staging_path"
  fi
  if [[ -d "$validation_dir" && ! -L "$validation_dir" ]]; then
    rm -r -- "$validation_dir"
  fi
  exit "$status"
}
trap cleanup_local_state EXIT INT TERM

regenerated_authority_receipt="$validation_dir/authority-receipt.json"
write_multi_release_rehearsal_authority_receipt \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$actual_authority_digest" \
  "$regenerated_authority_receipt"
cmp -s "$control_root/authority-receipt.json" \
  "$regenerated_authority_receipt" || {
  echo "sealed authority receipt does not match the approved authority" >&2
  exit 1
}

reprojected_phase="$validation_dir/$MINCO_MULTI_RELEASE_PHASE_ID.json"
MINCO_MULTI_RELEASE_PLAN_FILE="$sealed_plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$MINCO_REHEARSAL_AUTHORITY_FILE" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$actual_authority_digest" \
MINCO_MULTI_RELEASE_PHASE_ID="$MINCO_MULTI_RELEASE_PHASE_ID" \
  scripts/aws/plan-multi-release-phase.sh >"$reprojected_phase"

projection_digest="$(shasum -a 256 "$sealed_projection" | awk '{print $1}')"
[[ "$projection_digest" == "$(
  jq -er \
    --arg phase_id "$MINCO_MULTI_RELEASE_PHASE_ID" \
    '.execution.phases[] | select(.id == $phase_id) | .projection_digest' \
    "$controller_receipt"
)" && "$projection_digest" == "$(
  shasum -a 256 "$reprojected_phase" | awk '{print $1}'
)" ]] || {
  echo "phase projection does not match the initialized controller" >&2
  exit 1
}

phase_namespace="$(jq -er '.evidence.namespace' "$sealed_projection")"
phase_path="$(jq -er '.evidence.path' "$sealed_projection")"
[[ "$phase_namespace" == "phases/$MINCO_MULTI_RELEASE_PHASE_ID" &&
  "$phase_path" == "$evidence_root/$phase_namespace" ]] || {
  echo "phase evidence path does not match the initialized namespace" >&2
  exit 1
}
[[ ! -e "$phase_path" && ! -L "$phase_path" ]] || {
  echo "multi-release phase evidence namespace already exists" >&2
  exit 1
}

phases_root="$evidence_root/phases"
if [[ -z "$previous_phase_id" ]]; then
  [[ ! -e "$phases_root" && ! -L "$phases_root" ]] || {
    echo "multi-release phases boundary must not already exist before the first phase" >&2
    exit 1
  }
  staging_path="$evidence_root/.phases.start.$$"
  staging_phase_path="$staging_path/$MINCO_MULTI_RELEASE_PHASE_ID"
  mkdir -m 700 "$staging_path"
  mkdir -m 700 "$staging_phase_path"
else
  staging_path="$phases_root/.$MINCO_MULTI_RELEASE_PHASE_ID.start.$$"
  staging_phase_path="$staging_path"
  mkdir -m 700 "$staging_path"
fi
[[ -d "$staging_path" && ! -L "$staging_path" ]] || {
  echo "multi-release phase-start staging boundary is unsafe" >&2
  exit 1
}
cp "$sealed_projection" "$staging_phase_path/phase-projection.json"
chmod 600 "$staging_phase_path/phase-projection.json"

authority_json="$(jq -c '.authority' "$controller_receipt")"
phase_json="$(jq -c '.phase' "$sealed_projection")"
phase_start_payload="$staging_phase_path/phase-start-payload.json"
jq -n \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg plan_digest "$plan_digest" \
  --arg projection_digest "$projection_digest" \
  --argjson authority "$authority_json" \
  --argjson phase "$phase_json" \
  '{
    schema_version: 1,
    operation: "multi_release_phase_start",
    state: "started",
    external_aws_contact: false,
    controller: {
      receipt_digest: $controller_receipt_digest,
      plan_digest: $plan_digest
    },
    authority: $authority,
    phase: {
      id: $phase.id,
      release: $phase.release,
      source_revision: $phase.source.revision,
      evidence_namespace: $phase.evidence_namespace,
      projection_digest: $projection_digest,
      stack_action: $phase.stack_action,
      change_set_review_policy: $phase.change_set_review_policy
    },
    cleanup: {
      owner: "parent_controller",
      required: true,
      inner_phase_cleanup: false
    }
  }' >"$phase_start_payload"
chmod 600 "$phase_start_payload"

phase_start_digest="$(
  jq -cS . "$phase_start_payload" | shasum -a 256 | awk '{print $1}'
)"
phase_start_receipt="$staging_phase_path/phase-start-receipt.json"
jq --arg receipt_digest "$phase_start_digest" \
  '. + {receipt_digest: $receipt_digest}' \
  "$phase_start_payload" >"$phase_start_receipt"
chmod 600 "$phase_start_receipt"
rm -f -- "$phase_start_payload"
jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_start_receipt" >/dev/null || {
  echo "multi-release phase-start receipt is outside the fixed policy" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$phase_start_receipt" |
    shasum -a 256 | awk '{print $1}'
)" == "$phase_start_digest" ]] || {
  echo "multi-release phase-start receipt digest is invalid" >&2
  exit 1
}

staging_name="${staging_path##*/}"
if [[ -z "$previous_phase_id" ]]; then
  require_exact_entries "$evidence_root" "$staging_name" control || {
    echo "multi-release evidence root changed during initial phase start" >&2
    exit 1
  }
  [[ ! -e "$phases_root" && ! -L "$phases_root" ]] || {
    echo "multi-release phases boundary appeared before atomic start" >&2
    exit 1
  }
  mv -n "$staging_path" "$phases_root"
else
  if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "02-current" ]]; then
    require_exact_entries "$phases_root" "$staging_name" 01-prior-initial || {
      echo "multi-release first transition changed during phase start" >&2
      exit 1
    }
  else
    require_exact_entries "$phases_root" \
      "$staging_name" 01-prior-initial 02-current || {
      echo "multi-release rollback transition changed during phase start" >&2
      exit 1
    }
  fi
  mv -n "$staging_path" "$phase_path"
fi
[[ ! -e "$staging_path" &&
  -f "$phase_path/phase-start-receipt.json" &&
  ! -L "$phase_path/phase-start-receipt.json" ]] || {
  echo "could not atomically begin the multi-release phase" >&2
  exit 1
}
staging_path=
require_exact_entries "$evidence_root" control phases || {
  echo "multi-release evidence root is invalid after phase start" >&2
  exit 1
}
if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "01-prior-initial" ]]; then
  require_exact_entries "$phases_root" 01-prior-initial || exit 1
elif [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "02-current" ]]; then
  require_exact_entries "$phases_root" 01-prior-initial 02-current || exit 1
else
  require_exact_entries "$phases_root" \
    01-prior-initial 02-current 03-prior-rollback || exit 1
fi

cat "$phase_path/phase-start-receipt.json"
