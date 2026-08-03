#!/usr/bin/env bash
set -euo pipefail
umask 077

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in cp git jq mkdir mktemp mv shasum; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_PLAN_FILE:?set MINCO_MULTI_RELEASE_PLAN_FILE to the exact whole-run plan}"
: "${MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST to the whole-run plan SHA-256}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"

[[ -f "$MINCO_MULTI_RELEASE_PLAN_FILE" &&
  ! -L "$MINCO_MULTI_RELEASE_PLAN_FILE" ]] || {
  echo "multi-release plan must be a regular non-symlink file" >&2
  exit 1
}
[[ "$MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
  echo "multi-release plan approval must be a SHA-256 digest" >&2
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

actual_plan_digest="$(
  shasum -a 256 "$MINCO_MULTI_RELEASE_PLAN_FILE" | awk '{print $1}'
)"
[[ "$actual_plan_digest" == "$MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST" ]] || {
  echo "multi-release plan approval does not match the exact document digest" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq \
  "$MINCO_MULTI_RELEASE_PLAN_FILE" >/dev/null || {
  echo "multi-release plan is missing or broader than the fixed controller policy" >&2
  exit 1
}

planned_controller_root="$(
  jq -er '.phases[1].source.root' "$MINCO_MULTI_RELEASE_PLAN_FILE"
)"
[[ "$repo_root" == "$planned_controller_root" ]] || {
  echo "multi-release controller must run from the exact current checkout" >&2
  exit 1
}

evidence_root="$(jq -er '.evidence_root' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
[[ ! -e "$evidence_root" && ! -L "$evidence_root" ]] || {
  echo "multi-release evidence root must not already exist" >&2
  exit 1
}

validation_dir="$(mktemp -d)"
staging_root=
cleanup_local_state() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$staging_root" && -d "$staging_root" && ! -L "$staging_root" ]]; then
    rm -r -- "$staging_root"
  fi
  if [[ -d "$validation_dir" && ! -L "$validation_dir" ]]; then
    rm -r -- "$validation_dir"
  fi
  exit "$status"
}
trap cleanup_local_state EXIT INT TERM

phase_ids=(01-prior-initial 02-current 03-prior-rollback)
for phase_id in "${phase_ids[@]}"; do
  MINCO_MULTI_RELEASE_PHASE_ID="$phase_id" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$MINCO_MULTI_RELEASE_PLAN_FILE" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$actual_plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$MINCO_REHEARSAL_AUTHORITY_FILE" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
    scripts/aws/plan-multi-release-phase.sh \
      >"$validation_dir/$phase_id.json"
done
[[ "$(shasum -a 256 "$MINCO_REHEARSAL_AUTHORITY_FILE" | awk '{print $1}')" == \
  "$actual_authority_digest" ]] || {
  echo "multi-release authority changed during initialization" >&2
  exit 1
}

evidence_parent="$(dirname "$evidence_root")"
mkdir -p "$evidence_parent"
[[ -d "$evidence_parent" && ! -L "$evidence_parent" ]] || {
  echo "multi-release evidence parent must be a non-symlink directory" >&2
  exit 1
}
canonical_parent="$(cd "$evidence_parent" && pwd -P)"
[[ "$canonical_parent" == "$evidence_parent" ]] || {
  echo "multi-release evidence parent changed after plan validation" >&2
  exit 1
}
[[ ! -e "$evidence_root" && ! -L "$evidence_root" ]] || {
  echo "multi-release evidence root appeared during initialization" >&2
  exit 1
}

staging_root="$evidence_parent/.$(basename "$evidence_root").initialize.$$"
[[ ! -e "$staging_root" && ! -L "$staging_root" ]] || {
  echo "multi-release initialization staging boundary already exists" >&2
  exit 1
}
mkdir -m 700 "$staging_root"
mkdir -m 700 "$staging_root/control"
mkdir -m 700 "$staging_root/control/phases"

