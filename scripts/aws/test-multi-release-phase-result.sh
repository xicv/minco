#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

fixture_dir="$(mktemp -d)"
fixture_dir="$(cd "$fixture_dir" && pwd -P)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

source_root="$fixture_dir/source"
mkdir -p "$source_root"
git -C "$source_root" init -q
git -C "$source_root" config user.email minco-test@example.invalid
git -C "$source_root" config user.name "Minco test"
printf 'schema_version = 1\n' >"$source_root/minco.toml"
printf '[workspace]\nmembers = []\n' >"$source_root/Cargo.toml"
printf 'target/\n' >"$source_root/.gitignore"
git -C "$source_root" add .
git -C "$source_root" commit -qm "exact phase source"
source_revision="$(git -C "$source_root" rev-parse HEAD)"

write_phase_start() {
  local phase_id="$1"
  local release="$2"
  local stack_action="$3"
  local review_policy="$4"
  local output_path="$5"
  local payload="$fixture_dir/phase-start-payload.json"

  jq -n \
    --arg phase_id "$phase_id" \
    --arg release "$release" \
    --arg review_policy "$review_policy" \
    --arg source_revision "$source_revision" \
    --arg stack_action "$stack_action" \
    '{
      schema_version: 1,
      operation: "multi_release_phase_start",
      state: "started",
      external_aws_contact: false,
      controller: {
        plan_digest: ("1" * 64),
        receipt_digest: ("2" * 64)
      },
      authority: {
        approval_digest: ("3" * 64),
        kind: "minco.aws-multi-release-controller-rehearsal.v1",
        run_id: "phase-result-test"
      },
      phase: {
        id: $phase_id,
        release: $release,
        source_revision: $source_revision,
        evidence_namespace: ("phases/" + $phase_id),
        projection_digest: ("4" * 64),
        stack_action: $stack_action,
        change_set_review_policy: $review_policy
      },
      cleanup: {
        owner: "parent_controller",
        required: true,
        inner_phase_cleanup: false
      }
    }' >"$payload"
  payload_digest="$(jq -cS . "$payload" | shasum -a 256 | awk '{print $1}')"
  jq --arg receipt_digest "$payload_digest" \
    '. + {receipt_digest: $receipt_digest}' "$payload" >"$output_path"
  chmod 600 "$output_path"
}

