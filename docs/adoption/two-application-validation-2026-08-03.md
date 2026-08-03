# Two-application validation: CGSP and GarmentIQ

Date: 2026-08-03
Minco task: `M7-T01`
Verdict: initial comparison complete; two-application Minco adoption incomplete

This review tests Minco's framework boundaries against two real Rust/AWS web
applications. It does not turn either product into a framework fixture, infer
live state from source, or authorise changes outside this repository.

## Exact scope

| Evidence | Exact revision | State used by this review |
| --- | --- | --- |
| Minco main | `1430d0512fcd94e7fadea76bf2ab4f6d1aa391f1` | clean merged source after M11-T05 |
| Published Minco 0.6.0 | `v0.6.0` -> `2c4605b7d4abcd865035196ffc0484c4a0e82f1e` | crates.io family pinned by CGSP |
| CGSP | `020a18a837233ea7cc08d53f1c35fedcf6dfcb41` | exact `main@origin`; current dirty, stale local workspace excluded |
| GarmentIQ | `23a347f3ceb437a66f8d07b5db8bb652b8ab68d3` | exact `origin/main`; clean local branch is 13 commits ahead and excluded |

The product repositories were read only. No product test command was run because
build/test output would mutate those worktrees. Source inspection and existing
hosted evidence are labelled separately below.

## Outcome at a glance

| Boundary | CGSP | GarmentIQ | Framework conclusion |
| --- | --- | --- | --- |
| Contract | Minco 0.6.0 contract profile, 49 inventoried operations | canonical OpenAPI, 25 operations, zero Minco references | contract-only adoption is genuinely incremental |
| Code | domain/application remain framework-free; bounded platform layer selects Minco seams | domain is transport-light; API crate owns Axum and SQLx | keep product layering decisions outside core |
| Capabilities | static HTTP, Feedback, worker and resource helpers are selected explicitly | native providers and ports; no Minco graph | no runtime discovery or facade is justified |
| Resources | three complete Minco resource families; PostgreSQL/RLS remain product-owned | PostgreSQL and product migrations remain authoritative | resource conventions work without a generic repository |
| Deployment | Pulumi authoritative; Minco Plan/SAM advisory | CloudFormation, container Lambda Function URL, S3 and CloudFront | do not broaden Minco's default API Gateway/ZIP profile for one product |
| Evidence | exact inventory, CI, staged runtime records and explicit blockers | immutable release/rollback evidence and broad CI | evidence can interoperate while remaining application-owned |
| Removal | compatibility switches have owners and deletion gates; cleanup still blocked | no Minco dependency to remove | bridge removal needs two-app proof, not framework tests alone |

## CGSP slice

### Contract and code

CGSP pins `minco = "=0.6.0"` with `default-features = false` and the
`contract` feature at the workspace boundary. Its bounded `crates/platform`
layer opts into selected HTTP and plugin interfaces, while its worker opts into
`aws-worker`. `crates/domain` and `crates/application` contain no Minco, Axum,
SQLx, Lambda or AWS dependency.

The schema-1 operation inventory at the reviewed revision contains 49 exact
operations. Fifteen operations form three complete five-action resource
families: product categories, shopping carts and customer addresses. These
families use Minco's envelopes, bounded cursor lists, idempotent creates and
strong conditional writes while retaining use-case-shaped application ports,
transactional SQLx adapters and product authorization.

This is strong evidence for the thin resource convention. It is not evidence
for a generic CRUD repository, generated business behavior or framework-owned
permissions.

### Runtime and persistence

The source contains both the legacy and Minco HTTP middleware paths and wraps the
existing SQS processor with Minco's bounded partial-batch runtime. Existing
records explicitly keep the live HTTP selection on `legacy`. The complete HTTP
observation window and rollback rehearsal are outstanding. The worker has
deployment and health records, but its complete observation and mapping rollback
rehearsal are also outstanding.

PostgreSQL migrations, forced RLS, product transactions and Pulumi remain CGSP
authority. Minco Feedback uses an isolated PostgreSQL integration, but the
application has not transferred its general persistence lifecycle to Minco's
SQLx adapters. This is a deliberate boundary, not an adoption failure.

### Deployment, rollback and operations

Minco Plan/release output is advisory. It does not apply CGSP infrastructure or
replace the Pulumi deployment controller, preservation evidence or product
rollback policy. Compatibility switches name their owner, roll-forward,
rollback and deletion gates. Bridge deletion remains blocked by runtime
observation, rollback rehearsal, production recovery and product acceptance.

