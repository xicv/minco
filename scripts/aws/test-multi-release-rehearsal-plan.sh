#!/usr/bin/env bash
set -euo pipefail

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

create_checkout() {
  local root="$1"
  local role="$2"

  mkdir -p "$root"
  git -C "$root" init -q
  git -C "$root" config user.email minco-test@example.invalid
  git -C "$root" config user.name "Minco test"
  mkdir -p "$root/nested"
  printf '[workspace]\nmembers = []\n# %s\n' "$role" >"$root/Cargo.toml"
  printf 'schema_version = 1\n' >"$root/minco.toml"
  printf 'schema_version = 1\n' >"$root/nested/minco.toml"
  if [[ "$role" == current ]]; then
    mkdir -p "$root/scripts/aws/lib"
    cp scripts/aws/initialize-multi-release-rehearsal.sh \
      scripts/aws/plan-multi-release-phase.sh \
      scripts/aws/validate-multi-release-rehearsal-authority.sh \
      "$root/scripts/aws/"
    if [[ -f scripts/aws/begin-multi-release-phase.sh ]]; then
      cp scripts/aws/begin-multi-release-phase.sh "$root/scripts/aws/"
    fi
    if [[ -f scripts/aws/run-multi-release-parent-session.sh ]]; then
      cp scripts/aws/run-multi-release-parent-session.sh "$root/scripts/aws/"
    fi
    cp scripts/aws/lib/common.sh \
      scripts/aws/lib/validate-multi-release-controller-receipt.jq \
      scripts/aws/lib/validate-multi-release-plan.jq \
      scripts/aws/lib/validate-rehearsal-authority-common.jq \
      "$root/scripts/aws/lib/"
    if [[ -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq ]]; then
      cp scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
        "$root/scripts/aws/lib/"
    fi
    if [[ -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq ]]; then
      cp scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
        "$root/scripts/aws/lib/"
    fi
    if [[ -f scripts/aws/lib/validate-multi-release-provider-entry-plan.jq ]]; then
      cp scripts/aws/lib/validate-multi-release-provider-entry-plan.jq \
        "$root/scripts/aws/lib/"
    fi
    chmod +x "$root/scripts/aws/"*.sh
  fi
  git -C "$root" add .
  git -C "$root" commit -qm "$role release"
}

prior_root="$fixture_dir/prior"
current_root="$fixture_dir/current"
evidence_root="$fixture_dir/evidence/target/minco/aws/reviewed-multi-release-run"
create_checkout "$prior_root" prior
create_checkout "$current_root" current
prior_revision="$(git -C "$prior_root" rev-parse HEAD)"
current_revision="$(git -C "$current_root" rev-parse HEAD)"

database_boundary='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}'
approved_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expires_at="$(jq -nr 'now + 3600 | todateiso8601')"
authority_file="$fixture_dir/multi-release-authority.json"
jq -n \
  --arg approved_at "$approved_at" \
  --arg expires_at "$expires_at" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  --argjson database_boundary "$database_boundary" \
  '{
    schema_version: 1,
    authority_kind: "minco.aws-multi-release-controller-rehearsal.v1",
    run_id: "reviewed-multi-release-run",
    source_revisions: {
      current: $current_revision,
      prior: $prior_revision
    },
    release_sequence: ["prior", "current", "prior"],
    expected_account_id: "123456789012",
    expected_region: "ap-southeast-2",
    expected_role_arn: "arn:aws:iam::123456789012:role/minco-rehearsal",
    aws_profile: "minco-rehearsal",
    environment: "dev",
    database_boundary: $database_boundary,
    resource_allowlist: "bounded-multi-release-smoke-v1",
    cleanup_blast_radius: "cleanup-bounded-multi-release-smoke-v1",
    max_duration_minutes: 60,
    max_spend_usd: 25,
    approved_by: "release-owner",
    approved_at: $approved_at,
    expires_at: $expires_at
  }' >"$authority_file"
approval_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"

fake_bin="$fixture_dir/fake-bin"
provider_contact_log="$fixture_dir/provider-contact.log"
mkdir -p "$fake_bin"
for command in aws cargo curl psql sam uv; do
  # The generated command must expand these variables only when invoked.
  # shellcheck disable=SC2016
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "%s\\n" "$(basename "$0")" >>"$MINCO_PROVIDER_CONTACT_LOG"' \
    'exit 99' >"$fake_bin/$command"
  chmod +x "$fake_bin/$command"
done
# The generated command must expand these variables only when invoked.
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "%s\\n" "$(basename "$0")" >>"$MINCO_PROVIDER_CONTACT_LOG"' \
  '[[ "$#" -eq 9 && "$1" == "--no-cli-pager" && "$2" == "--region" && "$3" == "ap-southeast-2" && "$4" == "sts" && "$5" == "get-caller-identity" && "$6" == "--query" && "$7" == "{Account:Account,Arn:Arn,UserId:UserId}" && "$8" == "--output" && "$9" == "json" ]] || exit 99' \
  'if [[ "${MINCO_FAKE_AWS_IDENTITY_MODE:-match}" == "mismatch" ]]; then' \
  '  printf "%s\n" '\''{"Account":"123456789012","Arn":"arn:aws:sts::123456789012:assumed-role/unapproved-role/test-session","UserId":"AROATEST:test-session"}'\''' \
  'else' \
  '  printf "%s\n" '\''{"Account":"123456789012","Arn":"arn:aws:sts::123456789012:assumed-role/minco-rehearsal/test-session","UserId":"AROATEST:test-session"}'\''' \
  'fi' \
  >"$fake_bin/aws"
chmod +x "$fake_bin/aws"

plan_rehearsal() {
  local selected_prior_root="$1"
  local selected_current_root="$2"
  local selected_evidence_root="${3:-$evidence_root}"

  PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_PRIOR_ROOT="$selected_prior_root" \
  MINCO_CURRENT_ROOT="$selected_current_root" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$selected_evidence_root" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_AWS_RUN_ID=reviewed-multi-release-run \
  MINCO_REHEARSAL_PROFILE=minco-rehearsal \
  AWS_REGION=ap-southeast-2 \
  MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON="$database_boundary" \
  MINCO_REHEARSAL_RESOURCE_ALLOWLIST=bounded-multi-release-smoke-v1 \
  MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS=cleanup-bounded-multi-release-smoke-v1 \
    scripts/aws/plan-multi-release-rehearsal.sh
}

