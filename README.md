<p align="center">
  <img src="docs/assets/minco-icon.svg" width="180" alt="Minco mascot: a purple robot cloud" />
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

> Published baseline: `0.5.0`
>
> Current workspace version: `0.6.0`
>
> Workspace release state: `candidate`
>
> Current publishable package count: `28`

## Documentation

Read the [versioned Minco documentation](https://xicv.github.io/minco/), or
start directly with:

- [Build your first API](https://xicv.github.io/minco/0.5.0/tutorials/first-api)
- [Build a resource API](https://xicv.github.io/minco/0.5.0/how-to/resource-api)
- [Deploy to AWS](https://xicv.github.io/minco/0.5.0/tutorials/deploy-to-aws)
- [CLI reference](https://xicv.github.io/minco/0.5.0/reference/cli)
- [Zero idle, precisely](https://xicv.github.io/minco/0.5.0/explanation/zero-idle)
- [Preview the 0.6.0 candidate](https://xicv.github.io/minco/0.6.0/)

Repository-native decisions, operational detail, and release evidence remain
under [`docs/`](docs/), [`docs/DECISIONS.md`](docs/DECISIONS.md), and
[`VERIFICATION.md`](VERIFICATION.md).

## Quick start

Install the exact stable control plane:

```bash
rustup toolchain install 1.97.1 --component clippy,rustfmt
cargo +1.97.1 install cargo-minco --version 0.5.0 --locked
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
cargo add minco@0.5.0

# PostgreSQL API on native Lambda
cargo add minco@0.5.0 --features sqlx-postgres,aws-lambda,plan,release,test

# Provider-neutral core only
cargo add minco@0.5.0 --no-default-features
```

## The resource API convention

Minco 0.5.0 includes an opt-in, OpenAPI-first CRUD convention without adding an
ORM or generic repository:

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

## Static plugin distribution and conformance

The `0.6.0` candidate adds strict, archive-visible plugin distribution records
and one public offline conformance kit. Metadata can be inspected without
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
[`0.6.0` plugin guide](https://xicv.github.io/minco/0.6.0/guides/plugin-conformance).

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

The coordinated 28-crate `0.5.0` family is published from immutable tag
[`v0.5.0`](https://github.com/xicv/minco/releases/tag/v0.5.0). Registry
ownership, archive checksums, the CLI installation, default/no-default/all-
feature consumers, and all exact docs.rs routes were independently verified.

The workspace is a `0.6.0` candidate with the same package inventory. Do not
request it from crates.io until independent registry verification confirms the
complete family.

See [`CHANGELOG.md`](CHANGELOG.md),
[`docs/adoption/0.4.0-to-0.5.0.md`](docs/adoption/0.4.0-to-0.5.0.md),
[`docs/adoption/0.5.0-to-0.6.0.md`](docs/adoption/0.5.0-to-0.6.0.md), and
[`docs/development/publishing.md`](docs/development/publishing.md).

## License

Minco is released under the [MIT License](LICENSE).
