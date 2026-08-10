# 1.2 published support matrix

Workspace and published install baseline: `1.2.2`
MSRV: Rust `1.97.1`
Compatibility state: the 1.0 framework boundary, separately qualified DynamoDB
descendant, agent-native layer, browser/native HTTP metadata, verified uploads,
rich mail, owned local services and release-bound evidence are published
together as the complete 33-package `v1.2.2` family. Registry, docs.rs, stable
documentation and application/live proof remain separate.

The `1.2.2` patch does not add a support claim. It hardens presentation of this
published matrix and keeps browser evidence explicit.

This matrix combines authoritative feature/plugin metadata, exercised golden
path recipes and the exact CGSP/GarmentIQ adoption reconciliation. It describes
what Minco currently has evidence to support and the evidence still required;
it is not a blanket production-readiness promise.

## How to read the state

- **catalog stable** is the repository's current plugin metadata
  classification. The frozen public boundary also follows the 1.x
  compatibility policy.
- **published beta** is opt-in and usable with pinned exact 1.2 packages. Its
  provider and production evidence remain bounded, but its published 1.x
  API/feature/CLI/schema boundary cannot break silently.
- **qualified descendant** identifies work, such as DynamoDB, added after the
  frozen M12-T06 measurement. It passed its own gates and is included in the
  published M12-T07 source without rewriting the earlier evidence snapshot.
- **application evidenced** means an exact downstream slice exercised the seam;
  it does not transfer product policy or deployment authority to Minco.
- **unsupported** means Minco intentionally makes no current promise.

## Framework and runtime matrix

| Boundary | Release state | Golden-path evidence | Application evidence | Explicit limit |
| --- | --- | --- | --- | --- |
| Static typed composition | 1.0 evidenced | graph, provenance, dependency and conformance suites | CGSP bounded platform layer; GarmentIQ can avoid composition entirely | no runtime scanning, dynamic libraries or global service locator |
| OpenAPI contract/profile | 1.0 evidenced | contract check/sync, deterministic bindings, compatibility diff | CGSP 64 operations; GarmentIQ 25 operations | no semantic business-compatibility guarantee |
| Resource wire convention | 1.0 evidenced; DynamoDB published beta | memory, Axum, SQLite, compiled PostgreSQL and all-five-port DynamoDB Orders slices | CGSP 30 operations across six families | no ORM, generic repository, generated business logic or relational DynamoDB emulation |
| Typed configuration | 1.0 evidenced | strict redacted config graph and generated applications | product configuration remains authoritative | values, secret-reference names and provider truth are not serialized into reports |
| Database lifecycle | published beta | real SQLite and disposable PostgreSQL migration/seed/verify suites | CGSP retains its own SQLx migrations and forced RLS | no startup migration; backend semantics, backups and data policy remain explicit |
| Axum/Tower HTTP conventions | 1.2 evidenced | in-process status/header/body/security tests plus exact browser/native response metadata and CORS projection | CGSP parity seam exists but deployed HTTP stays legacy | no second mobile API, application authorization or route ownership transfer |
| Native Lambda HTTP/API Gateway | published beta | Plan/SAM, package and bounded controller rehearsal | neither reviewed product adopts Minco's exact default topology | no container Lambda Function URL, Pulumi or arbitrary topology promise |
| SQS Lambda partial-batch worker | published beta; application evidenced | runtime and Plan tests | CGSP product record has staging execution; rollback rehearsal remains incomplete | Minco creates no queue, mapping, schedule or business handler |
| PostgreSQL adapter | published beta | provider-specific profiles, generated app, disposable integration | CGSP deliberately keeps product SQLx/RLS authority | no transparent provider equivalence or current-price guarantee |
| SQLite adapter | published beta | persistent-file lifecycle, transactions and feature isolation | no reviewed downstream production claim | no network, multi-instance, managed-backup or PostgreSQL-locking guarantee |
| DynamoDB Orders adapter | published beta; qualified descendant | standard SDK unit tests, explicit Plan/SAM/IAM and pinned Rustack five-port conformance with cleanup | no reviewed downstream or real-AWS claim | access-pattern-specific; GSI lists are eventually consistent; no SQL or generic repository |
| Plan/SAM model | 1.2 evidenced | schema/policy snapshots, topology-aware cost and ingress validation, IAM checks | CGSP consumes it only as advisory evidence | Function URLs remain declared but unsupported; not an infrastructure apply, live price or product controller |
| AWS deployment controller | published beta | exact-artifact apply/verify/promote/rollback/cleanup rehearsal | product deployment controllers remain separate | requires explicit account/region/change-set approval; no hidden mutation |
| Static-site intent/publication | published beta | local contract and exact-byte/hash receipt tests | products retain their own site controllers | DNS, certificate, CloudFront/S3 mutation and live-site proof are separate |
| Verified direct object uploads | 1.2 published beta | authorization-first issue/complete tests, bounded policy, exact S3 POST signing and cleanup boundaries | no reviewed downstream adoption claim | content safety, lifecycle and separately authorised live S3 proof remain application/provider responsibilities |
| Rich observable mail | 1.2 published beta | deterministic capture, loopback Mailpit, SES v2 submission and SNS/EventBridge normalization tests | no reviewed downstream mailbox-delivery claim | provider acceptance is not final mailbox delivery; no automatic retry after ambiguous submission |
| Release-bound feedback and handover | 1.2 evidenced | exact release/deployment binding, digest-approved create-only receipts, path/rollback and malformed-evidence tests | no reviewed client handover adoption claim | feedback is untrusted input; receipts do not authorize implementation or deployment; live provider and performance proof remain absent |
| Release/promotion/rollback receipts | 1.0 evidenced | immutable manifest/digest and exact-artifact rehearsal | product release manifests remain authoritative for product rollbacks | no rebuild during promotion; data compatibility still needs operator evidence |
| Owned local services | 1.2 evidenced; local only | loopback PostgreSQL, Rustack and Mailpit identity, lifecycle, recovery and persistent-data-preservation tests | no reviewed downstream adoption claim | never adopts or deletes foreign resources; no production-provider claim |
| Local project view, MCP and workbench | published beta; local only | bounded/redacted model tests, stdio MCP and desktop/mobile browser journeys | no product adoption required for the repository view | read-only, no arbitrary shell, no hosted control plane or write authority |
| Agent-native application development | 1.1+ published; local only | version-matched skills, digest-bound plan/sync, bounded context, doctor and deterministic cross-client scenario evaluation | no reviewed downstream adoption claim | no model invocation, implicit mutation authority, provider access or framework-only policy inheritance |
| Subscriber-only realtime | published beta | protocol, resync, Plan/SAM and failure-policy tests | no reviewed downstream live adoption | ephemeral invalidation only; not authoritative storage or guaranteed delivery |