plan="$fixture_dir/plan.json"
plan_rehearsal "$prior_root" "$current_root" >"$plan"

[[ ! -e "$provider_contact_log" ]] || {
  echo "multi-release planning contacted a provider or build command" >&2
  exit 1
}

jq -e \
  --arg prior_root "$(cd "$prior_root" && pwd -P)" \
  --arg current_root "$(cd "$current_root" && pwd -P)" \
  --arg evidence_root "$(dirname "$evidence_root")/$(basename "$evidence_root")" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  '
    keys == [
      "authority",
      "cleanup",
      "evidence_root",
      "external_aws_contact",
      "operation",
      "phases",
      "provider_boundary",
      "rollback",
      "schema_version"
    ]
    and .schema_version == 1
    and .operation == "multi_release_controller_rehearsal"
    and .external_aws_contact == false
    and .authority == {
      approval_digest: .authority.approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and (.authority.approval_digest | test("^[0-9a-f]{64}$"))
    and .evidence_root == $evidence_root
    and .provider_boundary == {
      artifact_bucket_lifetime: "whole_run",
      shared_stack: true,
      stack_lifecycle: ["create", "update", "update", "delete"]
    }
    and .rollback == {
      accepted_result: "compatible",
      compatibility_assessment_required: true,
      current_promotion_phase: "02-current",
      historical_hosted_report_reuse: false,
      target_promotion_phase: "01-prior-initial"
    }
    and .cleanup == {
      after_phase: "03-prior-rollback",
      inner_phase_cleanup: false,
      owner: "parent_controller",
      trap_count: 1
    }
    and [.phases[].release] == ["prior", "current", "prior"]
    and [.phases[].stack_action] == ["create", "update", "update"]
    and [.phases[].change_set_review_policy] == [
      "bounded_create_v1",
      "bounded_release_update_v1",
      "bounded_release_update_v1"
    ]
    and [.phases[].evidence_namespace] == [
      "phases/01-prior-initial",
      "phases/02-current",
      "phases/03-prior-rollback"
    ]
    and ([.phases[].evidence_write_policy] | all(. == "create_only"))
    and .phases[0].source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phases[1].source == {
      revision: $current_revision,
      root: $current_root
    }
    and .phases[2].source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phases[0].artifact_policy == {
      build: true,
      replan: true,
      reuse_exact_release_from_phase: null
    }
    and .phases[1].artifact_policy == {
      build: true,
      replan: true,
      reuse_exact_release_from_phase: null
    }
    and .phases[2].artifact_policy == {
      build: false,
      replan: false,
      reuse_exact_release_from_phase: "01-prior-initial"
    }
    and ([.phases[].fresh_hosted_verification] | all)
    and ([.phases[].promotion_required] | all)
  ' "$plan" >/dev/null || {
  echo "multi-release plan omitted or weakened the exact phase lifecycle" >&2
  jq . "$plan" >&2
  exit 1
}

create_review="$fixture_dir/create-change-set-receipt.json"
update_review="$fixture_dir/update-change-set-receipt.json"
broadened_update_review="$fixture_dir/broadened-update-change-set-receipt.json"
jq -n '
  {
    change_set: {
      change_set_type: "create",
      review: {
        additions: [{
          logical_id: "HttpApi",
          resource_type: "AWS::ApiGatewayV2::Api",
          action: "add",
          replacement: null,
          policy_action: null,
          scope: []
        }],
        modifications: [],
        replacements: [],
        deletions: [],
        imports: [],
        indeterminate: [],
        metadata_syncs: []
      }
    }
  }
' >"$create_review"
bounded_phase_change_set_is_authorized "$create_review" bounded_create_v1 || {
  echo "exact bounded create review was rejected" >&2
  exit 1
}

jq -n '
  {
    change_set: {
      change_set_type: "update",
      review: {
        additions: [{
          logical_id: "ApiFunctionVersionA1B2C3",
          resource_type: "AWS::Lambda::Version",
          action: "add",
          replacement: null,
          policy_action: null,
          scope: []
        }],
        modifications: [
          {
            logical_id: "ApiFunction",
            resource_type: "AWS::Lambda::Function",
            action: "modify",
            replacement: "never",
            policy_action: null,
            scope: ["properties"]
          },
          {
            logical_id: "ApiFunctionAliascandidate",
            resource_type: "AWS::Lambda::Alias",
            action: "modify",
            replacement: null,
            policy_action: null,
            scope: ["properties"]
          }
        ],
        replacements: [],
        deletions: [{
          logical_id: "ApiFunctionVersionOld",
          resource_type: "AWS::Lambda::Version",
          action: "remove",
          replacement: null,
          policy_action: "delete",
          scope: []
        }],
        imports: [],
        indeterminate: [],
        metadata_syncs: []
      }
    }
  }
' >"$update_review"
bounded_phase_change_set_is_authorized \
  "$update_review" bounded_release_update_v1 || {
  echo "exact bounded release update review was rejected" >&2
  exit 1
}

jq '
  .change_set.review.modifications += [
    {
      logical_id: "ExecutionRole",
      resource_type: "AWS::IAM::Role",
      action: "modify",
      replacement: null,
      policy_action: null,
      scope: ["properties"]
    }
  ]
' "$update_review" >"$broadened_update_review"
if bounded_phase_change_set_is_authorized \
  "$broadened_update_review" bounded_release_update_v1; then
  echo "release update review accepted expanded IAM resources" >&2
  exit 1
fi

live_alias_update_review="$fixture_dir/live-alias-update-change-set-receipt.json"
jq '
  .change_set.review.modifications += [
    {
      logical_id: "LiveFunctionAlias",
      resource_type: "AWS::Lambda::Alias",
      action: "modify",
      replacement: null,
      policy_action: null,
      scope: ["properties"]
    }
  ]
' "$update_review" >"$live_alias_update_review"
if bounded_phase_change_set_is_authorized \
  "$live_alias_update_review" bounded_release_update_v1; then
  echo "release update review accepted live routing mutation" >&2
  exit 1
fi

if bounded_phase_change_set_is_authorized \
  "$update_review" operator_defined_update 2>/dev/null; then
  echo "release update review accepted an operator-defined policy" >&2
  exit 1
