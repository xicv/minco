<p align="center">
  <img src="docs/assets/minco-icon.svg" width="180" alt="Minco connected runtime mark" />
</p>

<h1 align="center">Minco</h1>

<p align="center"><strong>Minimal cost, maximum capability.</strong></p>

Minco is the contract-to-cloud Rust framework for building, operating, and
evolving low-idle-cost web applications through one inspectable application
graph. OpenAPI is canonical, business logic stays ordinary Rust, plugins are
statically linked, SQL remains visible, and AWS resources, cost, deployment,
and evidence stay connected.

The default AWS profile uses API Gateway HTTP API and a native ARM64 Lambda. It
contains no NAT Gateway, provisioned concurrency, scheduled poller, or
always-on application compute. Storage, retained logs, DNS, secrets, database
storage, schedules, requests, and other residual dimensions remain explicit.

> Published baseline: `1.10.0`
>
> Current workspace version: `1.11.0`
>
> Workspace release state: `candidate`
>
> Current publishable package count: `36`

The complete 36-package 1.10.0 family is published from immutable tag
`v1.10.0`, including trusted publishers, Pages and docs.rs proof. The 1.11.0
workspace is an unpublished additive candidate for contract-enforced request
validation; use exact 1.10.0 packages until registry publication is verified.

## Documentation

