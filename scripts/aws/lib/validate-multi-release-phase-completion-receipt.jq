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
  "result",
  "schema_version",
  "state",
  "transition"
]
and .schema_version == 1
and .operation == "multi_release_phase_completion"
and .state == "succeeded"
and .external_aws_contact == true
and (.receipt_digest | digest)
and (.controller | keys) == ["plan_digest", "receipt_digest"]
and (.controller.plan_digest | digest)
and (.controller.receipt_digest | digest)
and (.authority | keys) == ["approval_digest", "kind", "run_id"]
and .authority.kind == "minco.aws-multi-release-controller-rehearsal.v1"
and (.authority.approval_digest | digest)
and (.authority.run_id | test("^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"))
and (.phase | keys) == [
  "evidence_id",
  "id",
  "projection_digest",
  "source_revision",
  "start_receipt_digest"
]
and (.phase.source_revision | revision)
and (.phase.projection_digest | digest)
and (.phase.start_receipt_digest | digest)
and .phase.evidence_id == .phase.id
and (.result | keys) == [
  "artifacts",
  "cleanup",
  "external_aws_contact",
  "operation",
  "phase",
  "receipt_digest",
  "rollback",
  "schema_version",
  "state",
  "verification"
]
and (.result.receipt_digest | digest)
and .result.schema_version == 1
and .result.operation == "multi_release_phase_result"
and .result.state == "succeeded"
and .result.external_aws_contact == true
and .result.phase == {
  evidence_id: .phase.evidence_id,
  id: .phase.id,
  source_revision: .phase.source_revision
}
and (.result.artifacts | keys) == [
  "change_set_receipt_digest",
  "deployment_receipt_digest",
  "hosted_verification_digest",
  "migration_plan_digest",
  "migration_receipt_digest",
  "promotion_receipt_digest",
  "release_manifest_digest"
]
and ([.result.artifacts[]] | all(digest))
and .result.verification == {
  fresh: true,
  historical_report_reused: false
}
and .result.cleanup == {
  owner: "parent_controller",
  performed: false
}
and .cleanup == {
  deferred: true,
  owner: "parent_controller"
}
and (.transition | keys) == [
  "next_phase",
  "previous_phase_completion_digest"
]
and (
  if .phase.id == "01-prior-initial" then
    .result.rollback == {
      assessment_digest: null,
      exact_initial_release_reused: false,
      reused_release_manifest_digest: null
    }
    and .transition == {
      next_phase: "02-current",
      previous_phase_completion_digest: null
    }
  elif .phase.id == "02-current" then
    .result.rollback == {
      assessment_digest: null,
      exact_initial_release_reused: false,
      reused_release_manifest_digest: null
    }
    and (.transition.previous_phase_completion_digest | digest)
    and .transition.next_phase == "03-prior-rollback"
  elif .phase.id == "03-prior-rollback" then
    (.result.rollback.assessment_digest | digest)
    and .result.rollback.exact_initial_release_reused == true
    and (.result.rollback.reused_release_manifest_digest | digest)
    and .result.rollback.reused_release_manifest_digest
      == .result.artifacts.release_manifest_digest
    and (.transition.previous_phase_completion_digest | digest)
    and .transition.next_phase == null
  else
    false
  end
)