fi

incomplete_update_review="$fixture_dir/incomplete-update-change-set-receipt.json"
jq 'del(.change_set.review.metadata_syncs)' \
  "$update_review" >"$incomplete_update_review"
if bounded_phase_change_set_is_authorized \
  "$incomplete_update_review" bounded_release_update_v1; then
  echo "release update review accepted an incomplete provider classification" >&2
  exit 1
fi

incomplete_resource_review="$fixture_dir/incomplete-resource-change-set-receipt.json"
jq 'del(.change_set.review.modifications[0].action)' \
  "$update_review" >"$incomplete_resource_review"
if bounded_phase_change_set_is_authorized \
  "$incomplete_resource_review" bounded_release_update_v1; then
  echo "release update review accepted an incomplete resource change" >&2
  exit 1
fi

retained_update_review="$fixture_dir/retained-update-change-set-receipt.json"
jq '.change_set.review.deletions[0].policy_action = "retain"' \
  "$update_review" >"$retained_update_review"
if bounded_phase_change_set_is_authorized \
  "$retained_update_review" bounded_release_update_v1; then
  echo "release update review accepted retained generated resources" >&2
  exit 1
fi

metadata_update_review="$fixture_dir/metadata-update-change-set-receipt.json"
jq '.change_set.review.modifications[0].scope = ["metadata"]' \
  "$update_review" >"$metadata_update_review"
if bounded_phase_change_set_is_authorized \
  "$metadata_update_review" bounded_release_update_v1; then
  echo "release update review accepted a non-property mutation" >&2
  exit 1
fi

plan_digest="$(shasum -a 256 "$plan" | awk '{print $1}')"
phase_projection="$fixture_dir/phase-projection.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >"$phase_projection"

jq -e \
  --arg controller_root "$(cd "$current_root" && pwd -P)" \
  --arg prior_root "$(cd "$prior_root" && pwd -P)" \
  --arg prior_revision "$prior_revision" \
  --arg evidence_root "$evidence_root" \
  --arg plan_digest "$plan_digest" \
  '
    keys == [
      "authority",
      "controller",
      "evidence",
      "external_aws_contact",
      "operation",
      "phase",
      "plan_digest",
      "rollback",
      "schema_version"
    ]
    and .schema_version == 1
    and .operation == "multi_release_phase"
    and .external_aws_contact == false
    and .plan_digest == $plan_digest
    and .authority == {
      approval_digest: .authority.approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .controller == {
      cleanup_owner: "parent_controller",
      root: $controller_root
    }
    and .evidence == {
      namespace: "phases/03-prior-rollback",
      path: ($evidence_root + "/phases/03-prior-rollback"),
      write_policy: "create_only"
    }
    and .phase.id == "03-prior-rollback"
    and .phase.release == "prior"
    and .phase.source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phase.stack_action == "update"
    and .phase.change_set_review_policy == "bounded_release_update_v1"
    and .phase.artifact_policy == {
      build: false,
      replan: false,
      reuse_exact_release_from_phase: "01-prior-initial"
    }
    and .phase.fresh_hosted_verification == true
    and .phase.promotion_required == true
    and .rollback == {
      accepted_result: "compatible",
      compatibility_assessment_required: true,
      current_promotion_phase: "02-current",
      historical_hosted_report_reuse: false,
      target_promotion_phase: "01-prior-initial"
    }
  ' "$phase_projection" >/dev/null || {
  echo "phase projection omitted or weakened the sealed rollback handoff" >&2
  jq . "$phase_projection" >&2
  exit 1
}

controller_output="$fixture_dir/controller-initialization-output.json"
controller_initializer="$current_root/scripts/aws/initialize-multi-release-rehearsal.sh"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/initialize-multi-release-rehearsal.sh >/dev/null 2>&1; then
  echo "multi-release initialization accepted controller code outside the exact current checkout" >&2
  exit 1
fi
[[ ! -e "$evidence_root" ]] || {
  echo "rejected external controller initialization created evidence" >&2
  exit 1
}
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  "$controller_initializer" >"$controller_output"

controller_receipt="$evidence_root/control/controller-receipt.json"
authority_receipt="$evidence_root/control/authority-receipt.json"
sealed_plan="$evidence_root/control/multi-release-plan.json"
[[ -f "$controller_receipt" && ! -L "$controller_receipt" &&
  -f "$authority_receipt" && ! -L "$authority_receipt" &&
  -f "$sealed_plan" && ! -L "$sealed_plan" ]] || {
  echo "multi-release initialization omitted sealed control evidence" >&2
  exit 1
}
file_mode() {
  if stat -f '%Lp' "$1" 2>/dev/null; then
    return
  fi
  stat -c '%a' "$1"
}
[[ "$(file_mode "$evidence_root")" == "700" ]] || {
  echo "multi-release initialization did not keep its evidence root private" >&2
  exit 1
}
while IFS= read -r control_file; do
  [[ "$(file_mode "$control_file")" == "600" ]] || {
    echo "multi-release initialization did not keep control evidence private" >&2
    exit 1
  }
done < <(find "$evidence_root/control" -type f -print)
[[ "$(shasum -a 256 "$sealed_plan" | awk '{print $1}')" == "$plan_digest" ]] || {
  echo "multi-release initialization did not preserve the exact whole-run plan" >&2
  exit 1
}

for phase_id in 01-prior-initial 02-current 03-prior-rollback; do
  projection="$evidence_root/control/phases/$phase_id.json"
  [[ -f "$projection" && ! -L "$projection" ]] || {
    echo "multi-release initialization omitted an exact phase projection" >&2
    exit 1
  }
  [[ "$(jq -er '.phase.id' "$projection")" == "$phase_id" ]] || {
    echo "multi-release initialization crossed phase projection evidence" >&2
    exit 1
  }
done

