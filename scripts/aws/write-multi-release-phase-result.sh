#!/usr/bin/env bash
set -euo pipefail
umask 077

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
controller_root="$(minco_repo_root)"
cd "$controller_root"

for command in awk cmp git jq mktemp mv shasum stat; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_PHASE_ID:?set MINCO_MULTI_RELEASE_PHASE_ID to the exact phase ID}"
: "${MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT:?set MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT to the exact clean source checkout}"
: "${MINCO_MULTI_RELEASE_PHASE_START_RECEIPT:?set MINCO_MULTI_RELEASE_PHASE_START_RECEIPT to the exact phase-start receipt}"
: "${MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST to the exact phase-start receipt digest}"
: "${MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT:?set MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT to a new absolute result path}"

case "$MINCO_MULTI_RELEASE_PHASE_ID" in
  01-prior-initial|02-current)
    [[ -z "${MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST:-}" &&
      -z "${MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT:-}" ]] || {
      echo "non-rollback phase results do not accept rollback evidence" >&2
      exit 1
    }
    ;;
  03-prior-rollback)
    : "${MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST:?set MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST to the initial phase exact release manifest}"
    : "${MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT:?set MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT to the compatible local assessment}"
    ;;
  *)
    echo "multi-release phase ID is outside the fixed sequence" >&2
    exit 1
    ;;
esac

