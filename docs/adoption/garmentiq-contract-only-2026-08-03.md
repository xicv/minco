# GarmentIQ contract-only adoption evidence

Date: 2026-08-03
Minco task: `M7-T02`
Verdict: bounded downstream adoption qualified and merged

This record evaluates one separately authorised GarmentIQ change against
Minco's published contract-only surface. It does not claim that Minco owns the
product runtime, persistence, business policy, AWS topology, deployment or
release process.

## Exact revisions

| Evidence | Exact revision | State |
| --- | --- | --- |
| Minco task base | `5ea607e7eebb` | merged `main` after M7-T01 |
| Published Minco | `v0.6.0` -> `2c4605b7d4abcd865035196ffc0484c4a0e82f1e` | immutable Git tag and published registry family |
| GarmentIQ base | `23a347f3ceb437a66f8d07b5db8bb652b8ab68d3` | PR #55 base |
| GarmentIQ implementation | `697b98f32dba555d17f6f23f8df156f8b3650e44` | locally and initially hosted qualified source |
| GarmentIQ qualified head | `2262349989b0df8ad2d202092666e8aaed012b10` | evidence-complete PR head |
| GarmentIQ merge | `8d6e8146a3954db11c64b264f1990f5b853c3192` | current merged product source |

Minco 0.6.0 release PR #73 qualified head
`13840cb4dc507037e8d7fc7fbf66bc59597f91c1` and merged as the tagged commit.
Hosted release run `30688694186`, merged-main qualification run `30689722134`
and trusted publication run `30690519946` passed. The publication run verified
all 28 exact, non-yanked registry packages and external consumers. That release
evidence is distinct from the GarmentIQ adoption evidence below.

## Dependency boundary

GarmentIQ pins the facade exactly at the workspace boundary:

```toml
minco = { version = "=0.6.0", default-features = false, features = ["contract"] }
```

Only `garmentiq-api` selects it, and only as a development dependency for the
contract integration test. The lockfile resolves these Minco packages:

- `minco 0.6.0`, checksum
  `10c6c7713e63a4f44c146b155a711db351d6e7026464975577858a972864647e`;
- `minco-contract 0.6.0`, checksum
  `daed6c1d828e4a9259b8a501d03c126686242ea182145d2c79f40a9696b4a695`;
- `minco-core 0.6.0`, checksum
  `c6d01a00eb98cd28b33344ad1d7df47594bcce90bed61565378f341f117d457d`.

The project-owned dependency guard traverses the complete Cargo metadata
closure rooted at `minco`. It rejects any other Minco package plus Axum, SQLx,
Rusqlite, Tokio PostgreSQL, Lambda runtimes and AWS SDK/configuration packages.
Its four unit tests prove a contract-only pass, runtime-closure rejection,
exact-version rejection and isolation from unrelated application runtime
dependencies.

The merged `garmentiq-domain` manifest selects only Chrono, rust_decimal,
Serde, UUID and Utoipa. It selects no Minco, Axum, SQLx, Lambda or AWS package.
GarmentIQ's API crate still owns its pre-existing Axum and SQLx dependencies;
their existence outside Minco's closure is product architecture, not framework
adoption leakage.

## Contract and operation ownership

`crates/api/tests/minco_contract.rs` calls the public
`minco::contract::load_contract` API on GarmentIQ's canonical OpenAPI document.
The test fails on any policy finding and asserts an exact set of 25 operation
IDs, preventing same-count substitutions. It separately asserts the six
operations whose existing `Idempotency-Key` contract is represented by
`x-minco-idempotent: true`:

- `captureDeliveryProof`;
- `createExportJob`;
- `createGarment`;
- `createSupportRequest`;
- `scanGarmentIntoServiceArea`;
- `updateGarment`.

The adoption made previously implicit additive-object compatibility explicit
with `additionalProperties: true` and a rationale. It documented the existing
`/health` shared-secret failure as a `403` Problem response. These are contract
truth corrections, not new runtime behavior. Generated TypeScript types were
updated deterministically; application-owned delivery refinements changed from
`Omit` aliases to interface extension so known fields remain visible alongside
the honest open-object index signature.

No Minco project manifest, runtime bootstrap, database adapter, plugin graph or
deployment configuration was added. For this slice, the public contract library
is the thinner integration than introducing a second project configuration
solely to locate one already-canonical OpenAPI file.

## TDD and verification

The product task used a red-green-refactor sequence around the public contract
test and dependency guard. Local qualification at implementation commit
`697b98f32dba555d17f6f23f8df156f8b3650e44` passed:

- `cargo test -p garmentiq-api --test minco_contract --locked`;
- four dependency-boundary unit tests and the actual Cargo metadata check;
- format, workspace check, Clippy with warnings denied and workspace tests;
- `make ci-local`, including current generated API types, Oxc, TypeScript,
  production build and 38 Playwright merge-gate journeys.

The local database-backed HTTP suite was not run because no disposable database
URL was supplied. The final PR head closed that evidence gap in hosted CI:

| Boundary | Exact run | Result |
| --- | --- | --- |
| Foundation, contract, frontend, Rust and PostgreSQL HTTP | `30789612854` | passed |
| Database migration, baseline and guarded reset | `30789612786` | passed |
| Browser cookie, exact-origin and CSRF | `30789613094` | passed |
| External subject, API and browser session lifecycle | `30789613088` | passed |
| Disposable PostgreSQL restore rehearsal | `30789612778` | passed |

The remaining cost, recovery, rollback, staging-readiness and environment-policy
checks on that exact head also passed. PR #55 had no review comments or reviews,
was conflict-free, and was merged with an exact-head guard. On merge SHA
`8d6e8146a3954db11c64b264f1990f5b853c3192`, database-operations run
`30789802106` passed and foundation run `30789802054` requalified the exact
merged tree. Its deployment job remained intentionally skipped.

## Removal and authority

Removal is source-only: delete the API dev dependency, contract test,
dependency-boundary scripts and CI/lint hook, then regenerate the lockfile.
The explicit OpenAPI compatibility and Problem-response documentation can stay
as product contract truth; removing Minco does not require reverting it. No
database migration, data rewrite, provider rollback, AWS action or deployment
is involved.

GarmentIQ retains authority for authentication, authorization, tenancy,
idempotency implementation, PostgreSQL transactions and migrations, browser
sessions, protected data, CloudFormation, container Lambda, CloudFront, S3,
release manifests and rollback. Minco supplies only the selected contract
policy surface.

## Framework conclusion

The second real application confirms that Minco can be adopted incrementally
without transferring runtime or infrastructure authority. Together with CGSP,
the evidence supports the existing thin resource and contract conventions and
does not justify a generic CRUD repository, runtime service locator, dynamic
plugin scanning, second AWS deployment topology or product-specific core type.

The M7 two-application exit criterion is now evidenced. Compatibility freeze,
release, deployment and live-provider decisions remain separate later work.

## Evidence states

| State | Result |
| --- | --- |
| Exact merged product source inspected | yes |
| Published Minco package identity | exact 0.6.0 tag and registry family |
| Product-local contract and dependency tests | passed at implementation commit |
| Exact PR-head hosted source checks | passed |
| Exact merge-SHA hosted source checks | passed |
| Live AWS/provider verification for this adoption | not performed |
| Product deployment or release | not performed |
| Minco release, tag or registry mutation | not performed |

No product file, database, AWS resource, deployment, release, registry entry or
documentation site was changed by this Minco evidence task.