receipt_digest="$(jq -cS 'del(.receipt_digest)' "$controller_receipt" | shasum -a 256 | awk '{print $1}')"
jq -e \
  --arg plan_digest "$plan_digest" \
  --arg approval_digest "$approval_digest" \
  --arg receipt_digest "$receipt_digest" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  '
    keys == [
      "authority",
      "cleanup",
      "execution",
      "external_aws_contact",
      "operation",
      "plan_digest",
      "provider_boundary",
      "receipt_digest",
      "schema_version",
      "source_revisions",
      "state"
    ]
    and .schema_version == 1
    and .operation == "multi_release_controller_rehearsal"
    and .state == "initialized"
    and .external_aws_contact == false
    and .plan_digest == $plan_digest
    and .receipt_digest == $receipt_digest
    and .authority == {
      approval_digest: $approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .source_revisions == {
      current: $current_revision,
      prior: $prior_revision
    }
    and .execution.phase_sequence == [
      "01-prior-initial",
      "02-current",
      "03-prior-rollback"
    ]
    and .execution.next_phase == "01-prior-initial"
    and [.execution.phases[].id] == .execution.phase_sequence
    and ([.execution.phases[].state] | all(. == "pending"))
    and ([.execution.phases[].projection_digest]
      | all(test("^[0-9a-f]{64}$")))
    and .provider_boundary == {
      artifact_bucket_state: "not_created",
      shared_stack_state: "not_created"
    }
    and .cleanup == {
      owner: "parent_controller",
      required: true,
      state: "pending",
      trap_count: 1
    }
  ' "$controller_receipt" >/dev/null || {
  echo "multi-release initialization receipt weakened the parent execution boundary" >&2
  jq . "$controller_receipt" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null || {
  echo "multi-release initialization produced a receipt outside fixed policy" >&2
  exit 1
}
broadened_controller_receipt="$fixture_dir/broadened-controller-receipt.json"
jq '.cleanup.owner = "inner_phase"' \
  "$controller_receipt" >"$broadened_controller_receipt"
if jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$broadened_controller_receipt" >/dev/null; then
  echo "multi-release controller receipt accepted inner-phase cleanup ownership" >&2
  exit 1
fi
cmp -s "$controller_output" "$controller_receipt" || {
  echo "multi-release initialization output did not match its sealed receipt" >&2
  exit 1
}
jq -e \
  'keys == [
    "approval_digest",
    "approved_at",
    "authority_kind",
    "cleanup_blast_radius",
    "database_boundary_mode",
    "environment",
    "expires_at",
    "max_duration_minutes",
    "max_spend_usd",
    "release_sequence",
    "resource_allowlist",
    "run_id",
    "schema_version",
    "source_revisions"
  ]
  and (has("expected_account_id") | not)
  and (has("expected_role_arn") | not)
  and (has("aws_profile") | not)
  and (tostring | contains("/minco/rehearsal/database-url") | not)' \
  "$authority_receipt" >/dev/null || {
  echo "multi-release initialization retained sensitive authority identity" >&2
  exit 1
}
[[ ! -e "$evidence_root/phases/01-prior-initial" &&
  ! -e "$evidence_root/phases/02-current" &&
  ! -e "$evidence_root/phases/03-prior-rollback" ]] || {
  echo "multi-release initialization consumed create-only phase evidence" >&2
  exit 1
}
[[ ! -e "$provider_contact_log" ]] || {
  echo "multi-release initialization contacted a provider or build command" >&2
  exit 1
}

controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
phase_beginner="$current_root/scripts/aws/begin-multi-release-phase.sh"
phase_start_output="$fixture_dir/phase-start-output.json"
controller_file_digest_before_phase="$(
  shasum -a 256 "$controller_receipt" | awk '{print $1}'
)"
sealed_future_projection="$evidence_root/control/phases/02-current.json"
future_projection_target="$fixture_dir/02-current-symlink-target.json"
cp "$sealed_future_projection" "$future_projection_target"
chmod 600 "$future_projection_target"
rm -f -- "$sealed_future_projection"
ln -s "$future_projection_target" "$sealed_future_projection"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a symlinked future projection" >&2
  exit 1
fi
rm -f -- "$sealed_future_projection"
cp "$future_projection_target" "$sealed_future_projection"
chmod 600 "$sealed_future_projection"
[[ ! -e "$evidence_root/phases" && ! -L "$evidence_root/phases" ]] || {
  echo "rejected symlinked control evidence consumed a phase namespace" >&2
  exit 1
}
[[ ! -e "$provider_contact_log" ]] || {
  echo "rejected symlinked control evidence contacted a provider or build command" >&2
  exit 1
}
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a phase outside initialized order" >&2
  exit 1
fi
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted the wrong controller approval" >&2
  exit 1
fi
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  scripts/aws/begin-multi-release-phase.sh >/dev/null 2>&1; then
  echo "multi-release phase start accepted code outside the exact controller checkout" >&2
  exit 1
fi
mismatched_authority="$fixture_dir/mismatched-multi-release-authority.json"
jq '.approved_by = "different-release-owner"' \
  "$authority_file" >"$mismatched_authority"
mismatched_authority_digest="$(
  shasum -a 256 "$mismatched_authority" | awk '{print $1}'
)"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$mismatched_authority" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$mismatched_authority_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted authority outside the initialized controller" >&2
  exit 1
fi
chmod 644 "$controller_receipt"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted broadly accessible control evidence" >&2
  exit 1
fi
chmod 600 "$controller_receipt"
sealed_first_projection="$evidence_root/control/phases/01-prior-initial.json"
projection_backup="$fixture_dir/01-prior-initial-projection.json"
cp "$sealed_first_projection" "$projection_backup"
jq '.phase.stack_action = "update"' \
  "$projection_backup" >"$sealed_first_projection"
chmod 600 "$sealed_first_projection"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a tampered phase projection" >&2
  exit 1
fi
cp "$projection_backup" "$sealed_first_projection"
chmod 600 "$sealed_first_projection"
sealed_future_projection="$evidence_root/control/phases/02-current.json"
future_projection_backup="$fixture_dir/02-current-projection.json"
cp "$sealed_future_projection" "$future_projection_backup"
jq '.phase.stack_action = "create"' \
  "$future_projection_backup" >"$sealed_future_projection"
chmod 600 "$sealed_future_projection"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a tampered future projection" >&2
  exit 1
fi
cp "$future_projection_backup" "$sealed_future_projection"
chmod 600 "$sealed_future_projection"
authority_receipt_backup="$fixture_dir/authority-receipt.json"
cp "$authority_receipt" "$authority_receipt_backup"
jq '.max_spend_usd = 1' \
  "$authority_receipt_backup" >"$authority_receipt"
