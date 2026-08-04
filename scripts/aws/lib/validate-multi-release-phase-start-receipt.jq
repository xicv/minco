def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def digest:
  type == "string" and test("^[0-9a-f]{64}$");

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
  "stack_action"
]
and (.phase.source_revision | revision)
and (.phase.projection_digest | digest)
and (
  if .phase.id == "01-prior-initial" then
    .phase.release == "prior"
    and .phase.evidence_namespace == "phases/01-prior-initial"
    and .phase.stack_action == "create"
    and .phase.change_set_review_policy == "bounded_create_v1"
  elif .phase.id == "02-current" then
    .phase.release == "current"
    and .phase.evidence_namespace == "phases/02-current"
    and .phase.stack_action == "update"
    and .phase.change_set_review_policy == "bounded_release_update_v1"
  elif .phase.id == "03-prior-rollback" then
    .phase.release == "prior"
    and .phase.evidence_namespace == "phases/03-prior-rollback"
    and .phase.stack_action == "update"
    and .phase.change_set_review_policy == "bounded_release_update_v1"
  else
    false
  end
)
and .cleanup == {
  inner_phase_cleanup: false,
  owner: "parent_controller",
  required: true
}