sealed_plan="$staging_root/control/multi-release-plan.json"
cp "$MINCO_MULTI_RELEASE_PLAN_FILE" "$sealed_plan"
chmod 600 "$sealed_plan"
[[ "$(shasum -a 256 "$sealed_plan" | awk '{print $1}')" == \
  "$actual_plan_digest" ]] || {
  echo "sealed multi-release plan changed during initialization" >&2
  exit 1
}

authority_receipt="$staging_root/control/authority-receipt.json"
write_multi_release_rehearsal_authority_receipt \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$authority_receipt"

phase_receipts='[]'
for phase_id in "${phase_ids[@]}"; do
  projection_path="$staging_root/control/phases/$phase_id.json"
  cp "$validation_dir/$phase_id.json" "$projection_path"
  chmod 600 "$projection_path"
  projection_digest="$(shasum -a 256 "$projection_path" | awk '{print $1}')"
  evidence_namespace="$(jq -er '.evidence.namespace' "$projection_path")"
  phase_receipts="$(
    jq -c \
      --arg phase_id "$phase_id" \
      --arg evidence_namespace "$evidence_namespace" \
      --arg projection_digest "$projection_digest" \
      '. + [{
        id: $phase_id,
        evidence_namespace: $evidence_namespace,
        projection_digest: $projection_digest,
        state: "pending"
      }]' <<<"$phase_receipts"
  )"
done

authority_json="$(jq -c '.authority' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
source_revisions_json="$(
  jq -c '{
    current: .phases[1].source.revision,
    prior: .phases[0].source.revision
  }' "$MINCO_MULTI_RELEASE_PLAN_FILE"
)"
controller_payload="$staging_root/control/controller-payload.json"
jq -n \
  --arg plan_digest "$actual_plan_digest" \
  --argjson authority "$authority_json" \
  --argjson source_revisions "$source_revisions_json" \
  --argjson phases "$phase_receipts" \
  '{
    schema_version: 1,
    operation: "multi_release_controller_rehearsal",
    state: "initialized",
    external_aws_contact: false,
    plan_digest: $plan_digest,
    authority: $authority,
    source_revisions: $source_revisions,
    execution: {
      phase_sequence: [
        "01-prior-initial",
        "02-current",
        "03-prior-rollback"
      ],
      next_phase: "01-prior-initial",
      phases: $phases
    },
    provider_boundary: {
      shared_stack_state: "not_created",
      artifact_bucket_state: "not_created"
    },
    cleanup: {
      owner: "parent_controller",
      required: true,
      state: "pending",
      trap_count: 1
    }
  }' >"$controller_payload"
chmod 600 "$controller_payload"

receipt_digest="$(
  jq -cS . "$controller_payload" | shasum -a 256 | awk '{print $1}'
)"
controller_receipt="$staging_root/control/controller-receipt.json"
jq --arg receipt_digest "$receipt_digest" \
  '. + {receipt_digest: $receipt_digest}' \
  "$controller_payload" >"$controller_receipt"
chmod 600 "$controller_receipt"
rm -f -- "$controller_payload"
jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null || {
  echo "multi-release controller receipt is outside the initialized policy" >&2
  exit 1
}
[[ "$(
  jq -cS 'del(.receipt_digest)' "$controller_receipt" |
    shasum -a 256 | awk '{print $1}'
)" == "$receipt_digest" ]] || {
  echo "multi-release controller receipt digest is invalid" >&2
  exit 1
}

[[ ! -e "$evidence_root" && ! -L "$evidence_root" ]] || {
  echo "multi-release evidence root appeared before atomic initialization" >&2
  exit 1
}
mv -n "$staging_root" "$evidence_root"
[[ ! -e "$staging_root" &&
  -f "$evidence_root/control/controller-receipt.json" &&
  ! -L "$evidence_root/control/controller-receipt.json" ]] || {
  echo "could not atomically initialize the multi-release evidence boundary" >&2
  exit 1
}
staging_root=

cat "$evidence_root/control/controller-receipt.json"