chmod 600 "$authority_receipt"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a tampered authority receipt" >&2
  exit 1
fi
cp "$authority_receipt_backup" "$authority_receipt"
chmod 600 "$authority_receipt"
chmod 755 "$evidence_root"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a broadly accessible evidence root" >&2
  exit 1
fi
chmod 700 "$evidence_root"
mkdir -m 700 "$evidence_root/phases"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted a pre-existing phases boundary" >&2
  exit 1
fi
rmdir "$evidence_root/phases"
failing_move_bin="$fixture_dir/failing-move-bin"
mkdir -p "$failing_move_bin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'exit 91' >"$failing_move_bin/mv"
chmod +x "$failing_move_bin/mv"
if PATH="$failing_move_bin:$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start ignored a failed atomic publish" >&2
  exit 1
fi
[[ ! -e "$evidence_root/phases" && ! -L "$evidence_root/phases" ]] || {
  echo "failed multi-release phase start left a partial phases boundary" >&2
  exit 1
}
printf '{}\n' >"$evidence_root/forged-controller-state.json"
chmod 600 "$evidence_root/forged-controller-state.json"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start accepted unsealed controller state" >&2
  exit 1
fi
rm -f -- "$evidence_root/forged-controller-state.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >"$phase_start_output"

phase_path="$evidence_root/phases/01-prior-initial"
phase_start_receipt="$phase_path/phase-start-receipt.json"
phase_projection_copy="$phase_path/phase-projection.json"
[[ -d "$evidence_root/phases" && ! -L "$evidence_root/phases" &&
  -d "$phase_path" && ! -L "$phase_path" &&
  -f "$phase_start_receipt" && ! -L "$phase_start_receipt" &&
  -f "$phase_projection_copy" && ! -L "$phase_projection_copy" ]] || {
  echo "multi-release phase start omitted its private create-only evidence" >&2
  exit 1
}
[[ "$(file_mode "$evidence_root/phases")" == "700" &&
  "$(file_mode "$phase_path")" == "700" &&
  "$(file_mode "$phase_start_receipt")" == "600" &&
  "$(file_mode "$phase_projection_copy")" == "600" ]] || {
  echo "multi-release phase start broadened phase evidence permissions" >&2
  exit 1
}
cmp -s "$phase_start_output" "$phase_start_receipt" || {
  echo "multi-release phase start output did not match its sealed receipt" >&2
  exit 1
}
cmp -s "$sealed_first_projection" "$phase_projection_copy" || {
  echo "multi-release phase start did not preserve the exact projection" >&2
  exit 1
}
phase_start_receipt_digest="$(
  jq -cS 'del(.receipt_digest)' "$phase_start_receipt" |
    shasum -a 256 | awk '{print $1}'
)"
jq -e \
  --arg approval_digest "$approval_digest" \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg phase_start_receipt_digest "$phase_start_receipt_digest" \
  --arg plan_digest "$plan_digest" \
  --arg prior_revision "$prior_revision" \
  --arg projection_digest "$(
    shasum -a 256 "$sealed_first_projection" | awk '{print $1}'
  )" \
  '
    keys == [
      "authority",
      "cleanup",
      "controller",
      "external_aws_contact",
      "operation",
      "phase",
      "receipt_digest",
      "schema_version",
      "state"
    ]
    and .schema_version == 1
    and .operation == "multi_release_phase_start"
    and .state == "started"
    and .external_aws_contact == false
    and .receipt_digest == $phase_start_receipt_digest
    and .controller == {
      plan_digest: $plan_digest,
      receipt_digest: $controller_receipt_digest
    }
    and .authority == {
      approval_digest: $approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .phase == {
      change_set_review_policy: "bounded_create_v1",
      evidence_namespace: "phases/01-prior-initial",
      id: "01-prior-initial",
      projection_digest: $projection_digest,
      release: "prior",
      source_revision: $prior_revision,
      stack_action: "create"
    }
    and .cleanup == {
      inner_phase_cleanup: false,
      owner: "parent_controller",
      required: true
    }
    and (tostring | contains("123456789012") | not)
    and (tostring | contains("/minco/rehearsal/database-url") | not)
  ' "$phase_start_receipt" >/dev/null || {
  echo "multi-release phase-start receipt weakened or exposed its boundary" >&2
  jq . "$phase_start_receipt" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_start_receipt" >/dev/null || {
  echo "multi-release phase-start receipt is outside fixed policy" >&2
  exit 1
}
[[ "$(shasum -a 256 "$controller_receipt" | awk '{print $1}')" == \
  "$controller_file_digest_before_phase" ]] || {
  echo "multi-release phase start changed the immutable controller receipt" >&2
  exit 1
}
[[ ! -e "$evidence_root/phases/02-current" &&
  ! -e "$evidence_root/phases/03-prior-rollback" ]] || {
  echo "multi-release phase start consumed a later phase namespace" >&2
  exit 1
}
[[ ! -e "$provider_contact_log" ]] || {
  echo "multi-release phase start contacted a provider or build command" >&2
  exit 1
}
sealed_phase_start_digest="$(shasum -a 256 "$phase_start_receipt" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null 2>&1; then
  echo "multi-release phase start reused an existing phase namespace" >&2
  exit 1
fi
[[ "$(shasum -a 256 "$phase_start_receipt" | awk '{print $1}')" == \
  "$sealed_phase_start_digest" ]] || {
  echo "rejected repeated phase start changed sealed evidence" >&2
  exit 1
}

parent_session_runner="$current_root/scripts/aws/run-multi-release-parent-session.sh"
parent_session_output="$fixture_dir/parent-session-output.json"
phase_start_approval="$(jq -er '.receipt_digest' "$phase_start_receipt")"
provider_entry_plan="$fixture_dir/provider-entry-plan.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=plan \
  "$parent_session_runner" >"$provider_entry_plan"
