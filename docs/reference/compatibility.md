# Compatibility policy and 1.x release line

Freeze reviewed: 2026-08-05

Published release: `1.8.0`

Published comparison baseline: `1.5.0`

Current workspace: `1.8.0` published

MSRV: Rust `1.97.1`

This page retains the public boundary frozen at Minco `1.0.0` and applies it to
the 1.1 through published 1.8.0 release lines. Merge, tag,
registry, GitHub release and
versioned documentation are independently verified. Deployment and proof in a
consuming application remain separate release gates.

The published `1.3.0` minor adds the opt-in Waffo crate, facade feature, typed
service/client/transport surfaces, CLI group and Agent Skill. Existing APIs and
defaults remain compatible; the provider integration is disabled by default.

The 1.4.0 release changes release/package/documentation identities and
dependency implementations only. It does not change a public signature,
serialized contract, CLI command, diagnostic identity, feature meaning,
provider selection or deployment topology. Official descriptors advance to
`^1.4.0`; third-party `^1.3.0` descriptors remain SemVer-compatible unless they
deliberately require a later API.

The 1.5.0 release adds public, test-only fake types to five existing
packages and packages measured local assurance plus topology-cost regression.
Existing runtime interfaces, serialized data, CLI commands, diagnostics,
features, provider selection and deployment topology remain compatible.
Official descriptors advance in lock-step to `^1.5.0`; third-party `^1.4.0`
descriptors remain SemVer-compatible unless they deliberately require a
1.5-only public fake.

The 1.6.0 release adds the versioned V2 audit contract, ledger services and
derived DynamoDB audit plan without adding fields or variants to previously
exhaustively constructible public types. Existing audit interfaces, CLI names,
serialized Plan inputs, defaults and provider selection remain compatible.
Official descriptors advanced in lock-step to `^1.6.0`; older compatible
third-party descriptors remain valid unless they require the V2 surface.

The 1.7.0 release changes fresh `auto` local-service selection to prefer a
ready Apple Container runtime. It preserves explicit runtime selection,
receipt and exact-resource precedence, Docker fallback, public APIs, Plan IR,
production topology and provider semantics. Official descriptors advance in
lock-step to `^1.7.0`; see the
[1.6.0-to-1.7.0 guide](../adoption/1.6.0-to-1.7.0.md).

The 1.8.0 release adds opt-in streaming, multipart, private range-download,
conditional metadata and HTTP lifecycle contracts to object storage. Existing
buffering and single-upload APIs remain available, production topology defaults
do not change, and application use cases retain authorization, state, logical
identity, retention and inspection. Official descriptors advance in lock-step
to `^1.8.0`; see the
[1.7.0-to-1.8.0 guide](../adoption/1.7.0-to-1.8.0.md).

The 1.2 release adds a defaulted `FeedbackThread.clarifications` serialized
field. Stored JSON remains data-compatible, but downstream Rust code that uses
an exhaustive `FeedbackThread { ... }` literal must add
`clarifications: Vec::new()` or use a public constructor. This source impact is
documented in the 1.1-to-1.2 upgrade guide; it must not be mistaken for a fully
source-compatible struct-shape change.

## Frozen boundary

The generated references are the exhaustive inventory authorities. The freeze
applies to every rustdoc-visible public item in the
[34 publishable packages](generated/packages.md), all named package features,
the complete [CLI](generated/cli.md), the generated configuration and Plan
[schemas](generated/schemas.md), the plugin [distribution contract](generated/plugins.md),
and the generated [diagnostic codes](generated/diagnostics.md).

| Boundary | Frozen 1.x commitment | Authority |
|---|---|---|
| Rust API | every rustdoc-visible public item in a publishable package follows Rust SemVer; private items are not frozen | package source and rustdoc |
| Cargo features | feature names, meaning, default membership and dependency selection are compatibility surfaces | package manifests and [feature reference](generated/features.md) |
| CLI | command paths, arguments, requiredness, accepted values, exit meaning and machine-readable output are stable | Clap declarations and [CLI reference](generated/cli.md) |
| Configuration | schema-1 paths, kinds, required/secret classification, precedence and defaults are stable | typed configuration and [schema reference](generated/schemas.md) |
| Plan IR | schema-1 API-only input and schema-2 trigger-aware input remain accepted; validation and canonical projections are stable | `minco-plan`, ADR-0019 and [schema reference](generated/schemas.md) |
| Release and deployment evidence | release manifest schema 3 and schema-1 deployment, change-set, review, cleanup, hosted-verification, static-site, promotion and canary records are digest- and meaning-stable | `minco-release` and `minco-deploy-aws` public types |
| Plugin distribution | strict archive-visible schema 1, static descriptor overlap and fail-closed unknown versions remain stable | ADR-0027 and [plugin reference](generated/plugins.md) |
| Local project view | `ProjectView`, MCP results and workbench reports/exports remain read-only and schema 1 | ADR-0030 and the three local-tool crates |
| Diagnostics | codes and their meaning are stable; prose may become clearer without changing the code | [diagnostic reference](generated/diagnostics.md) |
| External API and behavior | documented HTTP shapes, safety ordering, redaction and fail-closed behavior remain compatibility boundaries | OpenAPI, contract tests and the supported matrix |