Read the [versioned Minco documentation](https://xicv.github.io/minco/), or
start directly with:

- [Build your first API](https://xicv.github.io/minco/1.10.0/getting-started/first-application)
- [Protect traffic at the gateway](https://xicv.github.io/minco/1.10.0/guides/traffic-policy)
- [Build a resource API](https://xicv.github.io/minco/1.10.0/guides/resource-api)
- [Deploy to AWS](https://xicv.github.io/minco/1.10.0/guides/deployment)
- [CLI reference](https://xicv.github.io/minco/1.10.0/reference/cli)
- [Generated package, feature, plugin, CLI, schema, and diagnostic reference](docs/reference/generated/index.md)
- [Plugin conformance](https://xicv.github.io/minco/1.10.0/guides/plugin-conformance)
- [Zero idle, precisely](https://xicv.github.io/minco/1.10.0/explanation/zero-idle)
- [Develop with Codex and Claude](https://xicv.github.io/minco/1.10.0/guides/agent-development)
- [Integrate Waffo hosted payments](https://xicv.github.io/minco/1.10.0/guides/payments-waffo)
- [Operate durable auditing](https://xicv.github.io/minco/1.10.0/guides/auditing)
- [Use Apple-first local services](https://xicv.github.io/minco/1.10.0/guides/local-development)
- [Transfer files directly](https://xicv.github.io/minco/1.10.0/guides/files-and-static-sites)
- [Use portal-first Ticketing](https://xicv.github.io/minco/1.10.0/guides/ticketing)
- [Preview contract-enforced request validation](https://xicv.github.io/minco/1.11.0/guides/contract-request-validation)

Repository-native decisions, operational detail, and release evidence remain
under [`docs/`](docs/), [`docs/DECISIONS.md`](docs/DECISIONS.md), and
[`VERIFICATION.md`](VERIFICATION.md).

## Quick start

Install the exact stable control plane:

```bash
rustup toolchain install 1.97.1 --component clippy,rustfmt
cargo +1.97.1 install cargo-minco --version 1.10.0 --locked
```

Generate and inspect a layered SQLite application:

```bash
cargo minco new hello-minco --database sqlite
cd hello-minco
cp .env.example .env
cargo minco contract check
cargo minco inspect --json
cargo minco check --with-cargo
```

Applications normally depend on the feature-gated facade:

```bash
cargo add minco@1.10.0

# PostgreSQL API on native Lambda
cargo add minco@1.10.0 --features sqlx-postgres,aws-lambda,plan,release,test

# Provider-neutral core only
cargo add minco@1.10.0 --no-default-features
```

## Agent-native application development

The `1.11.0` candidate packages nine focused, version-matched workflow skills
for Codex and Claude Code. Relevant skills teach generated request validation,
typed extraction, coarse authorization, safe correlation IDs and explicit
body-limit/timeout provenance while retaining every earlier boundary. The mandatory cumulative
changelog-to-skill freshness gate remains in force. Minco
plans project-local projections before writing, requires
the exact plan digest to synchronize them, and preserves user-owned
instructions and client configuration.

```bash
cargo minco agent plan --target all --json
cargo minco agent sync --target all --expect-plan-digest <sha256> --json
cargo minco agent doctor --target all --json
cargo minco agent context --operation placeOrder --json
cargo minco agent eval --target all --json
```

Context and evaluation are bounded, read-only projections over authoritative
Minco project facts. They do not invoke a model, contact a provider, run a task,
or grant commit, release, deployment, database, or production authority. See
the [1.11.0 candidate agent development guide](https://xicv.github.io/minco/1.11.0/guides/agent-development).

Release qualification also verifies cumulative feature coverage, current
versioned documentation, skill markers and a byte-identical deterministic
Codex/Claude workflow receipt. That is release-content evidence, not a model
quality score or mutation authority. Application-specific model evaluation and
measured human-review effort remain `NOT RUN` for this release.

## The resource API convention

Minco 1.11.0 retains the opt-in, OpenAPI-first CRUD convention without adding
an ORM or generic repository:

| Action | Success | Control |
|---|---|---|
| Create | `201` with `{ "data": ... }` | idempotency key, location, strong ETag |
| List | `200` with `data` and `page` | bounded opaque cursor, allowlisted sort/filter |
| Read | `200` with `{ "data": ... }` | strong ETag |
| Update | `200` with `{ "data": ... }` | required strong `If-Match` |
| Delete | `204` with no body | required strong `If-Match` |

Errors use `application/problem+json` with stable codes and request IDs.
Authorization, validation, domain invariants, audit, retention, deletion
policy, and transaction boundaries remain in application use cases.

## Contract-enforced requests

The 1.11.0 candidate can generate bounded semantic request checks directly from
reviewed OpenAPI when `x-minco-request-validation: generated` is selected.
`ValidatedJson`, `ValidatedQuery` and `ValidatedPath` extract once; a separate
generated policy enforces exact coarse permissions and scopes before one use
case. Structural failures are `400`, decoded assertion failures are bounded
`422`, and business authorization remains application-owned. See the
[candidate guide](https://xicv.github.io/minco/1.11.0/guides/contract-request-validation).

## Durable action auditing

The `1.6.0` release adds a schema-agnostic, append-only audit contract without
introducing an ORM. Application use cases produce semantic actions and adapters
commit them with the domain mutation: SQL profiles use a transactional source
journal and a separate ledger, while the Orders DynamoDB profile uses one
cross-table transaction and a separate retained audit table. History is
permission-gated, cursor-bounded and privacy-aware.

Audit storage does not silently rotate at a byte threshold. SQLite can seal
explicit bounded segments, PostgreSQL normally uses time partitions, and
DynamoDB can retain a hot horizon before a separately proven archive. Storage,
PITR, relationship fanout and archive costs stay visible. See the
[1.10.0 auditing guide](https://xicv.github.io/minco/1.10.0/guides/auditing).

## Gateway traffic and response compression

The `1.9.0` release adds an opt-in API Gateway HTTP traffic policy and a
hardened negotiated response-compression boundary. Both remain additive,
application-owned and free of new topology; see the
[1.10.0 traffic guide](https://xicv.github.io/minco/1.10.0/guides/traffic-policy)
and
[compression guide](https://xicv.github.io/minco/1.10.0/guides/http-compression).

## Direct object transfers

The `1.8.0` release added an opt-in authenticated JSON control plane for
direct single/multipart upload, immutable replacement, private full/range
download, stop/resume and conditional private-cache metadata. Large bytes go
directly to private storage; the application still owns authorization, quotas,
durable sessions, logical pointers, retention and content inspection. S3 is
the production-targeted byte plane, while non-S3 providers must implement the
additive streaming/signing/multipart contracts. See the
[1.10.0 file guide](https://xicv.github.io/minco/1.10.0/guides/files-and-static-sites).

## Static plugin distribution and conformance

The published `1.10.0` release includes strict, archive-visible plugin distribution
records and one public offline conformance kit. Metadata can be inspected without
loading plugin code; it never enables a crate or replaces explicit Cargo
dependencies and typed constructor registration.

```bash
cargo minco plugin list --json
cargo minco plugin validate --json
cargo minco plugin test --all --json
```

Passing conformance proves the declared package and, when supplied, concrete
composition behavior. Application, provider/live, deployment and production
readiness remain distinct evidence states. See the
[`1.10.0` plugin guide](https://xicv.github.io/minco/1.10.0/guides/plugin-conformance).

## Core guarantees

- **Contract first:** reviewed OpenAPI operations, schemas, examples, security,
  success responses, and Problems precede implementation.
- **Strong boundaries:** dependencies point `delivery → application → domain`;
  application ports are use-case-shaped and handlers contain no SQL.
- **Static capabilities:** plugins use typed services and explicit selection;
  there is no runtime scanning or global service locator.
- **AWS native:** Lambda/API Gateway events, SDK configuration, IAM intent,
  SAM/CloudFormation rendering, and secret references remain standard.
- **Cost aware:** wake sources, connection pressure, retained resources, and
  pricing confidence stay visible.
- **Build once:** promotion uses the exact verified artifact and manifest; it
  never rebuilds source.
- **AI native:** stable paths, JSON inspection, diagnostics, tasks, checked-in
  generation, and exact evidence support humans and coding agents alike.
- **JJ first:** one isolated workspace owns one task, with colocated Git for
  GitHub transport.

## Architecture boundary

Minco is intentionally narrow. It provides a deep AWS-native path from contract
to operation; it does not recreate Laravel’s runtime model, Active Record,
dynamic package discovery, or a hosted control plane. Domain and application
crates do not depend on Axum, SQLx, Lambda, AWS SDKs, or Minco deployment
internals. The composition root alone selects concrete adapters and runtimes.

The accepted product definition and 1.0 completion boundary are in
[`docs/vision/minco-framework-definition.md`](docs/vision/minco-framework-definition.md).

## Development

Before changing code, read [`AGENTS.md`](AGENTS.md), the relevant ADR, roadmap,
and owning task. Useful inspection commands are:

```bash
cargo minco inspect --json
cargo minco explain <operationId> --json
cargo minco task show <id> --json
cargo minco deploy plan --stdout --json
```

Run the authoritative local gate before finishing:

```bash
./scripts/quality.sh
jj log -r 'conflicts()'
```

Local tests, hosted qualification, package dry run, registry publication, live
deployment, promotion, and production runtime are separate evidence states.

## Release

The coordinated 36-crate `1.10.0` family is published from immutable tag
[`v1.10.0`](https://github.com/xicv/minco/releases/tag/v1.10.0) at exact qualified
commit `2075b60b8fe86c04d3c8289d71eb8293a39fc378`. Independent registry validation
found all 36 exact versions present and non-yanked after the guarded
dependency-ordered upload and two rate-limit-safe, registry-proven resumes. Source, hosted qualification, tag, GitHub release,
registry, docs.rs, stable documentation, AWS deployment and production runtime
remain separately verified evidence states; no live provider, AWS application
or production mutation was part of this crate release.

See [`CHANGELOG.md`](CHANGELOG.md),
[`docs/adoption/0.4.0-to-0.5.0.md`](docs/adoption/0.4.0-to-0.5.0.md),
[`docs/adoption/0.5.0-to-0.6.0.md`](docs/adoption/0.5.0-to-0.6.0.md), and
[`docs/adoption/0.6.0-to-1.0.0.md`](docs/adoption/0.6.0-to-1.0.0.md), and
[`docs/adoption/1.0.0-to-1.1.0.md`](docs/adoption/1.0.0-to-1.1.0.md), and
[`docs/adoption/1.1.0-to-1.2.0.md`](docs/adoption/1.1.0-to-1.2.0.md), and
[`docs/adoption/1.2.0-to-1.2.1.md`](docs/adoption/1.2.0-to-1.2.1.md), and
[`docs/adoption/1.2.1-to-1.2.2.md`](docs/adoption/1.2.1-to-1.2.2.md), and
[`docs/adoption/1.2.2-to-1.3.0.md`](docs/adoption/1.2.2-to-1.3.0.md), and
[`docs/adoption/1.3.0-to-1.4.0.md`](docs/adoption/1.3.0-to-1.4.0.md), and
[`docs/adoption/1.4.0-to-1.5.0.md`](docs/adoption/1.4.0-to-1.5.0.md), and
[`docs/adoption/1.5.0-to-1.6.0.md`](docs/adoption/1.5.0-to-1.6.0.md), and
[`docs/adoption/1.6.0-to-1.7.0.md`](docs/adoption/1.6.0-to-1.7.0.md), and
[`docs/adoption/1.7.0-to-1.8.0.md`](docs/adoption/1.7.0-to-1.8.0.md), and
[`docs/adoption/1.8.0-to-1.9.0.md`](docs/adoption/1.8.0-to-1.9.0.md), and
[`docs/adoption/1.9.0-to-1.10.0.md`](docs/adoption/1.9.0-to-1.10.0.md), and
[`docs/development/publishing.md`](docs/development/publishing.md).

## License

Minco is released under the [MIT License](LICENSE).