GitHub PR [xicv/CGSP#123](https://github.com/xicv/CGSP/pull/123) merged as the
reviewed CGSP revision. Its exact PR head passed the hosted `essential` and
`postgres` jobs. No workflow run was found on the merge SHA itself, so this
review records PR-head hosted evidence rather than claiming merge-SHA
requalification.

Historical product documents contain bounded live staging checks. They do not
prove the current exact CGSP revision, a current Minco HTTP observation window,
production recovery, or completed bridge removal.

## GarmentIQ slice

### Contract and code

The reviewed GarmentIQ tree has 25 OpenAPI operations and no file referencing
Minco. Its canonical contract, acceptance-first/TDD policy, Problem Details,
idempotency rules and provider ports are compatible with Minco's direction, but
compatibility is not adoption evidence.

The domain crate avoids Axum, SQLx, Lambda and AWS SDKs, although it currently
depends on `utoipa` for schema concerns. The API crate combines delivery and
SQLx persistence. Minco must not weaken its own dependency direction or invent
product ports to mirror that application structure.

### Persistence and deployment

GarmentIQ keeps PostgreSQL constraints, command functions, tenant scoping,
append-only history and forward migrations as product truth. Its AWS path uses
a container-image Lambda Function URL behind CloudFront, plus private S3 and
optional proof-only RDS/VPC resources. Minco's supported default remains native
ARM64 ZIP Lambda behind API Gateway HTTP API. Adding a second deployment runtime
solely to absorb this product would make the framework broader without improving
its core contract-to-cloud path.

Both systems avoid fixed application compute in their normal Lambda path, but
neither source inspection nor a CloudFormation template proves zero total cost.
CloudFront, S3, logs, container storage and databases retain their own cost
semantics.

### Rollback and operations

GarmentIQ has valuable application-owned release evidence: exact manifests,
database identity and preservation checks, an immutable authentication/edge
runtime contract, short-lived rollback approval and post-rollback verification.
Those are product evidence inputs, not generic Minco permissions or schemas.
Minco should continue accepting application evidence at release boundaries
rather than encoding GarmentIQ session names, protected tables or edge policy.

GitHub PR [xicv/garmentiq#50](https://github.com/xicv/garmentiq/pull/50)
merged as the reviewed GarmentIQ revision with its contract, Rust, PostgreSQL,
frontend, AWS static, recovery, cost and rollback checks green. Push workflows
on the merge SHA also passed the foundation and database-operation gates. No
Minco check ran, and no live/provider assertion was refreshed by this review.

## Framework decisions

The evidence supports four existing Minco decisions:

1. incremental contract-only adoption must remain possible without selecting a
   runtime, provider or database adapter;
2. resource API conventions should standardize the wire boundary while
   application ports, authorization and persistence stay product-owned;
3. deployment and rollback evidence must bind exact artifacts while allowing
   application-specific preservation and compatibility evidence;
4. Minco should stay narrow around its supported AWS profile instead of adding
   every topology already used by a product.

No new core abstraction is justified by this comparison. CGSP's business
permissions, RLS, Pulumi resources and observation policy remain CGSP policy.
GarmentIQ's tenancy, database commands, cookie/edge contract, container runtime
and protected-data set remain GarmentIQ policy.

## Remaining gap and next task

M7 cannot satisfy its milestone exit criterion until a real GarmentIQ slice uses
Minco. `M7-T02` therefore remains planned behind a separately authorised,
product-owned GarmentIQ change with the smallest useful scope:

- pin the published Minco 0.6.0 family with contract-only features;
- add a strict project manifest and bounded operation ownership evidence;
- run Minco contract check/sync/inspect alongside existing GarmentIQ CI;
- prove the resolved dependency graph selects no Minco HTTP, SQLx, Lambda, AWS
  or deployment provider;
- make rollback a source-only removal with no schema, data or AWS effect.

That product work must be reviewed and merged in GarmentIQ. The later Minco task
only records its exact revision and evidence. Until then, M7 stays `active`,
M12 adoption completion and the 1.0 compatibility freeze stay blocked, and no
two-application adoption claim is valid.

## Evidence states

| State | CGSP | GarmentIQ |
| --- | --- | --- |
| Exact source inspected | yes | yes |
| Local product tests in this review | not run; product repo remained read only | not run; product repo remained read only |
| Hosted source checks | PR-head essential + PostgreSQL passed | PR and merge-SHA foundation/database checks passed |
| Minco contract adoption | published 0.6.0 pin and inventory present | absent |
| Current exact live runtime proof | not present | not present |
| Current exact deployment proof | not performed | not performed |
| Current exact rollback rehearsal | incomplete for Minco bridges | native code contract exists; live rehearsal not established here |

No AWS API, database, deployment, product file, release, registry or public site
was changed during this validation.