canonical_private_file() {
  local label="$1"
  local path="$2"
  local canonical_parent
  local canonical_path
  local mode

  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] || {
    printf '%s must be an absolute regular non-symlink file\n' "$label" >&2
    return 1
  }
  canonical_parent="$(cd "$(dirname "$path")" && pwd -P)"
  canonical_path="$canonical_parent/$(basename "$path")"
  [[ "$canonical_path" == "$path" ]] || {
    printf '%s must be canonical\n' "$label" >&2
    return 1
  }
  mode="$(minco_file_mode "$path")"
  (((8#$mode & 8#077) == 0)) || {
    printf '%s must not be group/world accessible\n' "$label" >&2
    return 1
  }
  printf '%s\n' "$canonical_path"
}

[[ "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT" == /* &&
  -d "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT" &&
  ! -L "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT" &&
  -f "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT/minco.toml" &&
  ! -L "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT/minco.toml" ]] || {
  echo "phase source root must be an absolute existing checkout" >&2
  exit 1
}
source_root="$(cd "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT" && pwd -P)"
[[ "$source_root" == "$MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT" ]] || {
  echo "phase source root must be canonical" >&2
  exit 1
}
if [[ -d "$source_root/.jj" && ! -L "$source_root/.jj" ]] &&
  command -v jj >/dev/null; then
  source_status="$(cd "$source_root" && jj diff --summary)"
elif [[ (-d "$source_root/.git" || -f "$source_root/.git") &&
  ! -L "$source_root/.git" ]] &&
  git -C "$source_root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  source_status="$(git -C "$source_root" status --porcelain=v1 --untracked-files=normal)"
else
  echo "phase source root must own JJ or Git metadata" >&2
  exit 1
fi
[[ -z "$source_status" ]] || {
  echo "phase source root must remain clean while sealing provider evidence" >&2
  exit 1
}
source_revision="$(cd "$source_root" && current_source_revision)"

phase_start_receipt="$(canonical_private_file \
  "phase-start receipt" "$MINCO_MULTI_RELEASE_PHASE_START_RECEIPT")"
[[ "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
  echo "phase-start approval must be a SHA-256 digest" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_start_receipt" >/dev/null || {
  echo "phase-start receipt is outside the fixed policy" >&2
  exit 1
}
phase_start_digest="$(jq -er '.receipt_digest' "$phase_start_receipt")"
[[ "$phase_start_digest" == \
    "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" &&
  "$(
    jq -cS 'del(.receipt_digest)' "$phase_start_receipt" |
      shasum -a 256 | awk '{print $1}'
  )" == "$phase_start_digest" &&
  "$(jq -er '.phase.id' "$phase_start_receipt")" == \
    "$MINCO_MULTI_RELEASE_PHASE_ID" &&
  "$(jq -er '.phase.source_revision' "$phase_start_receipt")" == \
    "$source_revision" ]] || {
  echo "phase-start receipt does not bind the exact clean source" >&2
  exit 1
}

evidence_relative="target/minco/aws/$MINCO_MULTI_RELEASE_PHASE_ID"
evidence_dir="$source_root/$evidence_relative"
[[ -d "$evidence_dir" && ! -L "$evidence_dir" &&
  "$(cd "$evidence_dir" && pwd -P)" == "$evidence_dir" &&
  "$(minco_file_mode "$evidence_dir")" == "700" ]] || {
  echo "phase provider evidence directory must be canonical and private" >&2
  exit 1
}

release_manifest="$(canonical_private_file \
  "release manifest" "$evidence_dir/release.json")"
migration_plan="$(canonical_private_file \
  "migration plan" "$evidence_dir/database-migration-plan.json")"
migration_receipt="$(canonical_private_file \
  "migration receipt" "$evidence_dir/database-migration-receipt.json")"
change_set_receipt="$(canonical_private_file \
  "change-set receipt" "$evidence_dir/change-set-receipt.json")"
deployment_receipt="$(canonical_private_file \
  "deployment receipt" "$evidence_dir/deployment-receipt.json")"
hosted_verification="$(canonical_private_file \
  "hosted verification" "$evidence_dir/hosted-verification.json")"
promotion_receipt="$(canonical_private_file \
  "promotion receipt" "$evidence_dir/promotion-receipt.json")"

release_manifest_digest="$(shasum -a 256 "$release_manifest" | awk '{print $1}')"
migration_plan_digest="$(shasum -a 256 "$migration_plan" | awk '{print $1}')"
migration_receipt_digest="$(shasum -a 256 "$migration_receipt" | awk '{print $1}')"
change_set_receipt_digest="$(shasum -a 256 "$change_set_receipt" | awk '{print $1}')"
deployment_receipt_digest="$(shasum -a 256 "$deployment_receipt" | awk '{print $1}')"
hosted_verification_digest="$(shasum -a 256 "$hosted_verification" | awk '{print $1}')"
promotion_receipt_digest="$(shasum -a 256 "$promotion_receipt" | awk '{print $1}')"

rollback_assessment_digest=
reused_release_manifest_digest=
exact_initial_release_reused=false
if [[ "$MINCO_MULTI_RELEASE_PHASE_ID" == "03-prior-rollback" ]]; then
  initial_release_manifest="$(canonical_private_file \
    "initial release manifest" \
    "$MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST")"
  rollback_assessment="$(canonical_private_file \
    "rollback assessment" "$MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT")"
  cmp -s "$initial_release_manifest" "$release_manifest" || {
    echo "rollback phase release differs from the exact initial release" >&2
    exit 1
  }
  jq -e '
    .operation == "rollback_compatibility_assessment"
    and .external_aws_contact == false
    and .rebuild == false
    and .replan == false
    and .reverse_sql == false
    and .automatic_data_repair == false
    and .reuse_historical_hosted_report == false
    and .assessment.classification == "compatible"
    and .routing_authorized == true
    and (.blockers | type == "array" and length == 0)
  ' "$rollback_assessment" >/dev/null || {
    echo "rollback assessment is not an exact compatible local decision" >&2
    exit 1
  }
  rollback_assessment_digest="$(
    shasum -a 256 "$rollback_assessment" | awk '{print $1}'
  )"
  reused_release_manifest_digest="$(
    shasum -a 256 "$initial_release_manifest" | awk '{print $1}'
  )"
  exact_initial_release_reused=true
fi

[[ "$MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT" == /* ]] || {
  echo "phase result output must be an absolute path" >&2
  exit 1
}
output_parent="$(dirname "$MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT")"
[[ -d "$output_parent" && ! -L "$output_parent" ]] || {
  echo "phase result output parent must be an existing non-symlink directory" >&2
  exit 1
}
canonical_output_parent="$(cd "$output_parent" && pwd -P)"
output_path="$canonical_output_parent/$(basename "$MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT")"
[[ "$output_path" == "$MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT" &&
  ! -e "$output_path" && ! -L "$output_path" ]] || {
  echo "phase result output must be canonical and create-only" >&2
  exit 1
}

result_tmp="$(mktemp "$canonical_output_parent/.phase-result.XXXXXX")"
cleanup_result() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$result_tmp" ]]; then
    rm -f -- "$result_tmp"
  fi
  exit "$status"
}
trap cleanup_result EXIT INT TERM
jq -n \
  --arg phase_id "$MINCO_MULTI_RELEASE_PHASE_ID" \
  --arg source_revision "$source_revision" \
  --arg release_manifest_digest "$release_manifest_digest" \
  --arg migration_plan_digest "$migration_plan_digest" \
  --arg migration_receipt_digest "$migration_receipt_digest" \
  --arg change_set_receipt_digest "$change_set_receipt_digest" \
  --arg deployment_receipt_digest "$deployment_receipt_digest" \
  --arg hosted_verification_digest "$hosted_verification_digest" \
  --arg promotion_receipt_digest "$promotion_receipt_digest" \
  --arg rollback_assessment_digest "$rollback_assessment_digest" \
  --arg reused_release_manifest_digest "$reused_release_manifest_digest" \
  --argjson exact_initial_release_reused "$exact_initial_release_reused" \
  '{
    schema_version: 1,
    operation: "multi_release_phase_result",
    state: "succeeded",
    external_aws_contact: true,
    phase: {
      id: $phase_id,
      source_revision: $source_revision,
      evidence_id: $phase_id
    },
    artifacts: {
      release_manifest_digest: $release_manifest_digest,
      migration_plan_digest: $migration_plan_digest,
      migration_receipt_digest: $migration_receipt_digest,
      change_set_receipt_digest: $change_set_receipt_digest,
      deployment_receipt_digest: $deployment_receipt_digest,
      hosted_verification_digest: $hosted_verification_digest,
      promotion_receipt_digest: $promotion_receipt_digest
    },
    rollback: {
      assessment_digest: (
        if $rollback_assessment_digest == "" then null
        else $rollback_assessment_digest end
      ),
      exact_initial_release_reused: $exact_initial_release_reused,
      reused_release_manifest_digest: (
        if $reused_release_manifest_digest == "" then null
        else $reused_release_manifest_digest end
      )
    },
    verification: {
      fresh: true,
      historical_report_reused: false
    },
    cleanup: {
      owner: "parent_controller",
      performed: false
    }
  }' >"$result_tmp"
chmod 600 "$result_tmp"
jq -e -f scripts/aws/lib/validate-multi-release-phase-result.jq \
  "$result_tmp" >/dev/null || exit 1
mv -n "$result_tmp" "$output_path"
[[ ! -e "$result_tmp" && -f "$output_path" && ! -L "$output_path" &&
  "$(minco_file_mode "$output_path")" == "600" ]] || {
  echo "could not atomically publish the phase result" >&2
  exit 1
}
result_tmp=
trap - EXIT INT TERM
cat "$output_path"
