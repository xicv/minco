# Candidate support matrix

Workspace candidate: `0.7.0`
Published install baseline: `0.6.0`
MSRV: Rust `1.97.1`
Compatibility state: candidate evidence, not yet the M12-T04 1.0 freeze

This matrix combines authoritative feature/plugin metadata, exercised golden
path recipes and the exact CGSP/GarmentIQ adoption reconciliation. It describes
what Minco currently has evidence to support and the evidence still required;
it is not a blanket production-readiness promise.

## How to read the state

- **catalog stable** is the repository's current plugin metadata
  classification. It does not pre-empt the 1.0 API freeze.
- **candidate beta** is opt-in, checked in the source candidate and usable with
  pinned versions, but its public/provider boundary may still change in M12-T04.
- **application evidenced** means an exact downstream slice exercised the seam;
  it does not transfer product policy or deployment authority to Minco.
- **unsupported** means Minco intentionally makes no current promise.

## Framework and runtime matrix

| Boundary | Candidate state | Golden-path evidence | Application evidence | Explicit limit |
| --- | --- | --- | --- | --- |
| Static typed composition | candidate evidenced | graph, provenance, dependency and conformance suites | CGSP bounded platform layer; GarmentIQ can avoid composition entirely | no runtime scanning, dynamic libraries or global service locator |
| OpenAPI contract/profile | candidate evidenced; published in 0.6.0 | contract check/sync, deterministic bindings, compatibility diff | CGSP 64 operations; GarmentIQ 25 operations | no semantic business-compatibility guarantee |
| Resource wire convention | candidate evidenced | memory, Axum, SQLite and compiled PostgreSQL reference slices | CGSP 30 operations across six families | no ORM, generic repository, generated business logic or DynamoDB emulation |
| Typed configuration | candidate evidenced | strict redacted config graph and generated applications | product configuration remains authoritative | values, secret-reference names and provider truth are not serialized into reports |
| Database lifecycle | candidate beta | real SQLite and disposable PostgreSQL migration/seed/verify suites | CGSP retains its own SQLx migrations and forced RLS | no startup migration; backend semantics, backups and data policy remain explicit |
| Axum/Tower HTTP conventions | candidate evidenced | in-process status/header/body/security contract tests | CGSP parity seam exists but deployed HTTP stays legacy | no application authorization or route ownership transfer |
| Native Lambda HTTP/API Gateway | candidate beta | Plan/SAM, package and bounded controller rehearsal | neither reviewed product adopts Minco's exact default topology | no container Lambda Function URL, Pulumi or arbitrary topology promise |
| SQS Lambda partial-batch worker | candidate beta; application evidenced | runtime and Plan tests | CGSP product record has staging execution; rollback rehearsal remains incomplete | Minco creates no queue, mapping, schedule or business handler |
| PostgreSQL adapter | candidate beta | provider-specific profiles, generated app, disposable integration | CGSP deliberately keeps product SQLx/RLS authority | no transparent provider equivalence or current-price guarantee |
| SQLite adapter | candidate beta | persistent-file lifecycle, transactions and feature isolation | no reviewed downstream production claim | no network, multi-instance, managed-backup or PostgreSQL-locking guarantee |
| Plan/SAM model | candidate evidenced | schema/policy snapshots, cost and IAM checks | CGSP consumes it only as advisory evidence | not an infrastructure apply, live price or product controller |
| AWS deployment controller | candidate beta | exact-artifact apply/verify/promote/rollback/cleanup rehearsal | product deployment controllers remain separate | requires explicit account/region/change-set approval; no hidden mutation |
| Static-site intent/publication | candidate beta | local contract and exact-byte/hash receipt tests | products retain their own site controllers | DNS, certificate, CloudFront/S3 mutation and live-site proof are separate |
| Release/promotion/rollback receipts | candidate evidenced | immutable manifest/digest and exact-artifact rehearsal | product release manifests remain authoritative for product rollbacks | no rebuild during promotion; data compatibility still needs operator evidence |
| Local project view, MCP and workbench | candidate beta; local only | bounded/redacted model tests, stdio MCP and desktop/mobile browser journeys | no product adoption required for the repository view | read-only, no arbitrary shell, no hosted control plane or write authority |
| Subscriber-only realtime | candidate beta | protocol, resync, Plan/SAM and failure-policy tests | no reviewed downstream live adoption | ephemeral invalidation only; not authoritative storage or guaranteed delivery |

## Official plugin and adapter metadata

The generated plugin catalog remains authoritative for exact dependencies,
capabilities, resources and metadata digests.

| Catalog classification | Components | Interpretation |
| --- | --- | --- |
| catalog stable | `health`, `observability`, `idempotency`, `feedback` | bounded declared contracts pass current gates; 1.0 Rust/CLI/serialized freeze is still pending |
| candidate beta plugins | `audit`, `events`, `identity`, `notifications`, `object-storage`, `sessions`, `static-site`, `realtime` | explicit opt-in with provider/failure/retention policy required |
| candidate beta adapters/runtimes | `aws-adapters`, `aws-lambda`, `aws-worker`, `sqlx-postgres`, `sqlx-sqlite` | explicit provider/runtime selection; no default activation |

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
for exact application revisions and evidence limitations. M12-T04 must review
and freeze the Rust, Cargo feature, CLI, configuration, Plan, release and plugin
surfaces before this candidate becomes a 1.0 compatibility commitment.