write_phase_files() {
  local phase_id="$1"
  local release_content="$2"
  local evidence_dir="$source_root/target/minco/aws/$phase_id"

  mkdir -p "$evidence_dir"
  chmod 700 "$source_root/target" \
    "$source_root/target/minco" \
    "$source_root/target/minco/aws" \
    "$evidence_dir"
  printf '%s\n' "$release_content" >"$evidence_dir/release.json"
  printf '%s\n' "$phase_id-migration-plan" >"$evidence_dir/database-migration-plan.json"
  printf '%s\n' "$phase_id-migration-receipt" >"$evidence_dir/database-migration-receipt.json"
  printf '%s\n' "$phase_id-change-set" >"$evidence_dir/change-set-receipt.json"
  printf '%s\n' "$phase_id-deployment" >"$evidence_dir/deployment-receipt.json"
  printf '%s\n' "$phase_id-verification" >"$evidence_dir/hosted-verification.json"
  printf '%s\n' "$phase_id-promotion" >"$evidence_dir/promotion-receipt.json"
  chmod 600 "$evidence_dir"/*.json
}

result_writer="scripts/aws/write-multi-release-phase-result.sh"
[[ -x "$result_writer" ]] || {
  echo "multi-release phase result writer is missing" >&2
  exit 1
}

write_phase_files 01-prior-initial prior-release
phase_one_start="$fixture_dir/phase-one-start.json"
write_phase_start \
  01-prior-initial prior create bounded_create_v1 "$phase_one_start"
phase_one_start_digest="$(jq -er '.receipt_digest' "$phase_one_start")"
phase_one_result="$fixture_dir/phase-one-result.json"
if MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
  MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_one_start" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$phase_one_result" \
  "$result_writer" >/dev/null 2>&1; then
  echo "phase result writer accepted the wrong start approval" >&2
  exit 1
fi
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_one_start" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_one_start_digest" \
MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$phase_one_result" \
  "$result_writer" >/dev/null
jq -e -f scripts/aws/lib/validate-multi-release-phase-result.jq \
  "$phase_one_result" >/dev/null || {
  echo "phase result writer emitted an invalid result" >&2
  exit 1
}
if MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
  MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_one_start" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_one_start_digest" \
  MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$phase_one_result" \
  "$result_writer" >/dev/null 2>&1; then
  echo "phase result writer reused a create-only result path" >&2
  exit 1
fi
expected_release_digest="$(
  shasum -a 256 \
    "$source_root/target/minco/aws/01-prior-initial/release.json" |
    awk '{print $1}'
)"
jq -e \
  --arg release_digest "$expected_release_digest" \
  --arg source_revision "$source_revision" \
  '
    .phase == {
      evidence_id: "01-prior-initial",
      id: "01-prior-initial",
      source_revision: $source_revision
    }
    and .artifacts.release_manifest_digest == $release_digest
    and .rollback == {
      assessment_digest: null,
      exact_initial_release_reused: false,
      reused_release_manifest_digest: null
    }
  ' "$phase_one_result" >/dev/null || {
  echo "phase result did not bind the exact source and release file" >&2
  exit 1
}

printf '# source drift\n' >>"$source_root/minco.toml"
if MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
  MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_one_start" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_one_start_digest" \
  MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$fixture_dir/drift-result.json" \
  "$result_writer" >/dev/null 2>&1; then
  echo "phase result writer accepted source drift" >&2
  exit 1
fi
git -C "$source_root" restore minco.toml

verification_file="$source_root/target/minco/aws/01-prior-initial/hosted-verification.json"
mv "$verification_file" "$fixture_dir/verification.json"
ln -s "$fixture_dir/verification.json" "$verification_file"
if MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
  MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_one_start" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_one_start_digest" \
  MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$fixture_dir/symlink-result.json" \
  "$result_writer" >/dev/null 2>&1; then
  echo "phase result writer accepted a symlinked hosted report" >&2
  exit 1
fi
rm "$verification_file"
mv "$fixture_dir/verification.json" "$verification_file"

write_phase_files 03-prior-rollback prior-release
phase_three_start="$fixture_dir/phase-three-start.json"
write_phase_start \
  03-prior-rollback prior update bounded_release_update_v1 "$phase_three_start"
phase_three_start_digest="$(jq -er '.receipt_digest' "$phase_three_start")"
rollback_assessment="$fixture_dir/rollback-assessment.json"
jq -n '{
  operation: "rollback_compatibility_assessment",
  external_aws_contact: false,
  rebuild: false,
  replan: false,
  reverse_sql: false,
  automatic_data_repair: false,
  reuse_historical_hosted_report: false,
  assessment: {classification: "compatible"},
  routing_authorized: true,
  blockers: []
}' >"$rollback_assessment"
chmod 600 "$rollback_assessment"
phase_three_result="$fixture_dir/phase-three-result.json"
MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_three_start" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_three_start_digest" \
MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST="$source_root/target/minco/aws/01-prior-initial/release.json" \
MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT="$rollback_assessment" \
MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$phase_three_result" \
  "$result_writer" >/dev/null
jq -e \
  --arg release_digest "$expected_release_digest" \
  '
    .artifacts.release_manifest_digest == $release_digest
    and .rollback.exact_initial_release_reused == true
    and .rollback.reused_release_manifest_digest == $release_digest
    and (.rollback.assessment_digest | type == "string" and length == 64)
  ' "$phase_three_result" >/dev/null || {
  echo "rollback result did not bind exact release reuse and assessment" >&2
  exit 1
}

printf 'substituted-release\n' \
  >"$source_root/target/minco/aws/03-prior-rollback/release.json"
if MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
  MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_three_start" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_three_start_digest" \
  MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST="$source_root/target/minco/aws/01-prior-initial/release.json" \
  MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT="$rollback_assessment" \
  MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$fixture_dir/substituted-result.json" \
  "$result_writer" >/dev/null 2>&1; then
  echo "rollback result writer accepted a substituted release" >&2
  exit 1
fi

printf 'Multi-release phase result checks passed.\n'