jq -e \
  --arg approval_digest "$approval_digest" \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg phase_start_approval "$phase_start_approval" \
  --arg plan_digest "$plan_digest" \
  --arg prior_revision "$prior_revision" \
  --arg projection_digest "$(
    shasum -a 256 "$phase_projection_copy" | awk '{print $1}'
  )" \
  '
    keys == [
      "authority",
      "cleanup",
      "controller",
      "external_aws_contact",
      "operation",
      "phase",
      "provider",
      "schema_version"
    ]
    and .schema_version == 1
    and .operation == "multi_release_provider_entry"
    and .external_aws_contact == false
    and .controller == {
      plan_digest: $plan_digest,
      receipt_digest: $controller_receipt_digest
    }
    and .authority == {
      approval_digest: $approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .phase == {
      id: "01-prior-initial",
      projection_digest: $projection_digest,
      source_revision: $prior_revision,
      start_receipt_digest: $phase_start_approval
    }
    and .provider == {
      action: "sts_get_caller_identity",
      expected_region: "ap-southeast-2",
      mutation: false,
      secrets_requested: false
    }
    and .cleanup == {
      owner: "parent_controller",
      required: true,
      trap_count: 1
    }
    and (tostring | contains("123456789012") | not)
    and (tostring | contains("arn:aws:iam") | not)
    and (tostring | contains("/minco/rehearsal/database-url") | not)
  ' "$provider_entry_plan" >/dev/null || {
  echo "multi-release provider-entry plan weakened or exposed its boundary" >&2
  exit 1
}
[[ ! -e "$phase_path/parent-session-start-receipt.json" &&
  ! -e "$phase_path/parent-session-completion-receipt.json" &&
  ! -e "$provider_contact_log" ]] || {
  echo "multi-release provider-entry planning consumed evidence or contacted a provider" >&2
  exit 1
}
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$parent_session_runner" >/dev/null 2>&1; then
  echo "multi-release parent session accepted the wrong phase-start approval" >&2
  exit 1
fi
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  scripts/aws/run-multi-release-parent-session.sh >/dev/null 2>&1; then
  echo "multi-release parent session accepted code outside the exact controller checkout" >&2
  exit 1
fi
chmod 755 "$phase_path"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$parent_session_runner" >/dev/null 2>&1; then
  echo "multi-release parent session accepted broadly accessible phase evidence" >&2
  exit 1
fi
chmod 700 "$phase_path"
[[ ! -e "$phase_path/parent-session-start-receipt.json" &&
  ! -e "$phase_path/parent-session-completion-receipt.json" &&
  ! -e "$provider_contact_log" ]] || {
  echo "rejected parent session consumed evidence or contacted a provider" >&2
  exit 1
}
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$parent_session_runner" >"$parent_session_output"

