---
id: M14-T35
title: Correct DynamoDB audit transaction permissions
milestone: M14
status: completed
priority: critical
area: deployment/audit
depends_on: [M14-T34]
operations: []
owned_paths:
  - CHANGELOG.md
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/tests/multi_runtime.rs
  - docs/adrs/0043-durable-audit-ledger.md
  - docs/deployment/dynamodb-orders.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/generated/schemas.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T35-audit-dynamodb-dependent-actions.md
  - verification/1.7-performance-baseline.json
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/release-identity.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plan --test multi_runtime explicit_dynamodb_table_renders_on_demand_indexes_environment_and_exact_iam --locked
  - cargo test -p minco-plan --all-features --locked
  - cargo clippy -p minco-plan --all-targets --all-features --locked -- -D warnings
  - scripts/docs/generate-reference.sh --check
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - ./scripts/quality.sh
---

# M14-T35 - Correct DynamoDB audit transaction permissions

## Goal

Correct the generated audit-table IAM plan and SAM policy so DynamoDB audit
transactions receive the item-operation authorization required by every
transaction member.

## Acceptance

- an audit transaction containing `Put` members receives exact-table
  `dynamodb:PutItem` alongside `dynamodb:TransactWriteItems`;
- the ordinary Orders table permission set remains unchanged and no wildcard or
  standalone item-write path is introduced;
- Plan IR and rendered SAM regressions reject the previously generated
  four-action audit-table policy;
- documentation explains that the permission authorizes transactional `Put`
  members even though the adapter never calls standalone `PutItem`; and
- no provider contact, application deployment, production mutation or audit
  payload access occurs during this source correction.

## Evidence

Started from exact `main@origin`
`e9ccecec41c528446b59374cb23467c91696b682` after CGSP staging update 39
demonstrated that AWS rejects the incomplete four-action audit-table policy,
then rebased onto post-evidence-closure `main`
`250ba5f7f2322fc5c518b5d26b1402eacdea5328` before qualification. The
downstream no-write capability probe and table-scoped compatibility policy
remain the live release boundary until a reviewed published Minco version
includes this correction.

Current AWS transaction-IAM documentation confirms that a transactional `Put`
is authorized through the underlying `dynamodb:PutItem` action. The focused
regression failed against the old four-action projection, then passed with the
shared five-action Plan IR/SAM set. All 27 `minco-plan` unit tests and 53
multi-runtime tests passed, and warning-denying all-target/all-feature clippy
passed. The full repository `./scripts/quality.sh` run reached and passed its
final source-manifest verification; exact source-manifest, release-identity,
generated-reference and operational-evidence checks were then repeated on the
settled tree. Operational evidence remains `PASS` with truthful warnings for
absent current live-provider and hosted-performance evidence. No AWS request,
deployment, production mutation or audit payload access was performed.