Catalog `stable` and `beta` describe component maturity and provider evidence.
They do not weaken this compatibility promise: after 1.0, a beta component may
remain opt-in and operationally limited, but its published Rust, feature, CLI
and serialized surfaces still follow this policy.

The freeze makes no promise for the intentionally unsupported surfaces in the
[support matrix](supported-matrix.md#intentionally-unsupported-promises),
including framework-owned business policy, runtime plugin discovery, generic
CRUD/ORM semantics, a hosted control plane, automatic production migration,
arbitrary infrastructure topology, current provider pricing or live
production readiness. Adding one of those promises requires its own design and
evidence; it is not implied by a stable type or successful local check.

No `#[deprecated]`, `#[doc(hidden)]`, `#[non_exhaustive]`, unstable feature or
nightly feature marker exists in the release. Consequently, the current
public structs and enums are exact shapes unless Rust itself makes an addition
compatible. New fields or variants usually need a versioned sibling type or a
major release; component maturity is not an escape hatch. The only retained
pre-1.0 CLI compatibility paths are documented in the
[0.7-to-1.0 guide](../adoption/0.7.0-to-1.0.0.md).

## Feature and dependency measurements

The facade exposes 28 named features. Three other publishable packages expose
named feature sets: `minco-aws-adapters` has 9,
`minco-plugin-feedback` has 8 and `minco-aws-worker` has 2. No package exposes
an unstable or nightly feature. The exact names and edges are generated in the
[feature reference](generated/features.md).

The following measurements use `scripts/measure_adoption.py` and distinct
normal dependency package names from `cargo tree --locked -p minco`. Feature
tree lines are deterministic dependency-shape evidence, not build-time or
binary-size budgets.

| Facade selection | 0.6.0 normal packages | 1.0.0 normal packages | delta | 0.6.0 feature lines | 1.0.0 feature lines | delta |
|---|---:|---:|---:|---:|---:|---:|
| `--no-default-features` | 16 | 16 | 0 | 81 | 81 | 0 |
| defaults | 105 | 105 | 0 | 820 | 825 | +5 |
| defaults plus `official-plugins` | 118 | 119 | +1 | 1,040 | 1,062 | +22 |
| `--all-features` | 290 | 298 | +8 | 3,351 | 3,462 | +111 |

The default facade remains `contract`, `http` and `default-plugins`; changing
that membership after 1.0 is breaking even when Cargo can still resolve the
graph. Applications should keep selecting the smallest supported profile and
should not use `full` as an unreviewed default.

## Audit evidence and limits

The audit used
[`cargo-semver-checks 0.50.0`](https://github.com/obi1kenobi/cargo-semver-checks/releases/tag/v0.50.0),
released 2026-08-01, on Rust `1.97.1`. The task-prescribed command was run
exactly:

```text
cargo semver-checks --workspace --all-features
```

It returned exit 101 after comparing the published packages because
`minco-mcp` has no `0.6.0` registry version. The other first-publication
packages are `minco-plugin-realtime`, `minco-project-view` and
`minco-workbench`. This is recorded as a missing registry baseline, not a pass.
Those four packages are instead covered by current rustdoc, compiler, test,
feature and manual public-shape review before their first publication.

For published packages, the default `0.6.0` to `0.7.0` comparison is a
pre-1.0 major transition, so the tool correctly skips breaking-change lints.
The audit therefore also forced a hypothetical minor boundary and excluded
only the four first-publication packages:

```text
cargo semver-checks --workspace --all-features \
  --exclude minco-plugin-realtime \
  --exclude minco-project-view \
  --exclude minco-mcp \
  --exclude minco-workbench \
  --release-type minor
```

That command returned the expected exit 100 and found four packages with
pre-1.0 breaking changes: 18 fields added to exhaustively constructible public
structs and `StaticSitePublisher::publish` replaced by
`publish_manifest`. Every finding is covered in the
[0.6-to-0.7 migration guide](../adoption/0.6.0-to-0.7.0.md). All other
published packages passed 196 applicable checks each; 58 currently
inapplicable lints were skipped per package.

The checker is evidence, not proof. Its official documentation notes that it
cannot detect every type, generic, lifetime, behavioral or feature-subset
break. The freeze therefore also requires the complete compiler/test/doc
matrix, generated-reference drift check, manual serialized-schema inventory
and application evidence.

## Post-1.0 versioning rules

These project rules apply Cargo's authoritative
[Rust compatibility](https://doc.rust-lang.org/stable/cargo/reference/semver.html),
[feature](https://doc.rust-lang.org/stable/cargo/reference/features.html) and
[MSRV](https://doc.rust-lang.org/stable/cargo/reference/rust-version.html)
guidance to Minco's stricter serialized and operational boundaries.

| Boundary | Patch release | Minor release | Major release required |
|---|---|---|---|
| Rust | compatible correctness/docs changes | additive items that do not break exhaustive matching, trait implementations or inference | remove/rename/signature change, required trait item, incompatible trait bound, or exhaustive type change |
| Cargo | dependency patching that preserves selected surface | new opt-in feature or compatible dependency support | remove/rename feature, change its meaning, or change default membership |
| CLI | prose/help correction with the same behavior | additive command, optional argument or accepted value | remove/rename path or option, add a required argument, change default/exit meaning, or repurpose input |
| Serialized data | byte/meaning-preserving bug fix | new schema version while every supported old reader/input path remains available | mutate an existing strict schema, digest payload or meaning, or retire a supported schema |
| Diagnostics | clearer message with the same code and meaning | add a new code | remove, reuse or change the meaning/severity class of a code |
| Behavior | fix implementation to match the documented contract without weakening safety | opt-in additive capability with explicit evidence | change authorization, validation order, status/wire meaning, redaction, durability, failure or rollback semantics |
| MSRV | no increase | increase only through an explicit decision, changelog entry and full qualification | unannounced increase or an increase that invalidates the stated release line |

Strict schemas reject unknown fields, and receipt digests cover their canonical
payloads. An apparently additive field is therefore not silently compatible:
introduce a new schema version, keep an explicit old reader or migration path,
and document its retirement boundary. Removing an old schema requires a major
release unless that schema was never public or published.

The MSRV for 1.x is Rust `1.97.1`. A 1.x patch cannot raise it. A 1.x minor may
raise it only through an explicit compatibility decision and release note; the
repository's toolchain, package metadata, generated references and full matrix
must move together.

## Compatibility reports

Minco exposes two deterministic, read-only reports:

```text
cargo minco contract diff --against <revision> --json
cargo minco upgrade report --json
```

They provide evidence for an upgrade review. Neither command proves semantic
business behavior, deployment safety, persisted-data compatibility or
rollback safety.

## Contract diff

`contract diff` validates the current contract and the contract stored at the
requested JJ or Git revision. It reads the historical file through the VCS and
does not check out or modify the working copy. Revisions are bounded to simple
names, commit IDs and ancestry expressions; option-like or shell-shaped input
is rejected.

Both inputs use Minco's constrained OpenAPI loader. Local `#/...` references
are resolved recursively with cycle protection. External or unresolved
references are reported as `uncertain`; they are never silently accepted.

The schema-1 JSON report includes the two source identifiers and SHA-256
digests, an aggregate `classification`, sorted operation/schema changes,
evidence for every change and explicit limitations. Aggregation uses this
precedence:

```text
breaking > uncertain > non_breaking
```

`non_breaking` only means that the bounded classifier found no breaking or
uncertain structural change. It is not a behavioral compatibility guarantee.
The command succeeds after producing a valid report even when its
classification is `breaking`; automation must inspect `classification`.

### Bounded classifications

| Change | Classification |
|---|---|
| Add/remove operation | non-breaking / breaking |
| Change operation method or path | breaking |
| Require/remove authentication | breaking / uncertain |
| Require/remove the idempotency contract | breaking / breaking |
| Add/remove component schema | non-breaking / breaking |
| Add/remove a type constraint | breaking / uncertain |
| Change an existing type | breaking |
| Add/remove an enum constraint | breaking / uncertain |
| Add/remove an enum value | uncertain / breaking |
| Add optional property | non-breaking |
| Add required property or remove property | breaking |
| Remove required marker | uncertain |
| Change a recognized validation constraint | uncertain |
| Change unclassified operation/schema structure | uncertain |

Descriptions, summaries, examples, tags and other documentation-only operation
fields are ignored. Request/response direction can change the meaning of
otherwise similar schema edits, so the classifier uses `uncertain` where a
direction-independent answer would overstate evidence.

## Application upgrade report

`upgrade report` inventories the application boundaries consumed by release
notes and migration guides:

- application and CLI Rust minimum versions;
- running CLI version and the application's declared Minco requirement;
- selected Cargo features and default-feature policy;
- configuration field names, kinds and secret/required flags;
- plugin catalog schema, selections and linked descriptor versions;
- manifest, deployment-plan and OpenAPI schema/version identifiers;
- stable diagnostics and report limitations.

The command reads `minco.toml` as versioned data before strict manifest
loading. An unsupported manifest schema therefore produces a stable warning
and the remaining available evidence instead of preventing the report itself.
Project-declared files must still resolve to ordinary files inside the project.

Configuration defaults and values are excluded. Secret-reference names and
secret values are never serialized. The overall assessment remains
`review_required`: the report is an inventory against the running CLI, not a
replacement for release notes, compilation, application tests or runtime
verification.

## Upgrade workflow

1. Run `cargo minco upgrade report --json` before changing the Minco version.
2. Save the report with the release-review evidence.
3. Update the exact dependency and selected feature set.
4. Run the report again and compare schema-1 fields and diagnostic codes.
5. Run `cargo minco contract diff --against <reviewed-revision> --json`.
6. Review every `breaking` and `uncertain` item against request/response use.
7. Run contract, compiler, application, adapter and deployment-plan checks.
8. Treat migrations, live deployment and promotion as separate approvals.