parent_session_start="$phase_path/parent-session-start-receipt.json"
parent_session_completion="$phase_path/parent-session-completion-receipt.json"
[[ -f "$parent_session_start" && ! -L "$parent_session_start" &&
  -f "$parent_session_completion" && ! -L "$parent_session_completion" ]] || {
  echo "multi-release parent session omitted its immutable lifecycle receipts" >&2
  exit 1
}
[[ "$(file_mode "$parent_session_start")" == "600" &&
  "$(file_mode "$parent_session_completion")" == "600" ]] || {
  echo "multi-release parent session broadened lifecycle receipt permissions" >&2
  exit 1
}
cmp -s "$parent_session_output" "$parent_session_completion" || {
  echo "multi-release parent session output did not match its completion receipt" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
  "$parent_session_start" >/dev/null || {
  echo "multi-release parent-session start receipt is outside fixed policy" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
  "$parent_session_completion" >/dev/null || {
  echo "multi-release parent-session completion receipt is outside fixed policy" >&2
  exit 1
}
parent_session_start_digest="$(
  jq -cS 'del(.receipt_digest)' "$parent_session_start" |
    shasum -a 256 | awk '{print $1}'
)"
parent_session_completion_digest="$(
  jq -cS 'del(.receipt_digest)' "$parent_session_completion" |
    shasum -a 256 | awk '{print $1}'
)"
jq -e \
  --arg approval_digest "$approval_digest" \
  --arg controller_receipt_digest "$controller_receipt_digest" \
  --arg parent_session_start_digest "$parent_session_start_digest" \
  --arg phase_start_approval "$phase_start_approval" \
  --arg plan_digest "$plan_digest" \
  --arg prior_revision "$prior_revision" \
  --arg projection_digest "$(
    shasum -a 256 "$phase_projection_copy" | awk '{print $1}'
  )" \
  '
    .state == "started"
    and .external_aws_contact == false
    and .receipt_digest == $parent_session_start_digest
    and .controller == {
      plan_digest: $plan_digest,
      receipt_digest: $controller_receipt_digest
    }
    and .authority == {
      approval_digest: $approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .phase == {
      change_set_review_policy: "bounded_create_v1",
      evidence_namespace: "phases/01-prior-initial",
      id: "01-prior-initial",
      projection_digest: $projection_digest,
      release: "prior",
      source_revision: $prior_revision,
      stack_action: "create",
      start_receipt_digest: $phase_start_approval
    }
    and .execution == {
      mode: "validation_only",
      provider_entry_plan_digest: null,
      provider_state: "not_entered"
    }
    and .session == {start_receipt_digest: null}
    and .cleanup == {
      action: "none_before_provider_boundary",
      owner: "parent_controller",
      required: true,
      state: "installed",
      trap_count: 1
    }
    and (tostring | contains("123456789012") | not)
    and (tostring | contains("/minco/rehearsal/database-url") | not)
  ' "$parent_session_start" >/dev/null || {
  echo "multi-release parent-session start receipt weakened its boundary" >&2
  jq . "$parent_session_start" >&2
  exit 1
}
jq -e \
  --arg parent_session_completion_digest "$parent_session_completion_digest" \
  --slurpfile start "$parent_session_start" \
  '
    .state == "validated"
    and .external_aws_contact == false
    and .receipt_digest == $parent_session_completion_digest
    and .authority == $start[0].authority
    and .controller == $start[0].controller
    and .phase == $start[0].phase
    and .execution == $start[0].execution
    and .session == {
      start_receipt_digest: $start[0].receipt_digest
    }
    and .cleanup == {
      action: "none_provider_boundary_not_entered",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
  ' "$parent_session_completion" >/dev/null || {
  echo "multi-release parent-session completion receipt weakened its boundary" >&2
  jq . "$parent_session_completion" >&2
  exit 1
}
shopt -s dotglob nullglob
phase_entries=("$phase_path"/*)
shopt -u dotglob nullglob
[[ "${#phase_entries[@]}" -eq 4 &&
  -f "$phase_path/parent-session-completion-receipt.json" &&
  -f "$phase_path/parent-session-start-receipt.json" &&
  -f "$phase_path/phase-projection.json" &&
  -f "$phase_path/phase-start-receipt.json" ]] || {
  echo "multi-release parent session left unsealed phase state" >&2
  exit 1
}
[[ "$(shasum -a 256 "$controller_receipt" | awk '{print $1}')" == \
  "$controller_file_digest_before_phase" &&
  "$(shasum -a 256 "$phase_start_receipt" | awk '{print $1}')" == \
  "$sealed_phase_start_digest" ]] || {
  echo "multi-release parent session changed immutable controller or phase-start evidence" >&2
  exit 1
}
[[ ! -e "$provider_contact_log" ]] || {
  echo "multi-release parent validation session contacted a provider or build command" >&2
  exit 1
}
sealed_parent_start_digest="$(shasum -a 256 "$parent_session_start" | awk '{print $1}')"
sealed_parent_completion_digest="$(
  shasum -a 256 "$parent_session_completion" | awk '{print $1}'
)"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$parent_session_runner" >/dev/null 2>&1; then
  echo "multi-release parent session reused create-only lifecycle evidence" >&2
  exit 1
fi
[[ "$(shasum -a 256 "$parent_session_start" | awk '{print $1}')" == \
  "$sealed_parent_start_digest" &&
  "$(shasum -a 256 "$parent_session_completion" | awk '{print $1}')" == \
  "$sealed_parent_completion_digest" &&
  ! -e "$provider_contact_log" ]] || {
  echo "rejected repeated parent session changed evidence or contacted a provider" >&2
  exit 1
}

sealed_receipt_digest="$(shasum -a 256 "$controller_receipt" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  "$controller_initializer" >/dev/null 2>&1; then
  echo "multi-release initialization reused a pre-existing evidence boundary" >&2
  exit 1
fi
[[ "$(shasum -a 256 "$controller_receipt" | awk '{print $1}')" == "$sealed_receipt_digest" ]] || {
  echo "rejected multi-release initialization changed sealed evidence" >&2
  exit 1
}
rm -r -- "$evidence_root"

provider_session_output="$fixture_dir/provider-session-output.json"
provider_entry_execution_plan="$fixture_dir/provider-entry-execution-plan.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  "$controller_initializer" >/dev/null
controller_receipt="$evidence_root/control/controller-receipt.json"
controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null
phase_path="$evidence_root/phases/01-prior-initial"
phase_start_receipt="$phase_path/phase-start-receipt.json"
phase_start_approval="$(jq -er '.receipt_digest' "$phase_start_receipt")"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=plan \
  "$parent_session_runner" >"$provider_entry_execution_plan"
provider_entry_approval="$(
  shasum -a 256 "$provider_entry_execution_plan" | awk '{print $1}'
)"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
  MINCO_MULTI_RELEASE_PROVIDER_ACTION=execute \
  MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  "$parent_session_runner" >/dev/null 2>&1; then
  echo "provider identity preflight accepted the wrong provider-entry approval" >&2
  exit 1
fi
[[ ! -e "$phase_path/parent-session-start-receipt.json" &&
  ! -e "$phase_path/parent-session-completion-receipt.json" &&
  ! -e "$provider_contact_log" ]] || {
  echo "rejected provider identity preflight consumed evidence or contacted AWS" >&2
  exit 1
}
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=execute \
MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST="$provider_entry_approval" \
  "$parent_session_runner" >"$provider_session_output"

provider_session_start="$phase_path/parent-session-start-receipt.json"
provider_session_completion="$phase_path/parent-session-completion-receipt.json"
[[ -f "$provider_session_start" && ! -L "$provider_session_start" &&
  -f "$provider_session_completion" && ! -L "$provider_session_completion" &&
  "$(<"$provider_contact_log")" == "aws" ]] || {
  echo "provider identity preflight omitted its exact provider or lifecycle proof" >&2
  exit 1
}
cmp -s "$provider_session_output" "$provider_session_completion" || {
  echo "provider identity preflight output did not match its completion receipt" >&2
  exit 1
}
provider_session_start_digest="$(jq -er '.receipt_digest' "$provider_session_start")"
jq -e \
  --arg provider_entry_approval "$provider_entry_approval" \
  --arg provider_session_start_digest "$provider_session_start_digest" \
  '
    .state == "started"
    and .external_aws_contact == false
    and .execution == {
      mode: "provider_identity_preflight",
      provider_entry_plan_digest: $provider_entry_approval,
      provider_state: "not_entered"
    }
    and .session == {start_receipt_digest: null}
    and .cleanup == {
      action: "none_before_provider_boundary",
      owner: "parent_controller",
      required: true,
      state: "installed",
      trap_count: 1
    }
  ' "$provider_session_start" >/dev/null || {
  echo "provider identity preflight start receipt weakened its boundary" >&2
  exit 1
}
jq -e \
  --arg provider_entry_approval "$provider_entry_approval" \
  --arg provider_session_start_digest "$provider_session_start_digest" \
  '
    .state == "provider_identity_verified"
    and .external_aws_contact == true
    and .execution == {
      mode: "provider_identity_preflight",
      provider_entry_plan_digest: $provider_entry_approval,
      provider_state: "identity_verified"
    }
    and .session == {start_receipt_digest: $provider_session_start_digest}
    and .cleanup == {
      action: "none_read_only_identity_preflight",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
    and (tostring | contains("123456789012") | not)
    and (tostring | contains("arn:aws") | not)
    and (tostring | contains("minco-rehearsal") | not)
    and (tostring | contains("/minco/rehearsal/database-url") | not)
  ' "$provider_session_completion" >/dev/null || {
  echo "provider identity preflight completion receipt weakened or exposed its boundary" >&2
  exit 1
}
rm -r -- "$evidence_root"
rm -f -- "$provider_contact_log"

failed_provider_entry_plan="$fixture_dir/failed-provider-entry-plan.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  "$controller_initializer" >/dev/null
controller_receipt="$evidence_root/control/controller-receipt.json"
controller_receipt_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  "$phase_beginner" >/dev/null
phase_path="$evidence_root/phases/01-prior-initial"
phase_start_receipt="$phase_path/phase-start-receipt.json"
phase_start_approval="$(jq -er '.receipt_digest' "$phase_start_receipt")"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=plan \
  "$parent_session_runner" >"$failed_provider_entry_plan"
failed_provider_entry_approval="$(
  shasum -a 256 "$failed_provider_entry_plan" | awk '{print $1}'
)"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_FAKE_AWS_IDENTITY_MODE=mismatch \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
  MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_receipt_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_approval" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
  MINCO_MULTI_RELEASE_PROVIDER_ACTION=execute \
  MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST="$failed_provider_entry_approval" \
  "$parent_session_runner" >/dev/null 2>&1; then
  echo "provider identity preflight accepted an unapproved role" >&2
  exit 1
fi
failed_provider_start="$phase_path/parent-session-start-receipt.json"
failed_provider_completion="$phase_path/parent-session-completion-receipt.json"
[[ -f "$failed_provider_start" && ! -L "$failed_provider_start" &&
  -f "$failed_provider_completion" && ! -L "$failed_provider_completion" &&
  "$(<"$provider_contact_log")" == "aws" ]] || {
  echo "failed provider identity preflight omitted conservative lifecycle proof" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
  "$failed_provider_completion" >/dev/null || {
  echo "failed provider identity receipt is outside the fixed policy" >&2
  exit 1
}
failed_provider_start_digest="$(jq -er '.receipt_digest' "$failed_provider_start")"
jq -e \
  --arg failed_provider_entry_approval "$failed_provider_entry_approval" \
  --arg failed_provider_start_digest "$failed_provider_start_digest" \
  '
    .state == "failed"
    and .external_aws_contact == true
    and .execution == {
      mode: "provider_identity_preflight",
      provider_entry_plan_digest: $failed_provider_entry_approval,
      provider_state: "identity_unverified"
    }
    and .session == {start_receipt_digest: $failed_provider_start_digest}
    and .cleanup == {
      action: "none_read_only_identity_preflight",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
    and (tostring | contains("123456789012") | not)
    and (tostring | contains("arn:aws") | not)
    and (tostring | contains("unapproved-role") | not)
  ' "$failed_provider_completion" >/dev/null || {
  echo "failed provider identity receipt underreported contact or exposed identity" >&2
  exit 1
}
rm -r -- "$evidence_root"
rm -f -- "$provider_contact_log"

first_phase_projection="$fixture_dir/first-phase-projection.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >"$first_phase_projection"
first_phase_evidence_path="$(jq -er '.evidence.path' "$first_phase_projection")"
mkdir -p "$first_phase_evidence_path"

PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >/dev/null || {
  echo "earlier phase evidence blocked the next exact-source handoff" >&2
  exit 1
}

if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted an authority digest outside the whole-run plan" >&2
  exit 1
fi

broadened_plan="$fixture_dir/broadened-plan.json"
jq '.cleanup.owner = "inner_phase"' "$plan" >"$broadened_plan"
broadened_digest="$(shasum -a 256 "$broadened_plan" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$broadened_plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$broadened_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted a digest-matched plan outside fixed policy" >&2
  exit 1
fi

broadened_review_plan="$fixture_dir/broadened-review-plan.json"
jq '.phases[1].change_set_review_policy = "operator_defined_update"' \
  "$plan" >"$broadened_review_plan"
broadened_review_digest="$(
  shasum -a 256 "$broadened_review_plan" | awk '{print $1}'
)"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$broadened_review_plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$broadened_review_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted an operator-defined change-set review policy" >&2
  exit 1
fi

traversal_root="$fixture_dir/missing/../current/target/minco/aws/reviewed-multi-release-run"
traversal_plan="$fixture_dir/traversal-plan.json"
jq --arg evidence_root "$traversal_root" \
  '.evidence_root = $evidence_root' "$plan" >"$traversal_plan"
traversal_digest="$(shasum -a 256 "$traversal_plan" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$traversal_plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$traversal_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted an evidence path traversing into a source checkout" >&2
  exit 1
fi

phase_evidence_path="$evidence_root/phases/03-prior-rollback"
mkdir -p "$phase_evidence_path"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted a pre-existing create-only evidence namespace" >&2
  exit 1
fi
rm -r -- "$evidence_root"

printf '# post-plan drift\n' >>"$current_root/minco.toml"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted source drift after whole-run planning" >&2
  exit 1
fi
git -C "$current_root" restore minco.toml

if (
  cd "$fixture_dir"
  plan_rehearsal prior "$current_root" >/dev/null 2>&1
); then
  echo "multi-release planning accepted a relative prior root" >&2
  exit 1
fi

if plan_rehearsal "$prior_root" "$prior_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted one checkout for both releases" >&2
  exit 1
fi

if plan_rehearsal \
  "$prior_root" "$current_root" \
  "$current_root/target/minco/aws/reviewed-multi-release-run" \
  >/dev/null 2>&1; then
  echo "multi-release planning accepted an evidence root inside a source checkout" >&2
  exit 1
fi

if plan_rehearsal \
  "$prior_root" "$current_root" "$traversal_root" \
  >/dev/null 2>&1; then
  echo "multi-release planning accepted traversal in the evidence root" >&2
  exit 1
fi

mkdir -p "$evidence_root"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a pre-existing whole-run evidence root" >&2
  exit 1
fi
rm -r -- "$evidence_root"

prior_link="$fixture_dir/prior-link"
ln -s "$prior_root" "$prior_link"
if plan_rehearsal "$prior_link" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a symlink checkout root" >&2
  exit 1
fi

if plan_rehearsal "$prior_root/nested" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a nested directory as a checkout root" >&2
  exit 1
fi

printf '# uncommitted\n' >>"$current_root/minco.toml"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a dirty current checkout" >&2
  exit 1
fi
git -C "$current_root" restore minco.toml

printf '# new revision\n' >>"$current_root/minco.toml"
git -C "$current_root" add minco.toml
git -C "$current_root" commit -qm "unexpected current revision"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a source revision outside its authority" >&2
  exit 1
fi

[[ ! -e "$provider_contact_log" ]] || {
  echo "a rejected multi-release plan contacted a provider or build command" >&2
  exit 1
}

printf 'Multi-release rehearsal plan checks passed.\n'
