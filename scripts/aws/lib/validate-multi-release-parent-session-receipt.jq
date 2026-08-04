def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def digest:
  type == "string" and test("^[0-9a-f]{64}$");

keys == [
  "authority",
  "cleanup",
  "controller",
  "execution",
  "external_aws_contact",
  "operation",
  "phase",
  "receipt_digest",
  "schema_version",
  "session",
  "state"
]
and .schema_version == 1
and .operation == "multi_release_parent_session"
and (
  .state == "started"
  or .state == "validated"
  or .state == "provider_identity_verified"
  or .state == "failed"
)
and (.external_aws_contact | type == "boolean")
and (.receipt_digest | digest)
and (.controller | keys) == ["plan_digest", "receipt_digest"]
and (.controller.plan_digest | digest)
and (.controller.receipt_digest | digest)
and (.authority | keys) == ["approval_digest", "kind", "run_id"]
and .authority.kind == "minco.aws-multi-release-controller-rehearsal.v1"
and (.authority.approval_digest | digest)
and (.authority.run_id | test("^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"))
and (.phase | keys) == [
  "change_set_review_policy",
  "evidence_namespace",
  "id",
  "projection_digest",
  "release",
  "source_revision",
  "stack_action",
  "start_receipt_digest"
]
and .phase.id == "01-prior-initial"
and .phase.release == "prior"
and (.phase.source_revision | revision)
and .phase.evidence_namespace == "phases/01-prior-initial"
and (.phase.projection_digest | digest)
and (.phase.start_receipt_digest | digest)
and .phase.stack_action == "create"
and .phase.change_set_review_policy == "bounded_create_v1"
and (.execution | keys) == [
  "mode",
  "provider_entry_plan_digest",
  "provider_state"
]
and (
  if .execution.mode == "validation_only" then
    (.state == "started" or .state == "validated")
    and .external_aws_contact == false
    and .execution.provider_entry_plan_digest == null
    and .execution.provider_state == "not_entered"
  elif .execution.mode == "provider_identity_preflight" then
    (.execution.provider_entry_plan_digest | digest)
    and (
      if .state == "started" then
        .external_aws_contact == false
        and .execution.provider_state == "not_entered"
      elif .state == "provider_identity_verified" then
        .external_aws_contact == true
        and .execution.provider_state == "identity_verified"
      elif .state == "failed" then
        .external_aws_contact == true
        and .execution.provider_state == "identity_unverified"
      else
        false
      end
    )
  else
    false
  end
)
and (.session | keys) == ["start_receipt_digest"]
and (
  if .state == "started" then
    .session.start_receipt_digest == null
  else
    (.session.start_receipt_digest | digest)
  end
)
and .cleanup.owner == "parent_controller"
and .cleanup.required == true
and .cleanup.trap_count == 1
and (
  if .state == "started" then
    .cleanup == {
      action: "none_before_provider_boundary",
      owner: "parent_controller",
      required: true,
      state: "installed",
      trap_count: 1
    }
  elif .state == "validated" then
    .cleanup == {
      action: "none_provider_boundary_not_entered",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
  else
    .cleanup == {
      action: "none_read_only_identity_preflight",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
  end
)