## Official plugin and adapter metadata

The generated plugin catalog remains authoritative for exact dependencies,
capabilities, resources and metadata digests.

| Catalog classification | Components | Interpretation |
| --- | --- | --- |
| catalog stable | `health`, `observability`, `idempotency`, `feedback` | bounded declared contracts pass current gates; the reviewed 1.x Rust/CLI/serialized boundary follows SemVer |
| published beta plugins | `audit`, `events`, `identity`, `notifications`, `object-storage`, `sessions`, `static-site`, `realtime` | explicit opt-in with provider/failure/retention policy required |
| published beta adapters/runtimes | `aws-adapters`, `aws-dynamodb`, `aws-lambda`, `aws-worker`, `sqlx-postgres`, `sqlx-sqlite` | explicit provider/runtime selection; no default activation; DynamoDB remains application access-pattern-specific |

Memory/reference implementations are for tests and local development unless a
component explicitly documents durable production behavior. Catalog evidence
strings are inert metadata; validation never executes them or contacts a
provider.

## Recommended supported profiles

| Profile | Smallest selection | Evidence available | Remaining gate |
| --- | --- | --- | --- |
| Contract-only adoption | `default-features = false`, `contract` | both exact downstream applications | product contract and CI review |
| Local HTTP/application tests | `contract`, `http`, `test` | reference resource slice and Axum contract suites | product authorization and behavior tests |
| Local persistent SQLite | `sqlx-sqlite` | real file-backed lifecycle and generated app | product durability/concurrency fit |
| PostgreSQL application | `sqlx-postgres` | disposable adapter and generated-app qualification | chosen provider, connection budget, backups and live integration |
| DynamoDB Orders | `aws-dynamodb`, Orders `dynamodb`, `plan` | exact table/index contract and pinned Rustack all-five-port conformance | approved access-pattern fit, regional rates, backup/restore, quotas and separately approved real-AWS proof |
| Native AWS HTTP | `aws-lambda`, `plan`, selected adapters | local package/Plan/SAM plus bounded controller rehearsal | approved target and exact live verification |
| AWS SQS worker | `aws-worker`, `plan` | local runtime/Plan and CGSP staging evidence | application queue/mapping/IAM and rollback proof |
| Local AI/developer view | `minco-project-view`, optionally `minco-mcp` or `minco-workbench` | redaction, containment, protocol and browser evidence | remain local and read-only |

Prefer the smallest profile that closes one application boundary. Do not start
with `features = ["full"]` in a product unless its complete dependency/provider
surface is deliberately reviewed.

## Intentionally unsupported promises

- framework-owned product permissions, roles, workflows, schemas, tax,
  tenancy, RLS or migration policy;
- dynamic plugin discovery, runtime package installation, global service
  location or arbitrary shell execution;
- an ORM or generic CRUD repository, or relational semantics projected onto
  DynamoDB;
- hidden schedules, queues, event-source mappings, NAT gateways, fixed compute
  or provisioned concurrency;
- automatic production migrations or source rebuild during promotion;
- a hosted Minco control plane, hosted MCP/workbench, multi-cloud or arbitrary
  infrastructure-controller replacement;
- container-image Lambda Function URL and product-specific Pulumi topology as
  part of the default AWS profile;
- zero bill, current provider price, live deployment, provider delivery,
  rollback safety or production readiness inferred from local/hosted checks.

See the dated
[adoption reconciliation](../adoption/1.0-adoption-reconciliation-2026-08-05.md)
for exact application revisions and evidence limitations. The reviewed Rust,
Cargo feature, CLI, configuration, Plan, release and plugin commitment is in
the [compatibility policy](compatibility.md). Registry publication is complete;
application deployment remains separately authorized and evidenced.

The qualified-descendant DynamoDB boundary is described in the
[Orders DynamoDB profile](../deployment/dynamodb-orders.md).
